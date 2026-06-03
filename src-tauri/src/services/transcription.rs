use std::path::Path;
use std::sync::{Arc, Mutex};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::errors::AppError;

/// 16 kHz samples per Whisper audio-context frame. After the encoder's
/// stride-2 conv, one frame spans 20 ms = 320 samples at 16 kHz
/// (1500 frames = the full 30 s window).
const SAMPLES_PER_AUDIO_CTX_FRAME: usize = 320;

/// Frames of headroom added to a provisional buffer's `audio_ctx` so the tail
/// isn't truncated by the encoder's receptive field (~1.3 s). Lower for more
/// speed; raise if the end of provisional captions gets clipped.
const PROVISIONAL_AUDIO_CTX_MARGIN: i32 = 64;

/// A single Whisper segment with its start/end timestamps (centiseconds
/// relative to the beginning of the audio buffer).
pub struct TimestampedSegment {
    pub text: String,
    /// Start time in centiseconds (10 ms units) relative to buffer start.
    pub start_cs: i64,
    /// End time in centiseconds (10 ms units) relative to buffer start.
    pub end_cs: i64,
}

/// Wraps a lazily-loaded whisper.cpp model and exposes a blocking
/// `transcribe_batch()` method.  The model is loaded on the first call
/// and reused for every subsequent call (loading takes ~1-2 s, so we
/// only want to pay that cost once).
pub struct TranscriptionService {
    /// `None` until the first transcription request triggers model loading.
    /// Protected by a Mutex so multiple flush tasks can't race on init.
    context: Mutex<Option<WhisperContext>>,
}

// SAFETY: WhisperContext is Send + Sync per whisper-rs docs.
// Mutex<Option<WhisperContext>> is therefore Send + Sync.
unsafe impl Send for TranscriptionService {}
unsafe impl Sync for TranscriptionService {}

impl TranscriptionService {
    pub fn new() -> Self {
        Self {
            context: Mutex::new(None),
        }
    }

    /// Load the Whisper model from disk if it hasn't been loaded yet.
    /// Subsequent calls are a no-op (the context is already `Some`).
    fn ensure_loaded(&self, model_path: &Path) -> Result<(), AppError> {
        let mut guard = self.context.lock().map_err(|_| AppError::LockPoisoned("Whisper context lock poisoned".into()))?;
        if guard.is_some() {
            return Ok(());
        }

        let path_str = model_path
            .to_str()
            .ok_or_else(|| AppError::InvalidModel("Invalid model path encoding".into()))?;
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(path_str, params)
            .map_err(|e| AppError::TranscriptionFailed(format!("Failed to load model: {e}")))?;
        *guard = Some(ctx);
        Ok(())
    }

    /// Build Whisper decoding parameters.
    ///
    /// - Provisional segments use greedy sampling (best_of=3) for low latency.
    /// - Finalized segments use beam search (beam_size=5) for higher accuracy.
    /// - Temperature fallback (0.0 → 0.2 → … → 1.0) retries on garbled output.
    fn decode_params(
        initial_prompt: Option<&str>,
        is_provisional: bool,
        audio_ctx: Option<i32>,
    ) -> FullParams<'static, 'static> {
        let strategy = if is_provisional {
            SamplingStrategy::Greedy { best_of: 3 }
        } else {
            SamplingStrategy::BeamSearch {
                beam_size: 5,
                patience: -1.0,
            }
        };
        let mut params = FullParams::new(strategy);

        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Temperature fallback: re-decode with increasing temperature when
        // output entropy or avg log-probability indicate a bad decode.
        params.set_temperature(0.0);
        params.set_temperature_inc(0.2);
        params.set_entropy_thold(2.4);
        params.set_logprob_thold(-1.0);

        // Suppress blank tokens at the start of sampling.
        // Note: suppress_nst is left at its default (false) — whisper.cpp
        // explicitly disabled it because it caused hallucinations at the
        // end of audio segments.
        params.set_suppress_blank(true);

        // Provisional passes cap the encoder's audio context to ~the buffer
        // length — the dominant CPU cost. Finalized passes pass `None` so they
        // keep whisper's default full 30 s context for accuracy.
        if let Some(ac) = audio_ctx {
            params.set_audio_ctx(ac);
        }

        if let Some(prompt) = initial_prompt {
            params.set_initial_prompt(prompt);
        }
        params
    }

    const NO_SPEECH_THRESHOLD: f32 = 0.6;
    /// Segments with average token log-probability below this are likely
    /// hallucinations ("Thank you", "Thanks for watching", etc.) and are
    /// discarded. This is the same threshold OpenAI uses in their Whisper
    /// pipeline.
    const AVG_LOGPROB_THRESHOLD: f32 = -1.0;

    fn extract_segments(state: &whisper_rs::WhisperState) -> Vec<TimestampedSegment> {
        let mut segments = Vec::new();
        for i in 0..state.full_n_segments() {
            if let Some(segment) = state.get_segment(i) {
                if segment.no_speech_probability() > Self::NO_SPEECH_THRESHOLD {
                    continue;
                }
                // Reject low-confidence segments (likely hallucinations).
                let n = segment.n_tokens();
                if n > 0 {
                    let sum_logprob: f32 = (0..n)
                        .filter_map(|t| segment.get_token(t).map(|tok| tok.token_data().plog))
                        .sum();
                    let avg_logprob = sum_logprob / n as f32;
                    if avg_logprob < Self::AVG_LOGPROB_THRESHOLD {
                        continue;
                    }
                }
                if let Ok(s) = segment.to_str() {
                    let text = s.trim().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    segments.push(TimestampedSegment {
                        text,
                        start_cs: segment.start_timestamp(),
                        end_cs: segment.end_timestamp(),
                    });
                }
            }
        }
        segments
    }

    /// Run Whisper inference on multiple 16 kHz mono f32 PCM buffers using a
    /// single GPU state allocation to avoid repeated Metal init/free cycles.
    ///
    /// Returns per-segment results with Whisper timestamps for each buffer,
    /// enabling proper interleaving of segments from different sources.
    ///
    /// `prompts` provides an optional initial prompt per buffer (e.g. the tail
    /// of the previous transcription) to give Whisper linguistic context across
    /// window boundaries.
    ///
    /// When `is_provisional` is true, uses greedy decoding for lower latency;
    /// otherwise uses beam search for higher accuracy.
    ///
    /// **Blocking** — call from `spawn_blocking`.
    pub fn transcribe_batch(
        &self,
        model_path: &Path,
        buffers: &[&[f32]],
        prompts: &[Option<String>],
        is_provisional: bool,
    ) -> Result<Vec<Vec<TimestampedSegment>>, AppError> {
        self.ensure_loaded(model_path)?;

        let guard = self.context.lock().map_err(|_| AppError::LockPoisoned("Whisper context lock poisoned".into()))?;
        let ctx = guard
            .as_ref()
            .ok_or_else(|| AppError::TranscriptionFailed("Context not loaded".into()))?;

        // Full encoder context for this model (1500 frames = 30 s); provisional
        // buffers shrink below this in proportion to their length.
        let max_audio_ctx = ctx.n_audio_ctx();

        let mut state = ctx
            .create_state()
            .map_err(|e| AppError::TranscriptionFailed(e.to_string()))?;

        let mut results = Vec::with_capacity(buffers.len());
        for (i, samples) in buffers.iter().enumerate() {
            let prompt = prompts.get(i).and_then(|p| p.as_deref());

            // Provisional (throwaway, display-only) passes cap the encoder
            // context to ~the buffer length; finalized passes use full context.
            let audio_ctx = is_provisional.then(|| {
                let frames =
                    samples.len().div_ceil(SAMPLES_PER_AUDIO_CTX_FRAME) as i32;
                (frames + PROVISIONAL_AUDIO_CTX_MARGIN).clamp(1, max_audio_ctx)
            });

            state
                .full(Self::decode_params(prompt, is_provisional, audio_ctx), samples)
                .map_err(|e| AppError::TranscriptionFailed(e.to_string()))?;
            results.push(Self::extract_segments(&state));
        }
        Ok(results)
    }
}

/// Tauri managed-state wrapper.  The `Arc` allows cloning a handle into
/// spawned Tokio tasks that outlive the command handler's borrow.
pub struct TranscriptionState {
    pub service: Arc<TranscriptionService>,
}

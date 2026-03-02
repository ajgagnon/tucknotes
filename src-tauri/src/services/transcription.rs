use std::path::Path;
use std::sync::{Arc, Mutex};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::errors::AppError;

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

        if let Some(prompt) = initial_prompt {
            params.set_initial_prompt(prompt);
        }
        params
    }

    const NO_SPEECH_THRESHOLD: f32 = 0.6;

    fn extract_text(state: &whisper_rs::WhisperState) -> String {
        let mut text = String::new();
        for i in 0..state.full_n_segments() {
            if let Some(segment) = state.get_segment(i) {
                if segment.no_speech_probability() > Self::NO_SPEECH_THRESHOLD {
                    continue;
                }
                if let Ok(s) = segment.to_str() {
                    text.push_str(s);
                }
            }
        }
        text.trim().to_string()
    }

    /// Run Whisper inference on multiple 16 kHz mono f32 PCM buffers using a
    /// single GPU state allocation to avoid repeated Metal init/free cycles.
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
    ) -> Result<Vec<String>, AppError> {
        self.ensure_loaded(model_path)?;

        let guard = self.context.lock().map_err(|_| AppError::LockPoisoned("Whisper context lock poisoned".into()))?;
        let ctx = guard
            .as_ref()
            .ok_or_else(|| AppError::TranscriptionFailed("Context not loaded".into()))?;

        let mut state = ctx
            .create_state()
            .map_err(|e| AppError::TranscriptionFailed(e.to_string()))?;

        let mut results = Vec::with_capacity(buffers.len());
        for (i, samples) in buffers.iter().enumerate() {
            let prompt = prompts.get(i).and_then(|p| p.as_deref());
            state
                .full(Self::decode_params(prompt, is_provisional), samples)
                .map_err(|e| AppError::TranscriptionFailed(e.to_string()))?;
            results.push(Self::extract_text(&state));
        }
        Ok(results)
    }
}

/// Returns true if the final transcription text is a structural marker
/// rather than real speech (e.g. `[BLANK_AUDIO]`, `(buzzing)`).
pub fn is_low_quality_output(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.len() < 2 {
        return true;
    }
    (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('(') && trimmed.ends_with(')'))
}

/// Tauri managed-state wrapper.  The `Arc` allows cloning a handle into
/// spawned Tokio tasks that outlive the command handler's borrow.
pub struct TranscriptionState {
    pub service: Arc<TranscriptionService>,
}

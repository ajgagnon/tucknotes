use std::path::Path;
use std::sync::{Arc, Mutex};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::errors::AppError;

/// Wraps a lazily-loaded whisper.cpp model and exposes a blocking
/// `transcribe()` method.  The model is loaded on the first call and
/// reused for every subsequent call (loading takes ~1-2 s, so we only
/// want to pay that cost once).
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
        let mut guard = self.context.lock().map_err(|_| AppError::LockPoisoned)?;
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

    /// Run Whisper inference on a buffer of 16 kHz mono f32 PCM samples.
    ///
    /// This is a **blocking, CPU-intensive** call — callers should run it
    /// inside `tokio::task::spawn_blocking` to avoid starving the async
    /// runtime.
    ///
    /// `model_path` is only used on the first invocation to load the model;
    /// after that it's ignored (the cached context is reused).
    pub fn transcribe(&self, model_path: &Path, samples: &[f32]) -> Result<String, AppError> {
        self.ensure_loaded(model_path)?;

        let guard = self.context.lock().map_err(|_| AppError::LockPoisoned)?;
        let ctx = guard.as_ref().ok_or_else(|| {
            AppError::TranscriptionFailed("Context not loaded".into())
        })?;

        // A WhisperState holds the per-inference scratch buffers.
        // Creating one is cheap; it borrows the heavy weights from the context.
        let mut state = ctx
            .create_state()
            .map_err(|e| AppError::TranscriptionFailed(e.to_string()))?;

        // Configure the decoding pass:
        //   - Greedy decoding (pick the single best token at each step)
        //   - English language
        //   - Suppress all diagnostic output from whisper.cpp
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Run the full pipeline: PCM → mel spectrogram → encoder → decoder → text
        state
            .full(params, samples)
            .map_err(|e| AppError::TranscriptionFailed(e.to_string()))?;

        // Collect the decoded text from all segments whisper produced.
        // A "segment" is typically a sentence or clause boundary that
        // whisper.cpp detected in the audio.
        let num_segments = state.full_n_segments();
        let mut text = String::new();
        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                if let Ok(s) = segment.to_str() {
                    text.push_str(s);
                }
            }
        }
        Ok(text.trim().to_string())
    }
}

/// Tauri managed-state wrapper.  The `Arc` allows cloning a handle into
/// spawned Tokio tasks that outlive the command handler's borrow.
pub struct TranscriptionState {
    pub service: Arc<TranscriptionService>,
}

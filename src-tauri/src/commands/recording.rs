use tauri::Emitter;

use crate::errors::AppError;
use crate::models::{AudioChunkEvent, AudioSource, RecordingState};

#[tauri::command]
pub async fn start_recording(
    state: tauri::State<'_, RecordingState>,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let mut guard = state.capture.lock().map_err(|_| AppError::LockPoisoned)?;
        if guard.is_some() {
            return Err(AppError::CaptureFailed("Already recording".into()));
        }

        let (capture, mut rx) = crate::services::audio_capture::AudioCapture::start()
            .map_err(|e| AppError::CaptureFailed(e.to_string()))?;
        *guard = Some(capture);
        drop(guard);

        tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                let source = match chunk.source {
                    AudioSource::SystemAudio => "system",
                    AudioSource::Microphone => "microphone",
                };
                let rms = if chunk.pcm_data.is_empty() {
                    0.0
                } else {
                    let sum_sq: f32 = chunk.pcm_data.iter().map(|s| s * s).sum();
                    (sum_sq / chunk.pcm_data.len() as f32).sqrt()
                };
                let _ = app.emit(
                    "audio-chunk",
                    AudioChunkEvent {
                        sample_count: chunk.pcm_data.len(),
                        rms,
                        source: source.to_string(),
                        timestamp: chunk.timestamp,
                    },
                );
            }
        });

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&state, &app);
        Err(AppError::NotSupported)
    }
}

#[tauri::command]
pub async fn stop_recording(state: tauri::State<'_, RecordingState>) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let mut guard = state.capture.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(mut capture) = guard.take() {
            capture
                .stop()
                .map_err(|e| AppError::CaptureFailed(e.to_string()))?;
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = &state;
        Err(AppError::NotSupported)
    }
}

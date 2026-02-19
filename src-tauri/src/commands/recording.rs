use tauri::Emitter;

use crate::errors::AppError;
use crate::models::{AudioChunkEvent, AudioSource, RecordingState};

#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use tauri::Manager;
#[cfg(target_os = "macos")]
use tokio_util::sync::CancellationToken;

#[cfg(target_os = "macos")]
use crate::models::{PcmAccumulator, TranscriptEvent};
#[cfg(target_os = "macos")]
use crate::services::transcription::{TranscriptionService, TranscriptionState};

#[cfg(target_os = "macos")]
async fn flush_and_transcribe(
    accumulator: &Mutex<PcmAccumulator>,
    service: &Arc<TranscriptionService>,
    app: &tauri::AppHandle,
    base_dir: &std::path::Path,
) {
    let (system, mic) = {
        let Ok(mut acc) = accumulator.lock() else {
            return;
        };
        acc.flush()
    };

    let model_path = match crate::services::model_manager::resolve_model_path(base_dir) {
        Ok(Some(path)) => path,
        Ok(None) => {
            eprintln!("Transcription skipped: no model selected or file missing");
            return;
        }
        Err(e) => {
            eprintln!("Transcription skipped: {e}");
            return;
        }
    };

    for (audio, source_label) in [(system, "system"), (mic, "microphone")] {
        let Some(audio) = audio else { continue };
        if audio.samples.is_empty() {
            continue;
        }

        let svc = Arc::clone(service);
        let path = model_path.clone();
        let app_clone = app.clone();
        let source = source_label.to_string();
        let timestamp_ms = (audio.start_timestamp * 1000.0) as u64;
        let samples = audio.samples;

        tokio::task::spawn_blocking(move || svc.transcribe(&path, &samples))
            .await
            .map(|result| match result {
                Ok(text) if !text.is_empty() => {
                    let _ = app_clone.emit(
                        "transcript-segment",
                        TranscriptEvent {
                            text,
                            source,
                            timestamp_ms,
                        },
                    );
                }
                Err(e) => eprintln!("Transcription error ({source}): {e}"),
                _ => {}
            })
            .unwrap_or_else(|e| eprintln!("Transcription task panicked: {e}"));
    }
}

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

        // Reset accumulator for this recording session
        {
            let mut acc = state.accumulator.lock().map_err(|_| AppError::LockPoisoned)?;
            *acc = PcmAccumulator::new();
        }

        let accumulator = Arc::clone(&state.accumulator);
        let accumulator_for_flush = Arc::clone(&state.accumulator);

        // Get transcription service from Tauri managed state
        let transcription_state: tauri::State<'_, TranscriptionState> =
            app.state::<TranscriptionState>();
        let service = Arc::clone(&transcription_state.service);

        let base_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::IoError(e.to_string()))?;

        // Chunk-processing task: emit audio-chunk events and accumulate samples
        let app_for_chunks = app.clone();
        tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                let source_str = match chunk.source {
                    AudioSource::SystemAudio => "system",
                    AudioSource::Microphone => "microphone",
                };
                let rms = crate::services::audio::compute_rms(&chunk.pcm_data);
                let _ = app_for_chunks.emit(
                    "audio-chunk",
                    AudioChunkEvent {
                        sample_count: chunk.pcm_data.len(),
                        rms,
                        source: source_str.to_string(),
                        timestamp: chunk.timestamp,
                    },
                );

                if let Ok(mut acc) = accumulator.lock() {
                    acc.append(&chunk.source, &chunk.pcm_data, chunk.timestamp);
                }
            }
        });

        // Flush timer task: transcribe accumulated audio every 30 seconds
        let cancel = CancellationToken::new();
        {
            let mut token_guard = state.cancel_token.lock().map_err(|_| AppError::LockPoisoned)?;
            *token_guard = Some(cancel.clone());
        }

        let app_for_flush = app.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            interval.tick().await; // skip the immediate first tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        flush_and_transcribe(
                            &accumulator_for_flush,
                            &service,
                            &app_for_flush,
                            &base_dir,
                        )
                        .await;
                    }
                    _ = cancel.cancelled() => {
                        flush_and_transcribe(
                            &accumulator_for_flush,
                            &service,
                            &app_for_flush,
                            &base_dir,
                        )
                        .await;
                        break;
                    }
                }
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
        // Signal the flush timer to do a final flush and exit
        if let Ok(mut token) = state.cancel_token.lock() {
            if let Some(token) = token.take() {
                token.cancel();
            }
        }

        let mut guard = state.capture.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(mut capture) = guard.take() {
            capture
                .stop()
                .map_err(|e| AppError::CaptureFailed(e.to_string()))?;
        }

        // Reset the accumulator for the next recording session
        if let Ok(mut acc) = state.accumulator.lock() {
            *acc = PcmAccumulator::new();
        }

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = &state;
        Err(AppError::NotSupported)
    }
}

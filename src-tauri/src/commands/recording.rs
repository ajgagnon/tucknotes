use tauri::Emitter;

use crate::errors::AppError;
use crate::models::{AudioChunkEvent, AudioSource, RecordingState};

#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use tauri::Manager;
#[cfg(target_os = "macos")]
use tokio_util::sync::CancellationToken;

#[cfg(target_os = "macos")]
use crate::models::{AccumulatedAudio, PcmAccumulator, TranscriptEvent};
#[cfg(target_os = "macos")]
use crate::services::transcription::{TranscriptionService, TranscriptionState};

#[cfg(target_os = "macos")]
const STEP_INTERVAL: Duration = Duration::from_secs(3);
#[cfg(target_os = "macos")]
const WINDOW_MAX_SECS: f64 = 10.0;
#[cfg(target_os = "macos")]
const MIN_DURATION_SECS: f64 = 2.0;
#[cfg(target_os = "macos")]
const MIN_SPEECH_RATIO: f32 = 0.01;

// ---------------------------------------------------------------------------
// Transcription pipeline helpers (macOS only)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
struct BatchItem {
    samples: Vec<f32>,
    label: String,
    timestamp_ms: u64,
}

/// Collect eligible audio sources, resample to 16 kHz, and return as batch items.
#[cfg(target_os = "macos")]
fn collect_batch_items(
    sources: (Option<AccumulatedAudio>, Option<AccumulatedAudio>),
) -> Vec<BatchItem> {
    let (system, mic) = sources;
    let mut items = Vec::new();
    for (audio, label) in [(system, "system"), (mic, "microphone")] {
        if let Some(audio) = audio {
            let duration = audio.samples.len() as f64 / audio.sample_rate as f64;
            if duration < MIN_DURATION_SECS {
                continue;
            }
            let samples =
                crate::services::audio::resample_to_16khz(audio.samples, audio.sample_rate);
            let ratio = crate::services::vad::speech_ratio(&samples);
            if ratio < MIN_SPEECH_RATIO {
                continue;
            }
            items.push(BatchItem {
                timestamp_ms: (audio.start_timestamp * 1000.0) as u64,
                samples,
                label: label.to_string(),
            });
        }
    }
    items
}

/// Run batch transcription and emit events for each result.
/// Updates `prev_texts` with the latest transcript per source for cross-window context.
#[cfg(target_os = "macos")]
async fn transcribe_and_emit(
    items: Vec<BatchItem>,
    is_provisional: bool,
    service: &Arc<TranscriptionService>,
    app: &tauri::AppHandle,
    model_path: &std::path::Path,
    prev_texts: &Mutex<HashMap<String, String>>,
) {
    if items.is_empty() {
        return;
    }

    let prompts: Vec<Option<String>> = {
        let map = prev_texts.lock().unwrap_or_else(|e| e.into_inner());
        items.iter().map(|i| map.get(&i.label).cloned()).collect()
    };

    let svc = Arc::clone(service);
    let path = model_path.to_path_buf();
    let app = app.clone();

    let results = tokio::task::spawn_blocking(move || {
        let buffers: Vec<&[f32]> = items.iter().map(|i| i.samples.as_slice()).collect();
        let texts = svc.transcribe_batch(&path, &buffers, &prompts);
        (items, texts)
    })
    .await;

    match results {
        Ok((items, Ok(texts))) => {
            for (item, text) in items.into_iter().zip(texts) {
                if text.is_empty()
                    || crate::services::transcription::is_low_quality_output(&text)
                {
                    continue;
                }
                // Update context for next window (only on finalized results)
                if !is_provisional {
                    let mut map = prev_texts.lock().unwrap_or_else(|e| e.into_inner());
                    map.insert(item.label.clone(), text.clone());
                }
                let _ = app.emit(
                    "transcript-segment",
                    TranscriptEvent {
                        text,
                        source: item.label,
                        timestamp_ms: item.timestamp_ms,
                        is_provisional,
                    },
                );
            }
        }
        Ok((_, Err(e))) => eprintln!("[transcribe] error: {e}"),
        Err(e) => eprintln!("[transcribe] task panicked: {e}"),
    }
}

/// One tick of the sliding-window transcription loop.
#[cfg(target_os = "macos")]
async fn step_transcribe(
    accumulator: &Mutex<PcmAccumulator>,
    service: &Arc<TranscriptionService>,
    app: &tauri::AppHandle,
    base_dir: &std::path::Path,
    busy: &AtomicBool,
    prev_texts: &Mutex<HashMap<String, String>>,
) {
    if busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let model_path = match crate::services::model_manager::resolve_model_path(base_dir) {
        Ok(Some(path)) => path,
        _ => {
            busy.store(false, Ordering::SeqCst);
            return;
        }
    };

    let (sources, is_provisional) = {
        let Ok(mut acc) = accumulator.lock() else {
            busy.store(false, Ordering::SeqCst);
            return;
        };
        if acc.max_duration_secs() >= WINDOW_MAX_SECS {
            (acc.flush(), false)
        } else {
            (acc.peek(), true)
        }
    };

    let items = collect_batch_items(sources);
    transcribe_and_emit(items, is_provisional, service, app, &model_path, prev_texts).await;

    busy.store(false, Ordering::SeqCst);
}

/// Final flush: transcribe all remaining audio as non-provisional.
#[cfg(target_os = "macos")]
async fn final_flush(
    accumulator: &Mutex<PcmAccumulator>,
    service: &Arc<TranscriptionService>,
    app: &tauri::AppHandle,
    base_dir: &std::path::Path,
    prev_texts: &Mutex<HashMap<String, String>>,
) {
    let model_path = match crate::services::model_manager::resolve_model_path(base_dir) {
        Ok(Some(path)) => path,
        _ => return,
    };

    let sources = {
        let Ok(mut acc) = accumulator.lock() else {
            return;
        };
        acc.flush()
    };

    let items = collect_batch_items(sources);
    transcribe_and_emit(items, false, &Arc::clone(service), app, &model_path, prev_texts).await;
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

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

        {
            let mut acc = state.accumulator.lock().map_err(|_| AppError::LockPoisoned)?;
            *acc = PcmAccumulator::new();
        }

        let accumulator = Arc::clone(&state.accumulator);
        let accumulator_for_transcribe = Arc::clone(&state.accumulator);

        let transcription_state: tauri::State<'_, TranscriptionState> =
            app.state::<TranscriptionState>();
        let service = Arc::clone(&transcription_state.service);

        let base_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::IoError(e.to_string()))?;

        // Task 1: Forward audio chunks as events and accumulate for transcription
        let app_for_chunks = app.clone();
        tokio::task::spawn_blocking(move || {
            let mut aec = crate::services::echo_cancel::EchoCanceller::new().ok();

            while let Some(chunk) = rx.blocking_recv() {
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
                    match chunk.source {
                        AudioSource::SystemAudio => {
                            if let Some(aec) = aec.as_mut() {
                                aec.feed_system(&chunk.pcm_data);
                            }
                            acc.append(
                                &chunk.source,
                                &chunk.pcm_data,
                                chunk.timestamp,
                                chunk.sample_rate,
                            );
                        }
                        AudioSource::Microphone => {
                            let cleaned = aec
                                .as_mut()
                                .map(|aec| aec.feed_mic(&chunk.pcm_data, chunk.sample_rate));
                            if let Some(samples) = cleaned {
                                if !samples.is_empty() {
                                    acc.append(
                                        &chunk.source,
                                        &samples,
                                        chunk.timestamp,
                                        16000,
                                    );
                                }
                            } else {
                                acc.append(
                                    &chunk.source,
                                    &chunk.pcm_data,
                                    chunk.timestamp,
                                    chunk.sample_rate,
                                );
                            }
                        }
                    }
                }
            }
        });

        // Task 2: Sliding-window transcription loop
        let cancel = CancellationToken::new();
        {
            let mut token_guard = state.cancel_token.lock().map_err(|_| AppError::LockPoisoned)?;
            *token_guard = Some(cancel.clone());
        }

        let app_for_transcribe = app.clone();
        tokio::spawn(async move {
            let busy = Arc::new(AtomicBool::new(false));
            let prev_texts = Mutex::new(HashMap::<String, String>::new());
            let mut interval = tokio::time::interval(STEP_INTERVAL);
            interval.tick().await; // skip immediate first tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        step_transcribe(
                            &accumulator_for_transcribe,
                            &service,
                            &app_for_transcribe,
                            &base_dir,
                            &busy,
                            &prev_texts,
                        ).await;
                    }
                    _ = cancel.cancelled() => {
                        final_flush(
                            &accumulator_for_transcribe,
                            &service,
                            &app_for_transcribe,
                            &base_dir,
                            &prev_texts,
                        ).await;
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

use crate::errors::AppError;
use crate::models::RecordingState;

// ---------------------------------------------------------------------------
// macOS-specific implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod macos {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tauri::{Emitter, Manager};
    use tokio_util::sync::CancellationToken;

    use crate::errors::{lock_or_err, AppError};
    use crate::models::{
        AccumulatedAudio, AudioChunkEvent, AudioSource, PcmAccumulator, RecordingState,
        TranscriptEvent,
    };
    use crate::services::database::{self, DatabaseState};
    use crate::services::transcription::{TranscriptionService, TranscriptionState};

    const STEP_INTERVAL: Duration = Duration::from_secs(3);
    const WINDOW_MAX_SECS: f64 = 30.0;
    const MIN_DURATION_SECS: f64 = 2.0;
    const MIN_SPEECH_RATIO: f32 = 0.05;

    struct BatchItem {
        samples: Vec<f32>,
        label: String,
        timestamp_ms: u64,
    }

    /// Collect eligible audio sources, resample to 16 kHz, and return as batch items.
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
                let mut samples =
                    crate::services::audio::resample_to_16khz(audio.samples, audio.sample_rate);
                crate::services::audio::normalize_audio(&mut samples);
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
    /// Saves finalized (non-provisional) segments to the database.
    async fn transcribe_and_emit(
        items: Vec<BatchItem>,
        is_provisional: bool,
        service: &Arc<TranscriptionService>,
        app: &tauri::AppHandle,
        model_path: &std::path::Path,
        prev_texts: &Mutex<HashMap<String, String>>,
        meeting_id: &str,
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
        let meeting_id = meeting_id.to_string();

        let results = tokio::task::spawn_blocking(move || {
            let buffers: Vec<&[f32]> = items.iter().map(|i| i.samples.as_slice()).collect();
            let texts = svc.transcribe_batch(&path, &buffers, &prompts, is_provisional);
            (items, texts, prompts)
        })
        .await;

        match results {
            Ok((items, Ok(texts), prompts)) => {
                for (i, (item, text)) in items.into_iter().zip(texts).enumerate() {
                    if text.is_empty() {
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
                            text: text.clone(),
                            source: item.label.clone(),
                            timestamp_ms: item.timestamp_ms,
                            is_provisional,
                        },
                    );
                    // Persist finalized segments to SQLite
                    if !is_provisional {
                        let db: &DatabaseState = app.state::<DatabaseState>().inner();
                        match db.conn.lock() {
                            Ok(conn) => {
                                let prompt = prompts.get(i).and_then(|p| p.as_deref());
                                if let Err(e) = database::insert_segment(
                                    &conn,
                                    &meeting_id,
                                    &text,
                                    &item.label,
                                    item.timestamp_ms as i64,
                                    prompt,
                                    database::now_unix_ms(),
                                ) {
                                    eprintln!("[transcribe] failed to insert segment: {e}");
                                }
                            }
                            Err(e) => eprintln!("[transcribe] db lock poisoned: {e}"),
                        }
                    }
                }
            }
            Ok((_, Err(e), _)) => eprintln!("[transcribe] error: {e}"),
            Err(e) => eprintln!("[transcribe] task panicked: {e}"),
        }
    }

    /// One tick of the sliding-window transcription loop.
    async fn step_transcribe(
        accumulator: &Mutex<PcmAccumulator>,
        service: &Arc<TranscriptionService>,
        app: &tauri::AppHandle,
        base_dir: &std::path::Path,
        busy: &AtomicBool,
        prev_texts: &Mutex<HashMap<String, String>>,
        meeting_id: &str,
    ) {
        if busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let model_path = match crate::services::model_manager::resolve_whisper_path(base_dir) {
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
        transcribe_and_emit(
            items,
            is_provisional,
            service,
            app,
            &model_path,
            prev_texts,
            meeting_id,
        )
        .await;

        busy.store(false, Ordering::SeqCst);
    }

    /// Final flush: transcribe all remaining audio as non-provisional.
    async fn final_flush(
        accumulator: &Mutex<PcmAccumulator>,
        service: &Arc<TranscriptionService>,
        app: &tauri::AppHandle,
        base_dir: &std::path::Path,
        prev_texts: &Mutex<HashMap<String, String>>,
        meeting_id: &str,
    ) {
        let model_path = match crate::services::model_manager::resolve_whisper_path(base_dir) {
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
        transcribe_and_emit(items, false, service, app, &model_path, prev_texts, meeting_id).await;
    }

    /// Core implementation of start_recording for macOS.
    pub(super) async fn do_start_recording(
        state: tauri::State<'_, RecordingState>,
        app: tauri::AppHandle,
    ) -> Result<String, AppError> {
        if !unsafe { crate::services::permissions::CGPreflightScreenCaptureAccess() } {
            return Err(AppError::PermissionDenied(
                "Screen recording permission is required to capture audio.".into(),
            ));
        }

        let mut guard = lock_or_err(&state.capture)?;
        if guard.is_some() {
            return Err(AppError::CaptureFailed("Already recording".into()));
        }

        let (capture, mut rx) = crate::services::audio_capture::AudioCapture::start()
            .map_err(|e| AppError::CaptureFailed(e.to_string()))?;
        *guard = Some(capture);
        drop(guard);

        {
            let mut acc = lock_or_err(&state.accumulator)?;
            *acc = PcmAccumulator::new();
        }

        // Create a new meeting in the database
        let meeting_id = uuid::Uuid::new_v4().to_string();
        let now = database::now_unix_ms();
        {
            let db_state: tauri::State<'_, DatabaseState> = app.state::<DatabaseState>();
            let conn = lock_or_err(&db_state.conn)?;
            database::create_meeting(&conn, &meeting_id, "Recording", now)?;
        }
        {
            let mut sid = lock_or_err(&state.session_id)?;
            *sid = Some(meeting_id.clone());
        }
        {
            let mut started = lock_or_err(&state.started_at)?;
            *started = Some(std::time::Instant::now());
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
                            match aec
                                .as_mut()
                                .map(|aec| aec.feed_mic(&chunk.pcm_data, chunk.sample_rate))
                            {
                                Some(samples) if !samples.is_empty() => {
                                    acc.append(&chunk.source, &samples, chunk.timestamp, 16000);
                                }
                                Some(_) => { /* AEC returned empty, skip this chunk */ }
                                None => {
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
            }
        });

        // Task 2: Sliding-window transcription loop
        let cancel = CancellationToken::new();
        {
            let mut token_guard = lock_or_err(&state.cancel_token)?;
            *token_guard = Some(cancel.clone());
        }

        let app_for_transcribe = app.clone();
        let meeting_id_for_return = meeting_id.clone();
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
                            &meeting_id,
                        ).await;
                    }
                    _ = cancel.cancelled() => {
                        final_flush(
                            &accumulator_for_transcribe,
                            &service,
                            &app_for_transcribe,
                            &base_dir,
                            &prev_texts,
                            &meeting_id,
                        ).await;
                        break;
                    }
                }
            }
        });

        Ok(meeting_id_for_return)
    }

    /// Core implementation of stop_recording for macOS.
    pub(super) async fn do_stop_recording(
        state: tauri::State<'_, RecordingState>,
        app: tauri::AppHandle,
    ) -> Result<(), AppError> {
        match state.cancel_token.lock() {
            Ok(mut token) => {
                if let Some(token) = token.take() {
                    token.cancel();
                }
            }
            Err(e) => eprintln!("[stop_recording] cancel_token lock poisoned: {e}"),
        }

        let mut guard = lock_or_err(&state.capture)?;
        if let Some(mut capture) = guard.take() {
            capture
                .stop()
                .map_err(|e| AppError::CaptureFailed(e.to_string()))?;
        }

        match state.accumulator.lock() {
            Ok(mut acc) => {
                *acc = PcmAccumulator::new();
            }
            Err(e) => eprintln!("[stop_recording] accumulator lock poisoned: {e}"),
        }

        // End the meeting in the database
        let meeting_id = lock_or_err(&state.session_id)?.take();
        let started_at = lock_or_err(&state.started_at)?.take();
        if let Some(mid) = meeting_id {
            let duration_ms = started_at
                .map(|s| s.elapsed().as_millis() as i64)
                .unwrap_or(0);
            let db: &DatabaseState = app.state::<DatabaseState>().inner();
            match db.conn.lock() {
                Ok(conn) => {
                    if let Err(e) =
                        database::end_meeting(&conn, &mid, database::now_unix_ms(), duration_ms)
                    {
                        eprintln!("[stop_recording] failed to end meeting {mid}: {e}");
                    }
                }
                Err(e) => eprintln!("[stop_recording] db lock poisoned: {e}"),
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_recording(
    state: tauri::State<'_, RecordingState>,
    app: tauri::AppHandle,
) -> Result<String, AppError> {
    #[cfg(target_os = "macos")]
    {
        macos::do_start_recording(state, app).await
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&state, &app);
        Err(AppError::NotSupported(
            "Not supported on this platform".into(),
        ))
    }
}

#[tauri::command]
pub async fn stop_recording(
    state: tauri::State<'_, RecordingState>,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        macos::do_stop_recording(state, app).await
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&state, &app);
        Err(AppError::NotSupported(
            "Not supported on this platform".into(),
        ))
    }
}

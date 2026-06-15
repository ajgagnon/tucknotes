use tauri::Emitter;

use crate::commands::licensing::require_paid_entitlement;
use crate::errors::AppError;
use crate::models::licensing::LicensingState;
use crate::models::{RecordingState, RecordingStateEvent};

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
    use crate::models::RecordingFinalizedEvent;
    use tokio_util::sync::CancellationToken;

    use crate::errors::{lock_or_err, AppError};
    use crate::models::meeting_detection::MeetingDetectorState;
    use crate::models::{
        AccumulatedAudio, AudioChunkEvent, AudioSource, PcmAccumulator, RecordingState,
        RecordingStateEvent, TranscriptEvent,
    };
    use crate::services::database::{self, DatabaseState};
    use crate::services::transcription::{TranscriptionService, TranscriptionState};

    const STEP_INTERVAL: Duration = Duration::from_secs(6);
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

    /// A segment with its source label and absolute timestamp for interleaving.
    struct TaggedSegment {
        text: String,
        source: String,
        /// Absolute timestamp in ms (buffer start + Whisper offset).
        timestamp_ms: u64,
    }

    /// Run batch transcription, interleave segments from all sources by
    /// timestamp, then emit events in chronological order.
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
            let segment_lists = svc.transcribe_batch(&path, &buffers, &prompts, is_provisional);
            (items, segment_lists)
        })
        .await;

        match results {
            Ok((items, Ok(segment_lists))) => {
                if is_provisional {
                    // Provisional: emit one combined event per source so the
                    // frontend can display the full in-progress text (it stores
                    // provisional as Record<source, segment>, so per-segment
                    // emission would lose all but the last segment).
                    for (item, segments) in items.iter().zip(segment_lists.iter()) {
                        if segments.is_empty() {
                            continue;
                        }
                        let full_text: String = segments
                            .iter()
                            .map(|s| s.text.as_str())
                            .collect::<Vec<_>>()
                            .join(" ");
                        let _ = app.emit(
                            "transcript-segment",
                            TranscriptEvent {
                                text: full_text,
                                source: item.label.clone(),
                                timestamp_ms: item.timestamp_ms,
                                is_provisional: true,
                            },
                        );
                    }
                } else {
                    // Finalized: emit per-segment events interleaved by
                    // timestamp so the conversation is in chronological order.
                    let mut all_segments: Vec<TaggedSegment> = Vec::new();

                    for (item, segments) in items.iter().zip(segment_lists.iter()) {
                        for seg in segments {
                            let abs_ms = item.timestamp_ms + (seg.start_cs as u64 * 10);
                            all_segments.push(TaggedSegment {
                                text: seg.text.clone(),
                                source: item.label.clone(),
                                timestamp_ms: abs_ms,
                            });
                        }

                        // Update cross-window context per source
                        if !segments.is_empty() {
                            let full_text: String = segments
                                .iter()
                                .map(|s| s.text.as_str())
                                .collect::<Vec<_>>()
                                .join(" ");
                            let mut map =
                                prev_texts.lock().unwrap_or_else(|e| e.into_inner());
                            map.insert(item.label.clone(), full_text);
                        }
                    }

                    all_segments.sort_by_key(|s| s.timestamp_ms);

                    for seg in &all_segments {
                        let _ = app.emit(
                            "transcript-segment",
                            TranscriptEvent {
                                text: seg.text.clone(),
                                source: seg.source.clone(),
                                timestamp_ms: seg.timestamp_ms,
                                is_provisional: false,
                            },
                        );

                        let db: &DatabaseState = app.state::<DatabaseState>().inner();
                        match db.conn.lock() {
                            Ok(conn) => {
                                if let Err(e) = database::insert_segment(
                                    &conn,
                                    &meeting_id,
                                    &seg.text,
                                    &seg.source,
                                    seg.timestamp_ms as i64,
                                    None,
                                    database::now_unix_ms(),
                                ) {
                                    eprintln!("[transcribe] failed to insert segment: {e}");
                                }
                            }
                            Err(e) => eprintln!("[transcribe] db lock poisoned: {e}"),
                        }
                    }

                    if !all_segments.is_empty() {
                        let lines: Vec<(String, String)> = all_segments
                            .iter()
                            .map(|s| (s.source.clone(), s.text.clone()))
                            .collect();
                        crate::services::live_minutes::on_finalized_window(
                            &app,
                            &meeting_id,
                            &lines,
                        );
                    }
                }
            }
            Ok((_, Err(e))) => eprintln!("[transcribe] error: {e}"),
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

    pub(super) fn snapshot_recording_state(
        state: &RecordingState,
    ) -> Result<RecordingStateEvent, AppError> {
        // A session whose captures are still starting counts as recording —
        // otherwise the init window would read as paused (session set, no
        // capture), and the UI wouldn't navigate to the new meeting.
        let recording = lock_or_err(&state.capture)?.is_some()
            || state.capture_starting.load(Ordering::SeqCst);
        let meeting_id = lock_or_err(&state.session_id)?.clone();
        let paused = meeting_id.is_some()
            && !recording
            && !state.finalize_in_progress.load(Ordering::SeqCst);
        let wall_offset_secs = *lock_or_err(&state.wall_time_offset_secs)?;
        let elapsed_secs = lock_or_err(&state.started_at)?
            .map(|s| {
                let session_secs = s.elapsed().as_secs_f64();
                (wall_offset_secs + session_secs).floor() as u64
            })
            .unwrap_or(0);
        Ok(RecordingStateEvent {
            recording,
            paused,
            meeting_id,
            elapsed_secs,
            reset_live_transcript: false,
        })
    }

    /// Captures created by `create_captures`, not yet wired into shared state.
    /// Must not be silently dropped: VoiceCapture stops via Drop, but dropping
    /// an SCStream only releases the reference — the system keeps capturing —
    /// so discard paths must call `discard()`.
    struct PendingCaptures {
        capture: crate::services::audio_capture::AudioCapture,
        voice_capture: crate::services::voice_capture::VoiceCapture,
        shared_rx: tokio::sync::mpsc::Receiver<crate::services::audio_capture::AudioChunk>,
    }

    impl PendingCaptures {
        /// Tear down captures that never got installed.
        fn discard(mut self) {
            if let Err(e) = self.capture.stop() {
                eprintln!("[recording] failed to stop discarded capture: {e}");
            }
            // voice_capture stops via Drop.
        }
    }

    /// The slow, state-free half of capture startup: SCK stream + AVAudioEngine.
    /// Takes seconds (window enumeration, VoiceProcessingIO spin-up) — must run
    /// on a blocking thread, off the start_recording command's critical path.
    fn create_captures() -> Result<PendingCaptures, AppError> {
        let (shared_tx, shared_rx) = tokio::sync::mpsc::channel(1024);

        let (mut capture, mut sys_rx) = crate::services::audio_capture::AudioCapture::start()
            .map_err(|e| AppError::CaptureFailed(e.to_string()))?;

        let sys_tx = shared_tx.clone();
        tokio::task::spawn_blocking(move || {
            while let Some(chunk) = sys_rx.blocking_recv() {
                let _ = sys_tx.try_send(chunk);
            }
        });

        let use_vp = crate::services::audio_output::is_builtin_speakers();
        eprintln!("[recording] mic capture mode: voice_processing={use_vp}");
        let voice_capture =
            match crate::services::voice_capture::VoiceCapture::start(use_vp, shared_tx) {
                Ok(vc) => vc,
                Err(e) => {
                    // Dropping the SCStream doesn't stop the capture — do it
                    // explicitly so a half-failed init doesn't leak a stream.
                    let _ = capture.stop();
                    return Err(AppError::CaptureFailed(format!("VoiceCapture: {e}")));
                }
            };

        Ok(PendingCaptures {
            capture,
            voice_capture,
            shared_rx,
        })
    }

    /// Wires freshly created captures into shared state and starts the
    /// transcription loop. Returns `Ok(false)` (dropping the captures) when the
    /// session was stopped/paused while they were starting — detected by the
    /// epoch check under the capture mutex, which stop/pause bump before taking
    /// the capture.
    fn install_captures(
        state: &RecordingState,
        app: &tauri::AppHandle,
        meeting_id: &str,
        epoch: u64,
        pending: PendingCaptures,
    ) -> Result<bool, AppError> {
        // Everything installs under the capture mutex so stop/pause either see
        // none of it or all of it.
        let mut guard = match lock_or_err(&state.capture) {
            Ok(g) => g,
            Err(e) => {
                pending.discard();
                return Err(e);
            }
        };
        if state.capture_epoch.load(Ordering::SeqCst) != epoch || guard.is_some() {
            drop(guard);
            pending.discard();
            return Ok(false);
        }
        // The session clock was set by start/resume before spawning this init;
        // it must stay continuous across pause/resume so new audio timestamps
        // sort after existing segments.
        let wall_origin = match lock_or_err(&state.started_at)
            .map(|s| *s)
            .and_then(|s| s.ok_or_else(|| AppError::CaptureFailed("Missing session clock".into())))
        {
            Ok(origin) => origin,
            Err(e) => {
                drop(guard);
                pending.discard();
                return Err(e);
            }
        };
        let wall_offset_secs = match lock_or_err(&state.wall_time_offset_secs) {
            Ok(w) => *w,
            Err(e) => {
                drop(guard);
                pending.discard();
                return Err(e);
            }
        };

        let PendingCaptures {
            capture,
            voice_capture,
            mut shared_rx,
        } = pending;

        *guard = Some(capture);

        {
            let mut vc_guard = lock_or_err(&state.voice_capture)?;
            *vc_guard = Some(voice_capture);
        }

        {
            let mut acc = lock_or_err(&state.accumulator)?;
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

        let app_for_chunks = app.clone();
        tokio::task::spawn_blocking(move || {
            while let Some(chunk) = shared_rx.blocking_recv() {
                let wall_ts = wall_offset_secs + wall_origin.elapsed().as_secs_f64();
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
                        timestamp: wall_ts,
                    },
                );
                if let Ok(mut acc) = accumulator.lock() {
                    acc.append(
                        &chunk.source,
                        &chunk.pcm_data,
                        wall_ts,
                        chunk.sample_rate,
                    );
                }
            }
        });

        let cancel = CancellationToken::new();
        {
            let mut token_guard = lock_or_err(&state.cancel_token)?;
            *token_guard = Some(cancel.clone());
        }

        let app_for_transcribe = app.clone();
        let meeting_id_owned = meeting_id.to_string();
        let handle = tokio::spawn(async move {
            let busy = Arc::new(AtomicBool::new(false));
            let prev_texts = Mutex::new(HashMap::<String, String>::new());
            let mut interval = tokio::time::interval(STEP_INTERVAL);
            interval.tick().await;

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
                            &meeting_id_owned,
                        ).await;
                    }
                    _ = cancel.cancelled() => {
                        final_flush(
                            &accumulator_for_transcribe,
                            &service,
                            &app_for_transcribe,
                            &base_dir,
                            &prev_texts,
                            &meeting_id_owned,
                        ).await;
                        break;
                    }
                }
            }
        });
        *lock_or_err(&state.transcribe_task)? = Some(handle);

        state.capture_starting.store(false, Ordering::SeqCst);

        Ok(true)
    }

    /// What a failed capture init should do to the session it was starting.
    enum CaptureInitFailure {
        /// Fresh start: end the session (meeting row is kept; retrying from
        /// the meeting page goes through the resume path).
        EndSession,
        /// Resume: leave the meeting paused.
        StayPaused,
    }

    /// Runs the slow capture startup in the background and installs the result.
    /// On failure (and only if no stop/pause superseded this attempt), rolls
    /// the session back per `failure` and surfaces the error to the frontend
    /// via a `recording-error` event.
    fn spawn_capture_init(
        app: tauri::AppHandle,
        meeting_id: String,
        epoch: u64,
        failure: CaptureInitFailure,
    ) {
        // Serializes init tasks: VoiceCapture wraps a process-global engine, so
        // when a quick stop-then-restart supersedes an init, the stale one must
        // finish (and discard its engine) before the next may create one.
        static INIT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

        tokio::spawn(async move {
            let _init_guard = INIT_LOCK.lock().await;
            let t0 = std::time::Instant::now();
            let created = tokio::task::spawn_blocking(create_captures).await;
            let state: tauri::State<'_, RecordingState> = app.state();

            let result = match created {
                Ok(Ok(pending)) => {
                    eprintln!("[recording] capture init took {:.2?}", t0.elapsed());
                    install_captures(&state, &app, &meeting_id, epoch, pending)
                }
                Ok(Err(e)) => Err(e),
                Err(e) => Err(AppError::CaptureFailed(format!(
                    "capture init panicked: {e}"
                ))),
            };

            match result {
                Ok(true) => {
                    crate::services::live_minutes::start_session(&app, &meeting_id);
                    if let Ok(snap) = snapshot_recording_state(&state) {
                        let _ = app.emit("recording-state-changed", snap);
                    }
                }
                // Lost the race to stop/pause; the fresh captures were dropped.
                Ok(false) => {}
                Err(e) => {
                    eprintln!("[recording] capture init failed: {e}");
                    // A stop/pause that superseded this attempt already owns
                    // the session state — don't touch it.
                    if state.capture_epoch.load(Ordering::SeqCst) != epoch {
                        return;
                    }
                    state.capture_starting.store(false, Ordering::SeqCst);
                    if let CaptureInitFailure::EndSession = failure {
                        if let Err(e2) = finalize_session_after_transcribe(&state, &app) {
                            eprintln!("[recording] rollback after failed init: {e2}");
                        }
                    }
                    if let Ok(snap) = snapshot_recording_state(&state) {
                        let _ = app.emit("recording-state-changed", snap);
                    }
                    let _ = app.emit("recording-error", e);
                }
            }
        });
    }

    /// Core implementation of start_recording for macOS.
    /// When `resume_meeting_id` is set, continues an existing meeting (reopens it in the DB).
    pub(super) async fn do_start_recording(
        state: &RecordingState,
        app: tauri::AppHandle,
        resume_meeting_id: Option<String>,
    ) -> Result<String, AppError> {
        if !unsafe { crate::services::permissions::CGPreflightScreenCaptureAccess() } {
            return Err(AppError::PermissionDenied(
                "Screen recording permission is required to capture audio.".into(),
            ));
        }

        if state
            .finalize_in_progress
            .load(Ordering::SeqCst)
        {
            return Err(AppError::CaptureFailed(
                "Still saving the previous recording. Try again in a moment.".into(),
            ));
        }

        {
            let guard = lock_or_err(&state.capture)?;
            if guard.is_some() || state.capture_starting.load(Ordering::SeqCst) {
                return Err(AppError::CaptureFailed("Already recording".into()));
            }
        }

        let meeting_id = if let Some(existing_id) =
            resume_meeting_id.filter(|s| !s.is_empty())
        {
            let db_state: tauri::State<'_, DatabaseState> = app.state::<DatabaseState>();
            let conn = lock_or_err(&db_state.conn)?;
            if !database::meeting_exists(&conn, &existing_id)? {
                return Err(AppError::DatabaseError(format!(
                    "Meeting not found: {existing_id}"
                )));
            }
            database::reopen_meeting(&conn, &existing_id)?;
            let max_ms = database::max_segment_timestamp_ms(&conn, &existing_id)?;
            let offset_secs = (max_ms as f64) / 1000.0 + 0.001;
            {
                let mut w = lock_or_err(&state.wall_time_offset_secs)?;
                *w = offset_secs;
            }
            {
                let mut sid = lock_or_err(&state.session_id)?;
                *sid = Some(existing_id.clone());
            }
            {
                let mut started = lock_or_err(&state.started_at)?;
                *started = Some(std::time::Instant::now());
            }
            existing_id
        } else {
            {
                let mut w = lock_or_err(&state.wall_time_offset_secs)?;
                *w = 0.0;
            }
            let new_id = uuid::Uuid::new_v4().to_string();
            let now = database::now_unix_ms();
            {
                let db_state: tauri::State<'_, DatabaseState> = app.state::<DatabaseState>();
                let conn = lock_or_err(&db_state.conn)?;
                database::create_meeting(&conn, &new_id, None, now)?;
            }
            {
                let mut sid = lock_or_err(&state.session_id)?;
                *sid = Some(new_id.clone());
            }
            {
                let mut started = lock_or_err(&state.started_at)?;
                *started = Some(std::time::Instant::now());
            }
            new_id
        };

        // The slow capture startup (SCK + AVAudioEngine, several seconds) runs
        // in the background so this command — and the UI's navigation to the
        // meeting — returns immediately. `live_minutes::start_session` runs on
        // its success path.
        let epoch = state.capture_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        state.capture_starting.store(true, Ordering::SeqCst);
        spawn_capture_init(
            app.clone(),
            meeting_id.clone(),
            epoch,
            CaptureInitFailure::EndSession,
        );

        {
            let det: tauri::State<'_, MeetingDetectorState> = app.state();
            det.recording_active.store(true, Ordering::SeqCst);
        }
        if let Some(w) = app.get_webview_window("meeting-overlay") {
            let _ = w.close();
        }

        Ok(meeting_id)
    }

    /// Pause capture and transcription without ending the meeting (for transcript UI).
    pub(super) async fn do_pause_recording(state: &RecordingState) -> Result<(), AppError> {
        if state.finalize_in_progress.load(Ordering::SeqCst) {
            return Err(AppError::CaptureFailed(
                "Cannot pause while saving recording.".into(),
            ));
        }

        // Invalidate any in-flight capture init *before* taking the capture, so
        // a background init task can never install captures after we've looked.
        // Pause-while-starting cancels the init and leaves the session paused.
        let was_starting = state.capture_starting.swap(false, Ordering::SeqCst);
        state.capture_epoch.fetch_add(1, Ordering::SeqCst);

        let capture = {
            let mut cap_guard = lock_or_err(&state.capture)?;
            cap_guard.take()
        };
        if capture.is_none() && !was_starting {
            return Ok(());
        }
        if let Some(mut capture) = capture {
            capture
                .stop()
                .map_err(|e| AppError::CaptureFailed(e.to_string()))?;
        }

        match state.cancel_token.lock() {
            Ok(mut token) => {
                if let Some(token) = token.take() {
                    token.cancel();
                }
            }
            Err(e) => eprintln!("[pause_recording] cancel_token lock poisoned: {e}"),
        }

        {
            let mut vc_guard = lock_or_err(&state.voice_capture)?;
            vc_guard.take();
        }

        let handle = lock_or_err(&state.transcribe_task)?.take();
        if let Some(handle) = handle {
            if let Err(e) = handle.await {
                eprintln!("[pause_recording] transcribe task panicked: {e}");
            }
        }

        Ok(())
    }

    pub(super) async fn do_resume_recording(
        state: &RecordingState,
        app: tauri::AppHandle,
    ) -> Result<(), AppError> {
        if state.finalize_in_progress.load(Ordering::SeqCst) {
            return Err(AppError::CaptureFailed(
                "Still saving the previous recording. Try again in a moment.".into(),
            ));
        }

        {
            let guard = lock_or_err(&state.capture)?;
            if guard.is_some() || state.capture_starting.load(Ordering::SeqCst) {
                return Ok(());
            }
        }

        let meeting_id = lock_or_err(&state.session_id)?
            .clone()
            .ok_or_else(|| AppError::CaptureFailed("No meeting to resume".into()))?;

        lock_or_err(&state.started_at)?
            .ok_or_else(|| AppError::CaptureFailed("Missing session clock".into()))?;

        let epoch = state.capture_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        state.capture_starting.store(true, Ordering::SeqCst);
        spawn_capture_init(app, meeting_id, epoch, CaptureInitFailure::StayPaused);

        Ok(())
    }

    /// Clears the PCM accumulator, ends the meeting in the database, re-enables meeting detection.
    /// Returns the meeting id taken from `session_id` (if any).
    fn finalize_session_after_transcribe(
        recording_state: &RecordingState,
        app: &tauri::AppHandle,
    ) -> Result<Option<String>, AppError> {
        match recording_state.accumulator.lock() {
            Ok(mut acc) => *acc = PcmAccumulator::new(),
            Err(e) => eprintln!("[stop_recording] accumulator lock poisoned: {e}"),
        }

        let meeting_id = lock_or_err(&recording_state.session_id)?.take();
        let started_at = lock_or_err(&recording_state.started_at)?.take();

        if let Some(ref mid) = meeting_id {
            let session_ms = started_at
                .as_ref()
                .map(|s| s.elapsed().as_millis() as i64)
                .unwrap_or(0);
            let db: &DatabaseState = app.state::<DatabaseState>().inner();
            match db.conn.lock() {
                Ok(conn) => {
                    let prev_ms = database::meeting_recording_duration_ms(&conn, mid)
                        .unwrap_or(0);
                    let total_ms = prev_ms.saturating_add(session_ms);
                    if let Err(e) = database::end_meeting(
                        &conn,
                        mid,
                        database::now_unix_ms(),
                        total_ms,
                    ) {
                        eprintln!("[stop_recording] failed to end meeting {mid}: {e}");
                    }
                }
                Err(e) => eprintln!("[stop_recording] db lock poisoned: {e}"),
            }
        }

        {
            let det: tauri::State<'_, MeetingDetectorState> = app.state();
            det.recording_active.store(false, Ordering::SeqCst);
        }

        Ok(meeting_id)
    }

    /// Core implementation of stop_recording for macOS.
    ///
    /// Returns `Some(meeting_id)` when transcript persistence continues in the background.
    pub(super) async fn do_stop_recording(
        state: tauri::State<'_, RecordingState>,
        app: tauri::AppHandle,
    ) -> Result<Option<String>, AppError> {
        if state.finalize_in_progress.load(Ordering::SeqCst) {
            return Ok(None);
        }

        // Invalidate any in-flight capture init *before* taking the capture, so
        // a background init task can never install captures (and a transcribe
        // task we'd miss cancelling) after we've looked.
        state.capture_starting.store(false, Ordering::SeqCst);
        state.capture_epoch.fetch_add(1, Ordering::SeqCst);

        {
            let mut guard = lock_or_err(&state.capture)?;
            if let Some(mut capture) = guard.take() {
                capture
                    .stop()
                    .map_err(|e| AppError::CaptureFailed(e.to_string()))?;
            }
        }

        match state.cancel_token.lock() {
            Ok(mut token) => {
                if let Some(token) = token.take() {
                    token.cancel();
                }
            }
            Err(e) => eprintln!("[stop_recording] cancel_token lock poisoned: {e}"),
        }

        // Stop voice capture (AVAudioEngine + VoiceProcessingIO)
        {
            let mut vc_guard = lock_or_err(&state.voice_capture)?;
            // Dropping VoiceCapture calls voice_capture_stop() via Drop
            vc_guard.take();
        }

        let pending_meeting_id = lock_or_err(&state.session_id)?.clone();
        let handle = lock_or_err(&state.transcribe_task)?.take();

        if let Some(handle) = handle {
            state.finalize_in_progress.store(true, Ordering::SeqCst);
            let finalize_flag = Arc::clone(&state.finalize_in_progress);
            let app_bg = app.clone();
            let notify_meeting_id = pending_meeting_id.clone();

            tokio::spawn(async move {
                struct ClearFinalizeFlag(Arc<AtomicBool>);
                impl Drop for ClearFinalizeFlag {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::SeqCst);
                    }
                }

                // Scope the finalize flag to the transcript flush only: the
                // live-minutes flush below can take seconds (one LLM pass) and
                // doesn't touch capture/accumulator state, so it must not
                // block starting the next recording.
                let meeting_id = {
                    let _clear = ClearFinalizeFlag(Arc::clone(&finalize_flag));

                    if let Err(e) = handle.await {
                        eprintln!("[stop_recording] transcribe task panicked: {e}");
                    }

                    let recording_state: tauri::State<'_, RecordingState> = app_bg.state();

                    match finalize_session_after_transcribe(&recording_state, &app_bg) {
                        Ok(mid) => mid,
                        Err(e) => {
                            eprintln!("[stop_recording] finalize session after transcribe: {e}");
                            None
                        }
                    }
                };

                let finalized_id = meeting_id.or(notify_meeting_id);
                if let Some(mid) = finalized_id {
                    // Flush the live-minutes tail before announcing the
                    // recording finalized (which triggers auto-summarize).
                    // Bounded: every earlier pass already persisted, so a
                    // timeout only risks the final tail (which still lands
                    // late via its own persist).
                    let _ = tokio::time::timeout(
                        std::time::Duration::from_secs(15),
                        crate::services::live_minutes::finalize_session(&app_bg, &mid),
                    )
                    .await;
                    let _ = app_bg.emit(
                        "recording-finalized",
                        RecordingFinalizedEvent { meeting_id: mid },
                    );
                }
            });

            Ok(pending_meeting_id)
        } else {
            finalize_session_after_transcribe(&state, &app)?;
            // No transcription ran, so there's no tail to flush — just drop
            // any live-minutes session left from this recording.
            if let Some(mid) = pending_meeting_id {
                let app_bg = app.clone();
                tokio::spawn(async move {
                    crate::services::live_minutes::finalize_session(&app_bg, &mid).await;
                });
            }
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_recording(
    state: tauri::State<'_, RecordingState>,
    licensing: tauri::State<'_, LicensingState>,
    app: tauri::AppHandle,
    resume_meeting_id: Option<String>,
) -> Result<String, AppError> {
    require_paid_entitlement(&licensing)?;

    #[cfg(target_os = "macos")]
    {
        let reset_live_transcript = resume_meeting_id.as_ref().map_or(true, |s| s.is_empty());
        let id = macos::do_start_recording(&state, app.clone(), resume_meeting_id).await?;
        let mut snap = macos::snapshot_recording_state(&state)?;
        snap.reset_live_transcript = reset_live_transcript;
        let _ = app.emit("recording-state-changed", snap);
        Ok(id)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&state, &app, resume_meeting_id);
        Err(AppError::NotSupported(
            "Not supported on this platform".into(),
        ))
    }
}

#[tauri::command]
pub async fn stop_recording(
    state: tauri::State<'_, RecordingState>,
    app: tauri::AppHandle,
) -> Result<Option<String>, AppError> {
    #[cfg(target_os = "macos")]
    {
        let pending = macos::do_stop_recording(state, app.clone()).await?;
        let _ = app.emit(
            "recording-state-changed",
            RecordingStateEvent {
                recording: false,
                paused: false,
                meeting_id: None,
                elapsed_secs: 0,
                reset_live_transcript: false,
            },
        );
        Ok(pending)
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
pub async fn get_recording_state(
    state: tauri::State<'_, RecordingState>,
) -> Result<RecordingStateEvent, AppError> {
    #[cfg(target_os = "macos")]
    {
        macos::snapshot_recording_state(&state)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = &state;
        Ok(RecordingStateEvent {
            recording: false,
            paused: false,
            meeting_id: None,
            elapsed_secs: 0,
            reset_live_transcript: false,
        })
    }
}

#[tauri::command]
pub async fn pause_recording(
    state: tauri::State<'_, RecordingState>,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        macos::do_pause_recording(&state).await?;
        let snap = macos::snapshot_recording_state(&state)?;
        let _ = app.emit("recording-state-changed", snap);
        Ok(())
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
pub async fn resume_recording(
    state: tauri::State<'_, RecordingState>,
    licensing: tauri::State<'_, LicensingState>,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    require_paid_entitlement(&licensing)?;

    #[cfg(target_os = "macos")]
    {
        macos::do_resume_recording(&state, app.clone()).await?;
        let snap = macos::snapshot_recording_state(&state)?;
        let _ = app.emit("recording-state-changed", snap);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&state, &app);
        Err(AppError::NotSupported(
            "Not supported on this platform".into(),
        ))
    }
}

/// Debug command to show the meeting overlay without a real meeting.
/// Call from the browser console: `window.__TAURI__.core.invoke("debug_show_overlay", { appName: "Zoom" })`
#[tauri::command]
pub fn debug_show_overlay(app: tauri::AppHandle, app_name: String) {
    #[cfg(target_os = "macos")]
    crate::services::meeting_detector::show_overlay(&app, &app_name);

    #[cfg(not(target_os = "macos"))]
    let _ = (&app, &app_name);
}

#[tauri::command]
pub fn show_auto_stop_overlay(app: tauri::AppHandle, app_name: Option<String>) {
    #[cfg(target_os = "macos")]
    crate::services::meeting_detector::show_auto_stop_overlay(&app, app_name.as_deref());

    #[cfg(not(target_os = "macos"))]
    let _ = (&app, &app_name);
}

#[tauri::command]
pub fn hide_auto_stop_overlay(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    crate::services::meeting_detector::hide_auto_stop_overlay(&app);

    #[cfg(not(target_os = "macos"))]
    let _ = &app;
}

/// Invoked by the auto-stop overlay's "Keep recording" button. Bridges the
/// click into a Tauri event the main window's recording provider listens for.
#[tauri::command]
pub fn request_auto_stop_cancel(app: tauri::AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("auto-stop-cancel-requested", ());
}

/// Returns the app name of the currently detected meeting, or `null` when
/// the detector is idle. Used by the recording provider on session-start to
/// recover from the case where a meeting was already detected before the
/// recording began (otherwise no `meeting-detected` event arrives during the
/// session, and auto-stop is never armed).
#[tauri::command]
pub fn get_current_meeting_app(
    state: tauri::State<'_, crate::models::meeting_detection::MeetingDetectorState>,
) -> Option<String> {
    state
        .current_app
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

/// Diagnostic: returns "bundle.id: window title" for every on-screen window
/// the meeting detector can see. Call from dev console:
/// `await window.__TAURI__.core.invoke("debug_dump_windows")`
#[tauri::command]
pub fn debug_dump_windows() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        crate::services::meeting_detector::dump_windows()
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec!["debug_dump_windows: macOS only".into()]
    }
}

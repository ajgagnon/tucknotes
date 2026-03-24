mod commands;
pub mod errors;
pub mod models;
pub mod services;

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::{Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder};

use commands::appearance::*;
use commands::meetings::*;
use commands::models::*;
use commands::permissions::*;
use commands::recording::*;
use commands::summarization::*;
use models::meeting_detection::MeetingDetectorState;
use models::{PcmAccumulator, RecordingState};
use services::database::DatabaseState;
use services::summarization::{SummarizationService, SummarizationState};
#[cfg(target_os = "macos")]
use services::transcription::{TranscriptionService, TranscriptionState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    let recording_state = RecordingState {
        capture: Mutex::new(None),
        voice_capture: Mutex::new(None),
        accumulator: Arc::new(Mutex::new(PcmAccumulator::new())),
        cancel_token: Mutex::new(None),
        session_id: Mutex::new(None),
        started_at: Mutex::new(None),
        wall_time_offset_secs: Mutex::new(0.0),
        transcribe_task: Mutex::new(None),
        finalize_in_progress: Arc::new(AtomicBool::new(false)),
    };

    #[cfg(not(target_os = "macos"))]
    let recording_state = RecordingState;

    #[cfg(target_os = "macos")]
    let transcription_state = TranscriptionState {
        service: Arc::new(TranscriptionService::new()),
    };

    let recording_active = Arc::new(AtomicBool::new(false));

    let detector_state = MeetingDetectorState {
        cancel_token: Mutex::new(None),
        recording_active: Arc::clone(&recording_active),
    };

    let summarization_state = SummarizationState {
        service: Arc::new(
            SummarizationService::new().expect("failed to init summarization backend"),
        ),
        active_meeting_id: Mutex::new(None),
        pending_queue: Mutex::new(VecDeque::new()),
    };

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Initialize database
            let base_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&base_dir).expect("failed to create app data dir");
            let conn = services::database::open_db(&base_dir)
                .expect("failed to open database");
            app.manage(DatabaseState {
                conn: Mutex::new(conn),
            });

            let win_builder =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                    .title("Grain")
                    .inner_size(1024.0, 768.0)
                    .min_inner_size(600.0, 400.0)
                    .transparent(true);

            #[cfg(target_os = "macos")]
            let win_builder = win_builder
                .title_bar_style(TitleBarStyle::Overlay)
                .hidden_title(true)
                .traffic_light_position(tauri::LogicalPosition::new(28.0, 34.0));

            let window = win_builder.build().unwrap();

            #[cfg(target_os = "macos")]
            {
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
                apply_vibrancy(&window, NSVisualEffectMaterial::Sidebar, None, None)
                    .expect("Failed to apply vibrancy");
            }

            // Start meeting detection background loop
            #[cfg(target_os = "macos")]
            {
                let det: tauri::State<'_, MeetingDetectorState> = app.state();
                let recording_flag = Arc::clone(&det.recording_active);
                services::meeting_detector::start_detection_loop(
                    app.handle().clone(),
                    recording_flag,
                );
            }

            Ok(())
        })
        .manage(recording_state)
        .manage(detector_state)
        .manage(summarization_state);

    #[cfg(target_os = "macos")]
    {
        builder = builder.manage(transcription_state);
    }

    builder
        .invoke_handler(tauri::generate_handler![
            set_app_theme,
            check_screen_recording_permission,
            request_screen_recording_permission,
            open_screen_recording_settings,
            check_microphone_permission,
            request_microphone_permission,
            open_microphone_settings,
            open_sound_settings,
            check_accessibility_permission,
            request_accessibility_permission,
            open_accessibility_settings,
            start_recording,
            stop_recording,
            pause_recording,
            resume_recording,
            get_recording_state,
            debug_show_overlay,
            list_meetings,
            get_meeting,
            create_meeting_document,
            delete_meeting,
            list_available_models,
            get_model_status,
            download_model,
            get_selected_model,
            set_selected_model,
            list_available_llm_models,
            get_llm_model_status,
            download_llm_model,
            get_selected_llm_model,
            set_selected_llm_model,
            summarize_meeting,
            get_summarization_queue,
            update_meeting_title,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

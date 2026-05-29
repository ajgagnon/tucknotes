mod commands;
pub mod errors;
pub mod models;
pub mod services;

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::{Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder};

use commands::appearance::*;
use commands::chatbot::*;
use commands::licensing::*;
use commands::meetings::*;
use commands::models::*;
use commands::permissions::*;
use commands::recording::*;
use commands::summarization::*;
use models::licensing::LicensingState;
use models::meeting_detection::MeetingDetectorState;
use models::{PcmAccumulator, RecordingState};
use services::database::DatabaseState;
use services::licensing as licensing_svc;
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
        current_app: Arc::new(Mutex::new(None)),
    };

    let summarization_state = SummarizationState {
        service: Arc::new(
            SummarizationService::new().expect("failed to init summarization backend"),
        ),
        active_meeting_id: Mutex::new(None),
        pending_queue: Mutex::new(VecDeque::new()),
        llm_interrupt: Arc::new(AtomicBool::new(false)),
    };

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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

            // Initialize licensing storage (creates license.json on first launch
            // so the trial clock starts now).
            let license_storage = licensing_svc::init_storage(app.handle())
                .expect("failed to init license storage");
            app.manage(LicensingState::new(license_storage));

            let win_builder =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                    .title("TuckNotes")
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
                let current_app = Arc::clone(&det.current_app);
                services::meeting_detector::start_detection_loop(
                    app.handle().clone(),
                    recording_flag,
                    current_app,
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
            show_auto_stop_overlay,
            hide_auto_stop_overlay,
            request_auto_stop_cancel,
            get_current_meeting_app,
            debug_dump_windows,
            list_meetings,
            get_meeting,
            update_meeting_document_body,
            delete_meeting,
            list_available_models,
            get_model_status,
            download_model,
            get_selected_model,
            set_selected_model,
            remove_model,
            get_whisper_model_file_path,
            list_available_llm_models,
            get_llm_model_status,
            download_llm_model,
            get_selected_llm_model,
            set_selected_llm_model,
            remove_llm_model,
            get_llm_model_file_path,
            summarize_meeting,
            get_summarization_queue,
            update_meeting_title,
            list_summary_templates,
            get_default_template,
            set_default_template,
            chat_send_message,
            chat_stop,
            get_license_status,
            activate_license_key,
            deactivate_license,
            revalidate_license,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

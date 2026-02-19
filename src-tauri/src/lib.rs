mod commands;
pub mod errors;
pub mod models;
pub mod services;

use std::sync::{Arc, Mutex};

use commands::models::*;
use commands::permissions::*;
use commands::recording::*;
use models::{PcmAccumulator, RecordingState};
#[cfg(target_os = "macos")]
use services::transcription::{TranscriptionService, TranscriptionState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    let recording_state = RecordingState {
        capture: Mutex::new(None),
        accumulator: Arc::new(Mutex::new(PcmAccumulator::new())),
        cancel_token: Mutex::new(None),
    };

    #[cfg(not(target_os = "macos"))]
    let recording_state = RecordingState;

    #[cfg(target_os = "macos")]
    let transcription_state = TranscriptionState {
        service: Arc::new(TranscriptionService::new()),
    };

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(recording_state);

    #[cfg(target_os = "macos")]
    {
        builder = builder.manage(transcription_state);
    }

    builder
        .invoke_handler(tauri::generate_handler![
            check_screen_recording_permission,
            request_screen_recording_permission,
            open_screen_recording_settings,
            check_microphone_permission,
            request_microphone_permission,
            open_microphone_settings,
            start_recording,
            stop_recording,
            list_available_models,
            get_model_status,
            download_model,
            get_selected_model,
            set_selected_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

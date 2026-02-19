mod commands;
pub mod errors;
pub mod models;
pub mod services;

use std::sync::Mutex;

use commands::models::*;
use commands::permissions::*;
use commands::recording::*;
use models::RecordingState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    let recording_state = RecordingState {
        capture: Mutex::new(None),
    };

    #[cfg(not(target_os = "macos"))]
    let recording_state = RecordingState;

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(recording_state)
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

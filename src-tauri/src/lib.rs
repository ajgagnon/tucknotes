mod commands;
pub mod errors;
pub mod models;
pub mod services;

use std::sync::{Arc, Mutex};

use tauri::{TitleBarStyle, WebviewUrl, WebviewWindowBuilder};

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
        .setup(|app| {
            let win_builder =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                    .title("Grain")
                    .inner_size(1024.0, 768.0)
                    .transparent(true);

            #[cfg(target_os = "macos")]
            let win_builder = win_builder
                .title_bar_style(TitleBarStyle::Overlay)
                .hidden_title(true)
                .traffic_light_position(tauri::LogicalPosition::new(16.0, 24.0));

            let window = win_builder.build().unwrap();

            #[cfg(target_os = "macos")]
            {
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
                apply_vibrancy(&window, NSVisualEffectMaterial::Sidebar, None, None)
                    .expect("Failed to apply vibrancy");
            }

            Ok(())
        })
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

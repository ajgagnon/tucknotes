#[cfg(target_os = "macos")]
mod audio;

#[cfg(target_os = "macos")]
mod macos_permissions {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGPreflightScreenCaptureAccess() -> bool;
        pub fn CGRequestScreenCaptureAccess() -> bool;
    }

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    use objc2::msg_send;
    use objc2::runtime::AnyClass;
    use objc2_foundation::NSString;

    /// 0 = notDetermined, 1 = restricted, 2 = denied, 3 = authorized
    pub fn microphone_authorization_status() -> isize {
        unsafe {
            let cls = AnyClass::get(c"AVCaptureDevice").unwrap();
            let media_type = NSString::from_str("soun"); // AVMediaTypeAudio
            msg_send![cls, authorizationStatusForMediaType: &*media_type]
        }
    }

    pub fn request_microphone_access() -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(Some(tx));

        unsafe {
            let cls = AnyClass::get(c"AVCaptureDevice").unwrap();
            let media_type = NSString::from_str("soun");
            let block = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
                if let Some(tx) = tx.lock().unwrap().take() {
                    let _ = tx.send(granted.as_bool());
                }
            });
            let _: () = msg_send![
                cls,
                requestAccessForMediaType: &*media_type,
                completionHandler: &*block
            ];
        }

        rx.recv_timeout(std::time::Duration::from_secs(60))
            .unwrap_or(false)
    }
}

use std::sync::Mutex;
use tauri::Emitter;

// ── Shared app state ──────────────────────────────────────────────

#[cfg(target_os = "macos")]
struct RecordingState {
    capture: Mutex<Option<audio::AudioCapture>>,
}

#[cfg(not(target_os = "macos"))]
struct RecordingState;

// ── Screen recording permission commands ──────────────────────────

#[tauri::command]
fn check_screen_recording_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { macos_permissions::CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
fn request_screen_recording_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { macos_permissions::CGRequestScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
fn open_screen_recording_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
    }
}

// ── Microphone permission commands ────────────────────────────────

#[tauri::command]
fn check_microphone_permission() -> String {
    #[cfg(target_os = "macos")]
    {
        let status = macos_permissions::microphone_authorization_status();
        match status {
            0 => "not_determined".into(),
            1 => "restricted".into(),
            2 => "denied".into(),
            3 => "authorized".into(),
            _ => "unknown".into(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        "authorized".into()
    }
}

#[tauri::command]
fn request_microphone_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos_permissions::request_microphone_access()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
fn open_microphone_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .spawn();
    }
}

// ── Recording commands ────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct AudioChunkEvent {
    sample_count: usize,
    rms: f32,
    source: String,
    timestamp: f64,
}

#[tauri::command]
async fn start_recording(
    state: tauri::State<'_, RecordingState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut guard = state.capture.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Err("Already recording".into());
        }

        let (capture, mut rx) = audio::AudioCapture::start().map_err(|e| e.to_string())?;
        *guard = Some(capture);
        drop(guard);

        tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                let source = match chunk.source {
                    audio::AudioSource::SystemAudio => "system",
                    audio::AudioSource::Microphone => "microphone",
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
        Err("Recording is only supported on macOS".into())
    }
}

#[tauri::command]
async fn stop_recording(state: tauri::State<'_, RecordingState>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut guard = state.capture.lock().map_err(|e| e.to_string())?;
        if let Some(mut capture) = guard.take() {
            capture.stop().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = &state;
        Err("Recording is only supported on macOS".into())
    }
}

// ── App entry ─────────────────────────────────────────────────────

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

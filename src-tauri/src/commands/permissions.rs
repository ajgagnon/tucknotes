#[tauri::command]
pub fn check_screen_recording_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { crate::services::permissions::CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub fn request_screen_recording_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { crate::services::permissions::CGRequestScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub fn open_screen_recording_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
    }
}

#[tauri::command]
pub fn check_microphone_permission() -> String {
    #[cfg(target_os = "macos")]
    {
        let status = crate::services::permissions::microphone_authorization_status();
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
pub fn request_microphone_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::services::permissions::request_microphone_access()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub fn open_microphone_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .spawn();
    }
}

#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::services::permissions::check_accessibility()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub fn request_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::services::permissions::request_accessibility()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

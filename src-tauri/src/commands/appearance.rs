use tauri::{command, AppHandle, Manager};

#[command]
pub fn set_app_theme(app: AppHandle, theme: String) {
    if let Some(window) = app.get_webview_window("main") {
        let tauri_theme = match theme.as_str() {
            "light" => Some(tauri::Theme::Light),
            "dark" => Some(tauri::Theme::Dark),
            _ => None, // "system" → follow OS
        };
        let _ = window.set_theme(tauri_theme);
    }
}

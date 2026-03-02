use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    CaptureFailed(String),
    ConfigError(String),
    DownloadFailed(String),
    IoError(String),
    InvalidModel(String),
    TranscriptionFailed(String),
    DatabaseError(String),
    LockPoisoned(String),
    NotSupported(String),
    PermissionDenied(String),
    SummarizationFailed(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AppError::CaptureFailed(msg) => write!(f, "Capture failed: {}", msg),
            AppError::ConfigError(msg) => write!(f, "Config error: {}", msg),
            AppError::DownloadFailed(msg) => write!(f, "Download failed: {}", msg),
            AppError::IoError(msg) => write!(f, "IO error: {}", msg),
            AppError::InvalidModel(msg) => write!(f, "Invalid model: {}", msg),
            AppError::TranscriptionFailed(msg) => write!(f, "Transcription failed: {}", msg),
            AppError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            AppError::LockPoisoned(msg) => write!(f, "Lock poisoned: {}", msg),
            AppError::NotSupported(msg) => write!(f, "Not supported: {}", msg),
            AppError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            AppError::SummarizationFailed(msg) => write!(f, "Summarization failed: {}", msg),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::IoError(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::ConfigError(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::DownloadFailed(e.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::DatabaseError(e.to_string())
    }
}

/// Lock a `Mutex`, converting a poisoned lock into `AppError::LockPoisoned`.
pub fn lock_or_err<T>(
    mutex: &std::sync::Mutex<T>,
) -> Result<std::sync::MutexGuard<'_, T>, AppError> {
    mutex
        .lock()
        .map_err(|_| AppError::LockPoisoned("Lock poisoned".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tagged_serialization_with_message() {
        let json = serde_json::to_value(AppError::CaptureFailed("no display".into())).unwrap();
        assert_eq!(json["kind"], "CaptureFailed");
        assert_eq!(json["message"], "no display");
    }

    #[test]
    fn tagged_serialization_lock_poisoned() {
        let json =
            serde_json::to_value(AppError::LockPoisoned("mutex poisoned".into())).unwrap();
        assert_eq!(json["kind"], "LockPoisoned");
        assert_eq!(json["message"], "mutex poisoned");
    }

    #[test]
    fn tagged_serialization_permission_denied() {
        let json =
            serde_json::to_value(AppError::PermissionDenied("no screen access".into())).unwrap();
        assert_eq!(json["kind"], "PermissionDenied");
        assert_eq!(json["message"], "no screen access");
    }

    #[test]
    fn display_variant_with_message() {
        assert_eq!(
            AppError::CaptureFailed("timeout".into()).to_string(),
            "Capture failed: timeout"
        );
    }

    #[test]
    fn display_not_supported() {
        assert_eq!(
            AppError::NotSupported("this platform".into()).to_string(),
            "Not supported: this platform"
        );
    }
}

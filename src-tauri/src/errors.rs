use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    CaptureFailed(String),
    ConfigError(String),
    DownloadFailed(String),
    IoError(String),
    InvalidModel(String),
    LockPoisoned,
    NotSupported,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AppError::CaptureFailed(msg) => write!(f, "Capture failed: {}", msg),
            AppError::ConfigError(msg) => write!(f, "Config error: {}", msg),
            AppError::DownloadFailed(msg) => write!(f, "Download failed: {}", msg),
            AppError::IoError(msg) => write!(f, "IO error: {}", msg),
            AppError::InvalidModel(msg) => write!(f, "Invalid model: {}", msg),
            AppError::LockPoisoned => write!(f, "Lock poisoned"),
            AppError::NotSupported => write!(f, "Not supported on this platform"),
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
    fn tagged_serialization_unit_variant() {
        let json = serde_json::to_value(AppError::LockPoisoned).unwrap();
        assert_eq!(json["kind"], "LockPoisoned");
        assert!(json.get("message").is_none());
    }

    #[test]
    fn display_variant_with_message() {
        assert_eq!(
            AppError::CaptureFailed("timeout".into()).to_string(),
            "Capture failed: timeout"
        );
    }

    #[test]
    fn display_unit_variant() {
        assert_eq!(
            AppError::NotSupported.to_string(),
            "Not supported on this platform"
        );
    }
}

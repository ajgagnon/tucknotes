use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    CaptureFailed(String),
    LockPoisoned,
    NotSupported,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AppError::CaptureFailed(msg) => write!(f, "Capture failed: {}", msg),
            AppError::LockPoisoned => write!(f, "Lock poisoned"),
            AppError::NotSupported => write!(f, "Not supported on this platform"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_failed_serializes_with_kind_and_message() {
        let err = AppError::CaptureFailed("no display".into());
        let json: serde_json::Value = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "CaptureFailed");
        assert_eq!(json["message"], "no display");
    }

    #[test]
    fn lock_poisoned_serializes_as_kind_only() {
        let err = AppError::LockPoisoned;
        let json: serde_json::Value = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "LockPoisoned");
        assert!(json.get("message").is_none());
    }

    #[test]
    fn not_supported_serializes_as_kind_only() {
        let err = AppError::NotSupported;
        let json: serde_json::Value = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "NotSupported");
        assert!(json.get("message").is_none());
    }

    #[test]
    fn display_capture_failed() {
        let err = AppError::CaptureFailed("timeout".into());
        assert_eq!(err.to_string(), "Capture failed: timeout");
    }

    #[test]
    fn display_lock_poisoned() {
        assert_eq!(AppError::LockPoisoned.to_string(), "Lock poisoned");
    }

    #[test]
    fn display_not_supported() {
        assert_eq!(
            AppError::NotSupported.to_string(),
            "Not supported on this platform"
        );
    }
}

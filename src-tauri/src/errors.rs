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

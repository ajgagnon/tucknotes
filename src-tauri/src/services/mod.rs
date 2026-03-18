pub mod audio;
pub mod database;
pub mod model_manager;
pub mod summarization;
pub mod vad;

#[cfg(target_os = "macos")]
pub mod audio_capture;
#[cfg(target_os = "macos")]
pub mod voice_capture;
#[cfg(target_os = "macos")]
pub mod permissions;
#[cfg(target_os = "macos")]
pub mod meeting_detector;
#[cfg(target_os = "macos")]
pub mod transcription;

pub mod audio;
pub mod model_manager;
pub mod vad;

#[cfg(target_os = "macos")]
pub mod audio_capture;
#[cfg(target_os = "macos")]
pub mod permissions;
#[cfg(target_os = "macos")]
pub mod transcription;

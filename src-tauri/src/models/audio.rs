use std::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize)]
pub enum AudioSource {
    SystemAudio,
    Microphone,
}

#[derive(Clone, serde::Serialize)]
pub struct AudioChunkEvent {
    pub sample_count: usize,
    pub rms: f32,
    pub source: String,
    pub timestamp: f64,
}

#[cfg(target_os = "macos")]
pub struct RecordingState {
    pub capture: Mutex<Option<crate::services::audio_capture::AudioCapture>>,
}

#[cfg(not(target_os = "macos"))]
pub struct RecordingState;

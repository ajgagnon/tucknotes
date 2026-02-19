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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_source_serializes_variants() {
        let sys = serde_json::to_value(AudioSource::SystemAudio).unwrap();
        assert_eq!(sys, "SystemAudio");

        let mic = serde_json::to_value(AudioSource::Microphone).unwrap();
        assert_eq!(mic, "Microphone");
    }

    #[test]
    fn audio_chunk_event_serializes_all_fields() {
        let event = AudioChunkEvent {
            sample_count: 512,
            rms: 0.042,
            source: "system".into(),
            timestamp: 1.5,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["sample_count"], 512);
        assert!((json["rms"].as_f64().unwrap() - 0.042).abs() < 1e-3);
        assert_eq!(json["source"], "system");
        assert!((json["timestamp"].as_f64().unwrap() - 1.5).abs() < f64::EPSILON);
    }
}

use std::sync::{Arc, Mutex};

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

#[derive(Clone, serde::Serialize)]
pub struct TranscriptEvent {
    pub text: String,
    pub source: String,
    pub timestamp_ms: u64,
}

pub struct AccumulatedAudio {
    pub samples: Vec<f32>,
    pub start_timestamp: f64,
}

pub struct PcmAccumulator {
    system_buf: Vec<f32>,
    mic_buf: Vec<f32>,
    system_start_ts: Option<f64>,
    mic_start_ts: Option<f64>,
}

impl PcmAccumulator {
    pub fn new() -> Self {
        Self {
            system_buf: Vec::new(),
            mic_buf: Vec::new(),
            system_start_ts: None,
            mic_start_ts: None,
        }
    }

    pub fn append(&mut self, source: &AudioSource, samples: &[f32], timestamp: f64) {
        match source {
            AudioSource::SystemAudio => {
                self.system_start_ts.get_or_insert(timestamp);
                self.system_buf.extend_from_slice(samples);
            }
            AudioSource::Microphone => {
                self.mic_start_ts.get_or_insert(timestamp);
                self.mic_buf.extend_from_slice(samples);
            }
        }
    }

    /// Flush both buffers, returning accumulated audio for each source.
    /// Uses `std::mem::take` to swap buffers with empty Vecs (zero-copy).
    pub fn flush(&mut self) -> (Option<AccumulatedAudio>, Option<AccumulatedAudio>) {
        let system = if self.system_buf.is_empty() {
            None
        } else {
            Some(AccumulatedAudio {
                samples: std::mem::take(&mut self.system_buf),
                start_timestamp: self.system_start_ts.take().unwrap_or(0.0),
            })
        };

        let mic = if self.mic_buf.is_empty() {
            None
        } else {
            Some(AccumulatedAudio {
                samples: std::mem::take(&mut self.mic_buf),
                start_timestamp: self.mic_start_ts.take().unwrap_or(0.0),
            })
        };

        (system, mic)
    }
}

#[cfg(target_os = "macos")]
pub struct RecordingState {
    pub capture: Mutex<Option<crate::services::audio_capture::AudioCapture>>,
    pub accumulator: Arc<Mutex<PcmAccumulator>>,
    pub cancel_token: Mutex<Option<tokio_util::sync::CancellationToken>>,
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

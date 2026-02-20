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
    pub is_provisional: bool,
}

pub struct AccumulatedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub start_timestamp: f64,
}

pub struct PcmAccumulator {
    system_buf: Vec<f32>,
    mic_buf: Vec<f32>,
    system_start_ts: Option<f64>,
    mic_start_ts: Option<f64>,
    system_rate: u32,
    mic_rate: u32,
}

impl PcmAccumulator {
    pub fn new() -> Self {
        Self {
            system_buf: Vec::new(),
            mic_buf: Vec::new(),
            system_start_ts: None,
            mic_start_ts: None,
            system_rate: 16000,
            mic_rate: 48000,
        }
    }

    pub fn append(&mut self, source: &AudioSource, samples: &[f32], timestamp: f64, sample_rate: u32) {
        match source {
            AudioSource::SystemAudio => {
                self.system_start_ts.get_or_insert(timestamp);
                self.system_rate = sample_rate;
                self.system_buf.extend_from_slice(samples);
            }
            AudioSource::Microphone => {
                self.mic_start_ts.get_or_insert(timestamp);
                self.mic_rate = sample_rate;
                self.mic_buf.extend_from_slice(samples);
            }
        }
    }

    fn duration_secs(buf: &[f32], rate: u32) -> f64 {
        if rate == 0 { 0.0 } else { buf.len() as f64 / rate as f64 }
    }

    fn flush_buf(
        buf: &mut Vec<f32>,
        start_ts: &mut Option<f64>,
        rate: u32,
        keep_secs: Option<f64>,
    ) -> Option<AccumulatedAudio> {
        if buf.is_empty() {
            return None;
        }
        let ts = start_ts.take().unwrap_or(0.0);
        let full = std::mem::take(buf);
        if let Some(keep) = keep_secs {
            let keep_samples = (keep * rate as f64) as usize;
            if keep_samples > 0 && full.len() > keep_samples {
                *buf = full[full.len() - keep_samples..].to_vec();
                let full_dur = full.len() as f64 / rate as f64;
                *start_ts = Some(ts + full_dur - keep);
            }
        }
        Some(AccumulatedAudio {
            samples: full,
            sample_rate: rate,
            start_timestamp: ts,
        })
    }

    /// Flush both buffers, returning accumulated audio for each source.
    pub fn flush(&mut self) -> (Option<AccumulatedAudio>, Option<AccumulatedAudio>) {
        let system = Self::flush_buf(&mut self.system_buf, &mut self.system_start_ts, self.system_rate, None);
        let mic = Self::flush_buf(&mut self.mic_buf, &mut self.mic_start_ts, self.mic_rate, None);
        (system, mic)
    }

    /// Clone current buffers without clearing (for provisional transcription).
    pub fn peek(&self) -> (Option<AccumulatedAudio>, Option<AccumulatedAudio>) {
        let system = if self.system_buf.is_empty() {
            None
        } else {
            Some(AccumulatedAudio {
                samples: self.system_buf.clone(),
                sample_rate: self.system_rate,
                start_timestamp: self.system_start_ts.unwrap_or(0.0),
            })
        };
        let mic = if self.mic_buf.is_empty() {
            None
        } else {
            Some(AccumulatedAudio {
                samples: self.mic_buf.clone(),
                sample_rate: self.mic_rate,
                start_timestamp: self.mic_start_ts.unwrap_or(0.0),
            })
        };
        (system, mic)
    }

    /// Flush buffers but retain `keep_secs` of audio as overlap for context.
    pub fn flush_with_overlap(
        &mut self,
        keep_secs: f64,
    ) -> (Option<AccumulatedAudio>, Option<AccumulatedAudio>) {
        let system = Self::flush_buf(&mut self.system_buf, &mut self.system_start_ts, self.system_rate, Some(keep_secs));
        let mic = Self::flush_buf(&mut self.mic_buf, &mut self.mic_start_ts, self.mic_rate, Some(keep_secs));
        (system, mic)
    }

    /// Returns the maximum duration in seconds across both buffers.
    pub fn max_duration_secs(&self) -> f64 {
        let sys = Self::duration_secs(&self.system_buf, self.system_rate);
        let mic = Self::duration_secs(&self.mic_buf, self.mic_rate);
        sys.max(mic)
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

use aec3::voip::{VoipAec3, VoipAec3Error};

use crate::services::audio::resample_to_16khz;

const SAMPLE_RATE: usize = 16_000;
const FRAME_SIZE: usize = SAMPLE_RATE / 100; // 160 samples = 10 ms

pub struct EchoCanceller {
    pipeline: VoipAec3,
    system_buf: Vec<f32>,
    mic_buf: Vec<f32>,
}

impl EchoCanceller {
    pub fn new() -> Result<Self, VoipAec3Error> {
        let pipeline = VoipAec3::builder(SAMPLE_RATE, 1, 1)
            .enable_high_pass(true)
            .initial_delay_ms(0)
            .build()?;
        Ok(Self {
            pipeline,
            system_buf: Vec::new(),
            mic_buf: Vec::new(),
        })
    }

    /// Feed system audio (16 kHz mono) as the far-end reference.
    /// Buffers internally and drains complete 10 ms frames.
    pub fn feed_system(&mut self, samples: &[f32]) {
        self.system_buf.extend_from_slice(samples);
        while self.system_buf.len() >= FRAME_SIZE {
            let frame: Vec<f32> = self.system_buf.drain(..FRAME_SIZE).collect();
            let _ = self.pipeline.handle_render_frame(&frame);
        }
    }

    /// Feed mic audio (any sample rate). Resamples to 16 kHz, runs AEC,
    /// and returns the echo-cancelled output at 16 kHz.
    pub fn feed_mic(&mut self, samples: &[f32], sample_rate: u32) -> Vec<f32> {
        let resampled = resample_to_16khz(samples.to_vec(), sample_rate);
        self.mic_buf.extend_from_slice(&resampled);

        let mut output = Vec::new();
        while self.mic_buf.len() >= FRAME_SIZE {
            let frame: Vec<f32> = self.mic_buf.drain(..FRAME_SIZE).collect();
            let mut out_frame = vec![0.0f32; FRAME_SIZE];
            if self.pipeline.process_capture_frame(&frame, false, &mut out_frame).is_ok() {
                output.extend_from_slice(&out_frame);
            } else {
                // Fallback: pass through unprocessed frame
                output.extend_from_slice(&frame);
            }
        }
        output
    }
}

use screencapturekit::prelude::*;
use tokio::sync::mpsc;

use crate::models::AudioSource;

pub struct AudioChunk {
    pub pcm_data: Vec<f32>,
    pub sample_rate: u32,
    pub source: AudioSource,
    pub timestamp: f64,
}

pub struct AudioCapture {
    stream: Option<SCStream>,
    _tx: mpsc::Sender<AudioChunk>,
}

struct CaptureHandler {
    tx: mpsc::Sender<AudioChunk>,
}

impl SCStreamOutputTrait for CaptureHandler {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        of_type: SCStreamOutputType,
    ) {
        let source = match of_type {
            SCStreamOutputType::Audio => AudioSource::SystemAudio,
            SCStreamOutputType::Microphone => AudioSource::Microphone,
            SCStreamOutputType::Screen => return,
        };

        if let Some(pcm) = extract_pcm_f32(&sample_buffer) {
            let chunk = AudioChunk {
                pcm_data: pcm,
                sample_rate: 16000,
                source,
                timestamp: sample_buffer
                    .presentation_timestamp()
                    .as_seconds()
                    .unwrap_or(0.0),
            };
            let _ = self.tx.try_send(chunk);
        }
    }
}

fn extract_pcm_f32(sample: &CMSampleBuffer) -> Option<Vec<f32>> {
    let buffer_list = sample.audio_buffer_list()?;
    let mut samples = Vec::new();
    for buffer in buffer_list.iter() {
        let bytes = buffer.data();
        let floats: &[f32] = bytemuck::cast_slice(bytes);
        samples.extend_from_slice(floats);
    }
    Some(samples)
}

impl AudioCapture {
    pub fn start() -> Result<(Self, mpsc::Receiver<AudioChunk>), Box<dyn std::error::Error>> {
        let content = SCShareableContent::get()?;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or("No display found")?;

        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();

        // Minimum video surface (required by SCK even for audio-only capture).
        // 1 fps at 2x2 to keep overhead negligible.
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_minimum_frame_interval(&CMTime::new(1, 1))
            .with_captures_audio(true)
            .with_sample_rate(16000)
            .with_channel_count(1)
            .with_captures_microphone(true); // macOS 15+ — single stream with mic + system audio

        let (tx, rx) = mpsc::channel(1024);

        let mut stream = SCStream::new(&filter, &config);
        stream.add_output_handler(
            CaptureHandler { tx: tx.clone() },
            SCStreamOutputType::Audio,
        );
        stream.add_output_handler(
            CaptureHandler { tx: tx.clone() },
            SCStreamOutputType::Microphone,
        );
        stream.start_capture()?;

        Ok((
            AudioCapture {
                stream: Some(stream),
                _tx: tx,
            },
            rx,
        ))
    }

    pub fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(stream) = self.stream.take() {
            stream.stop_capture()?;
        }
        Ok(())
    }
}

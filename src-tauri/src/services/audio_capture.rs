use std::sync::atomic::{AtomicBool, Ordering};

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
    logged_system: AtomicBool,
    logged_mic: AtomicBool,
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

        // Read the actual audio format from the buffer's format description.
        let fmt = sample_buffer.format_description();
        let sample_rate = fmt
            .as_ref()
            .and_then(|f| f.audio_sample_rate())
            .unwrap_or(16000.0);
        let fmt_channels = fmt
            .as_ref()
            .and_then(|f| f.audio_channel_count())
            .unwrap_or(1);

        let Some(buffer_list) = sample_buffer.audio_buffer_list() else {
            return;
        };

        let n_buffers: usize = buffer_list.iter().count();
        let Some(first_buffer) = buffer_list.iter().next() else {
            return;
        };
        let buf_channels = first_buffer.number_channels;
        let bytes = first_buffer.data();
        let raw: &[f32] = bytemuck::cast_slice(bytes);
        let num_frames = sample_buffer.num_samples();

        // Extract mono audio:
        // - n_buffers >= 2: non-interleaved, first buffer = channel 0, already mono
        // - n_buffers == 1 with buf_channels >= 2: interleaved, deinterleave
        // - n_buffers == 1 with buf_channels == 1: mono, use as-is
        let pcm = if n_buffers >= 2 || buf_channels <= 1 {
            raw.to_vec()
        } else {
            // Interleaved multi-channel: average all channels per frame
            let ch = buf_channels as usize;
            raw.chunks_exact(ch)
                .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                .collect()
        };

        // Log the actual audio format once per source for diagnostics.
        let logged = match source {
            AudioSource::SystemAudio => &self.logged_system,
            AudioSource::Microphone => &self.logged_mic,
        };
        if !logged.swap(true, Ordering::Relaxed) {
            let source_name = match source {
                AudioSource::SystemAudio => "system",
                AudioSource::Microphone => "mic",
            };
            eprintln!(
                "[audio_capture] {source_name}: rate={sample_rate}Hz, fmt_ch={fmt_channels}, buf_ch={buf_channels}, n_buffers={n_buffers}, frames={num_frames}, raw_f32={}, mono={}",
                raw.len(), pcm.len()
            );
        }

        let chunk = AudioChunk {
            pcm_data: pcm,
            sample_rate: sample_rate as u32,
            source,
            timestamp: sample_buffer
                .presentation_timestamp()
                .as_seconds()
                .unwrap_or(0.0),
        };
        let _ = self.tx.try_send(chunk);
    }
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

        let handler = CaptureHandler {
            tx: tx.clone(),
            logged_system: AtomicBool::new(false),
            logged_mic: AtomicBool::new(false),
        };

        let mut stream = SCStream::new(&filter, &config);
        stream.add_output_handler(handler, SCStreamOutputType::Audio);
        // Mic handler needs its own instance
        let mic_handler = CaptureHandler {
            tx: tx.clone(),
            logged_system: AtomicBool::new(false),
            logged_mic: AtomicBool::new(false),
        };
        stream.add_output_handler(mic_handler, SCStreamOutputType::Microphone);
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

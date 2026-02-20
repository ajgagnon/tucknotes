use voice_activity_detector::VoiceActivityDetector;

const SPEECH_THRESHOLD: f32 = 0.5;

/// Compute the fraction of 512-sample windows that contain speech.
/// Input must be 16 kHz mono f32 PCM. Returns 0.0–1.0.
pub fn speech_ratio(samples: &[f32]) -> f32 {
    let Ok(mut vad) = VoiceActivityDetector::builder()
        .sample_rate(16000)
        .chunk_size(512usize)
        .build()
    else {
        return 1.0; // fail open — don't suppress audio if VAD fails
    };

    let mut speech_chunks = 0u32;
    let mut total_chunks = 0u32;

    for chunk in samples.chunks(512) {
        if chunk.len() < 512 {
            break;
        }
        let prob = vad.predict(chunk.iter().copied());
        total_chunks += 1;
        if prob > SPEECH_THRESHOLD {
            speech_chunks += 1;
        }
    }

    if total_chunks == 0 {
        return 0.0;
    }
    speech_chunks as f32 / total_chunks as f32
}

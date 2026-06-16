use tokio::sync::mpsc;

use crate::models::AudioSource;
use crate::services::audio_capture::AudioChunk;

// FFI declarations matching native/voice_capture.h
type VoiceCaptureCallback = extern "C" fn(
    context: *mut std::ffi::c_void,
    samples: *const f32,
    sample_count: u32,
    sample_rate: u32,
    timestamp: f64,
);

extern "C" {
    fn voice_capture_start(
        callback: VoiceCaptureCallback,
        context: *mut std::ffi::c_void,
        error_out: *mut std::ffi::c_char,
        error_out_len: i32,
    ) -> bool;
    fn voice_capture_stop();
}

/// Opaque context passed through the C callback back to Rust.
struct CallbackContext {
    tx: mpsc::Sender<AudioChunk>,
}

/// Called from the AVAudioEngine tap thread.
extern "C" fn on_audio_buffer(
    context: *mut std::ffi::c_void,
    samples: *const f32,
    sample_count: u32,
    sample_rate: u32,
    timestamp: f64,
) {
    if context.is_null() || samples.is_null() || sample_count == 0 {
        return;
    }
    let ctx = unsafe { &*(context as *const CallbackContext) };
    let pcm_data =
        unsafe { std::slice::from_raw_parts(samples, sample_count as usize) }.to_vec();

    let chunk = AudioChunk {
        pcm_data,
        sample_rate,
        source: AudioSource::Microphone,
        timestamp,
    };
    // Non-blocking send — drop chunk if channel is full rather than blocking the audio thread.
    let _ = ctx.tx.try_send(chunk);
}

/// Handle to a running VoiceCapture session. Stops capture on drop.
pub struct VoiceCapture {
    /// Prevent Send — the ObjC engine must be managed on the thread it was created.
    /// The Box keeps the context alive for the duration of capture; a raw pointer
    /// to it is held by the C callback. We stop the C engine before dropping the
    /// box, so the pointer is never dangling during a callback.
    _context: Box<CallbackContext>,
}

impl VoiceCapture {
    /// Start microphone capture using AVAudioEngine's plain input node.
    /// Captures in the hardware-native sample rate (reported per chunk); no
    /// echo cancellation and no system-audio ducking.
    /// Audio chunks are sent to `tx` tagged as `AudioSource::Microphone`.
    pub fn start(tx: mpsc::Sender<AudioChunk>) -> Result<Self, Box<dyn std::error::Error>> {
        let context = Box::new(CallbackContext { tx });
        // Lend a raw pointer to C. The Box stays alive in `VoiceCapture._context`.
        let raw: *mut CallbackContext = &*context as *const _ as *mut _;

        let mut err_buf = [0u8; 256];
        let ok = unsafe {
            voice_capture_start(
                on_audio_buffer,
                raw as *mut std::ffi::c_void,
                err_buf.as_mut_ptr() as *mut std::ffi::c_char,
                err_buf.len() as i32,
            )
        };

        if !ok {
            let end = err_buf.iter().position(|&b| b == 0).unwrap_or(err_buf.len());
            let reason = String::from_utf8_lossy(&err_buf[..end]);
            let reason = if reason.is_empty() {
                "unknown error".into()
            } else {
                reason
            };
            return Err(format!("Failed to start voice capture engine: {reason}").into());
        }

        Ok(VoiceCapture { _context: context })
    }
}

impl Drop for VoiceCapture {
    fn drop(&mut self) {
        // Stop the C engine first so the callback can no longer fire,
        // then the Box<CallbackContext> is dropped safely.
        unsafe { voice_capture_stop() };
    }
}

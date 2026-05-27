extern "C" {
    fn audio_output_is_builtin_speakers() -> bool;
}

/// True when the default macOS output device is the built-in speakers.
/// Returns false for headphones, Bluetooth, USB headsets, HDMI, AirPlay,
/// aggregate devices, and on any CoreAudio query failure.
pub fn is_builtin_speakers() -> bool {
    unsafe { audio_output_is_builtin_speakers() }
}

#ifndef AUDIO_OUTPUT_H
#define AUDIO_OUTPUT_H

#include <stdbool.h>

/// Returns true when the default macOS output device is the built-in
/// speakers (i.e. user is not on headphones / Bluetooth / USB headset /
/// HDMI / AirPlay / aggregate device).
///
/// On any CoreAudio query failure this returns false — the safe default
/// for our use case, since callers branch into a more conservative mode
/// (no voice processing, no ducking) when headphones are assumed.
bool audio_output_is_builtin_speakers(void);

#endif /* AUDIO_OUTPUT_H */

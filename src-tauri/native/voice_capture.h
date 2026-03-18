#ifndef VOICE_CAPTURE_H
#define VOICE_CAPTURE_H

#include <stdint.h>
#include <stdbool.h>

/// Callback invoked on the audio tap thread with PCM float32 mono samples.
/// `context` is the opaque pointer passed to `voice_capture_start`.
typedef void (*VoiceCaptureCallback)(
    void *context,
    const float *samples,
    uint32_t sample_count,
    uint32_t sample_rate,
    double timestamp
);

/// Start capturing microphone audio via AVAudioEngine with VoiceProcessingIO.
/// Returns true on success. The callback is invoked on a realtime audio thread.
bool voice_capture_start(VoiceCaptureCallback callback, void *context);

/// Stop capturing and release the audio engine.
void voice_capture_stop(void);

#endif /* VOICE_CAPTURE_H */

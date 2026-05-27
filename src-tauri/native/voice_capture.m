#import <AVFoundation/AVFoundation.h>
#import "voice_capture.h"

static AVAudioEngine *gEngine = nil;

bool voice_capture_start(VoiceCaptureCallback callback, void *context) {
    if (gEngine) {
        return false; // already running
    }

    AVAudioEngine *engine = [[AVAudioEngine alloc] init];

    // Use the plain input node (no VoiceProcessingIO). Voice processing would
    // give us hardware AEC/noise suppression/AGC, but it also unconditionally
    // ducks other system audio while the engine is running, which is far more
    // disruptive than the benefit is worth for our transcription use case.
    AVAudioFormat *inputFormat = [engine.inputNode inputFormatForBus:0];
    NSLog(@"[voice_capture] input format: %@", inputFormat);

    // Install a tap on the input node to receive PCM buffers.
    [engine.inputNode installTapOnBus:0
                           bufferSize:4096
                               format:inputFormat
                                block:^(AVAudioPCMBuffer *buffer, AVAudioTime *when) {
        if (!callback || buffer.frameLength == 0) return;

        const float *channelData = buffer.floatChannelData[0];
        uint32_t frameCount = (uint32_t)buffer.frameLength;
        uint32_t sampleRate = (uint32_t)buffer.format.sampleRate;
        double timestamp = (double)when.sampleTime / sampleRate;

        callback(context, channelData, frameCount, sampleRate, timestamp);
    }];

    [engine prepare];

    NSError *startError = nil;
    if (![engine startAndReturnError:&startError]) {
        NSLog(@"[voice_capture] failed to start engine: %@", startError);
        [engine.inputNode removeTapOnBus:0];
        return false;
    }

    gEngine = engine;
    NSLog(@"[voice_capture] started");
    return true;
}

void voice_capture_stop(void) {
    if (!gEngine) return;

    [gEngine.inputNode removeTapOnBus:0];
    [gEngine stop];
    gEngine = nil;
    NSLog(@"[voice_capture] stopped");
}

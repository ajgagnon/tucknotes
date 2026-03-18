#import <AVFoundation/AVFoundation.h>
#import <AudioToolbox/AudioToolbox.h>
#import "voice_capture.h"

static AVAudioEngine *gEngine = nil;

/// Minimize VoiceProcessingIO audio ducking as much as the API allows.
/// Note: VoiceProcessingIO always ducks other audio to some degree —
/// there is no way to fully disable it. This is a known trade-off
/// for getting hardware-tuned AEC.
static void minimize_ducking(AVAudioEngine *engine) {
    if (@available(macOS 14.0, *)) {
        AudioUnit au = engine.inputNode.audioUnit;
        if (au) {
            AUVoiceIOOtherAudioDuckingConfiguration cfg;
            cfg.mEnableAdvancedDucking = true;
            cfg.mDuckingLevel = kAUVoiceIOOtherAudioDuckingLevelMin;
            AudioUnitSetProperty(
                au,
                kAUVoiceIOProperty_OtherAudioDuckingConfiguration,
                kAudioUnitScope_Global, 0,
                &cfg, sizeof(cfg)
            );
        }

        AVAudioVoiceProcessingOtherAudioDuckingConfiguration hlConfig;
        hlConfig.enableAdvancedDucking = YES;
        hlConfig.duckingLevel = AVAudioVoiceProcessingOtherAudioDuckingLevelMin;
        engine.inputNode.voiceProcessingOtherAudioDuckingConfiguration = hlConfig;
    }
}

bool voice_capture_start(VoiceCaptureCallback callback, void *context) {
    if (gEngine) {
        return false; // already running
    }

    AVAudioEngine *engine = [[AVAudioEngine alloc] init];

    // Enable voice processing on the input node — this wraps AUVoiceProcessingIO,
    // giving us hardware-tuned AEC, noise suppression, AGC, and the system
    // mic mode picker (Standard / Voice Isolation / Wide Spectrum).
    NSError *vpError = nil;
    if (![engine.inputNode setVoiceProcessingEnabled:YES error:&vpError]) {
        NSLog(@"[voice_capture] failed to enable voice processing: %@", vpError);
    }

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

    // Minimize ducking after engine start (AU must be instantiated first).
    minimize_ducking(engine);

    gEngine = engine;
    NSLog(@"[voice_capture] started with voice processing");
    return true;
}

void voice_capture_stop(void) {
    if (!gEngine) return;

    [gEngine.inputNode removeTapOnBus:0];
    [gEngine stop];
    gEngine = nil;
    NSLog(@"[voice_capture] stopped");
}

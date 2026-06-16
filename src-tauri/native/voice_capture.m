#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <string.h>
#import "voice_capture.h"

// Microphone capture via AVCaptureSession. Unlike AVAudioEngine, this opens
// only the input device — never the output — so it is immune to input/output
// sample-rate mismatches (e.g. a 48 kHz mic alongside 44.1 kHz speakers), and
// it uses no AUVoiceProcessingIO, so it never ducks other system audio. Echo
// between the mic and system streams is removed downstream at the transcript
// level (see services/dedup.rs).

@interface VCDelegate : NSObject <AVCaptureAudioDataOutputSampleBufferDelegate>
@property(nonatomic, assign) VoiceCaptureCallback callback;
@property(nonatomic, assign) void *context;
@end

@implementation VCDelegate
- (void)captureOutput:(AVCaptureOutput *)output
    didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer
           fromConnection:(AVCaptureConnection *)connection {
    VoiceCaptureCallback cb = self.callback;
    if (!cb || sampleBuffer == NULL || !CMSampleBufferIsValid(sampleBuffer)) {
        return;
    }

    CMFormatDescriptionRef fmt = CMSampleBufferGetFormatDescription(sampleBuffer);
    const AudioStreamBasicDescription *asbd =
        fmt ? CMAudioFormatDescriptionGetStreamBasicDescription(fmt) : NULL;
    // We request 32-bit float PCM below; bail if we somehow get something else.
    if (!asbd || !(asbd->mFormatFlags & kAudioFormatFlagIsFloat)) {
        return;
    }

    uint32_t sampleRate = (uint32_t)asbd->mSampleRate;
    uint32_t channels = asbd->mChannelsPerFrame ? asbd->mChannelsPerFrame : 1;

    CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
    double timestamp = CMTIME_IS_VALID(pts) ? CMTimeGetSeconds(pts) : 0.0;

    // Size-query then allocate so any channel/interleaving layout is handled.
    size_t ablSize = 0;
    if (CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer, &ablSize, NULL, 0, NULL, NULL, 0, NULL) != noErr ||
        ablSize == 0) {
        return;
    }
    AudioBufferList *abl = (AudioBufferList *)malloc(ablSize);
    if (!abl) {
        return;
    }
    CMBlockBufferRef blockBuffer = NULL;
    OSStatus status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
        sampleBuffer, NULL, abl, ablSize, NULL, NULL,
        kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment, &blockBuffer);
    if (status != noErr || blockBuffer == NULL || abl->mNumberBuffers == 0) {
        if (blockBuffer) CFRelease(blockBuffer);
        free(abl);
        return;
    }

    const float *buf0 = (const float *)abl->mBuffers[0].mData;
    uint32_t buf0Floats = abl->mBuffers[0].mDataByteSize / (uint32_t)sizeof(float);

    if (buf0 && buf0Floats > 0) {
        if (channels <= 1 || abl->mNumberBuffers > 1) {
            // Mono, or non-interleaved (channel 0 is its own buffer): pass directly.
            cb(self.context, buf0, buf0Floats, sampleRate, timestamp);
        } else {
            // Interleaved multi-channel: extract channel 0 into a mono buffer.
            uint32_t frameCount = buf0Floats / channels;
            float *mono = (float *)malloc((size_t)frameCount * sizeof(float));
            if (mono) {
                for (uint32_t i = 0; i < frameCount; i++) {
                    mono[i] = buf0[(size_t)i * channels];
                }
                cb(self.context, mono, frameCount, sampleRate, timestamp);
                free(mono);
            }
        }
    }

    CFRelease(blockBuffer);
    free(abl);
}
@end

static AVCaptureSession *gSession = nil;
static VCDelegate *gDelegate = nil;
static dispatch_queue_t gQueue = NULL;

static void set_error(char *error_out, int error_out_len, NSString *msg) {
    if (error_out && error_out_len > 0 && msg) {
        strlcpy(error_out, msg.UTF8String, (size_t)error_out_len);
    }
}

bool voice_capture_start(VoiceCaptureCallback callback,
                         void *context,
                         char *error_out,
                         int error_out_len) {
    if (gSession) {
        set_error(error_out, error_out_len, @"capture already running");
        return false;
    }

    AVCaptureDevice *device = [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeAudio];
    if (!device) {
        set_error(error_out, error_out_len, @"no microphone input device available");
        return false;
    }
    NSLog(@"[voice_capture] input device: %@", device.localizedName);

    NSError *inputError = nil;
    AVCaptureDeviceInput *input = [AVCaptureDeviceInput deviceInputWithDevice:device
                                                                       error:&inputError];
    if (!input) {
        set_error(error_out, error_out_len,
                  inputError.localizedDescription ?: @"failed to open microphone input");
        return false;
    }

    AVCaptureSession *session = [[AVCaptureSession alloc] init];
    if (![session canAddInput:input]) {
        set_error(error_out, error_out_len, @"cannot add microphone input to session");
        return false;
    }
    [session addInput:input];

    AVCaptureAudioDataOutput *output = [[AVCaptureAudioDataOutput alloc] init];
    // Request 32-bit float mono PCM at the device's native rate; downstream
    // resamples to 16 kHz.
    output.audioSettings = @{
        (NSString *)AVFormatIDKey : @(kAudioFormatLinearPCM),
        (NSString *)AVLinearPCMBitDepthKey : @32,
        (NSString *)AVLinearPCMIsFloatKey : @YES,
        (NSString *)AVLinearPCMIsNonInterleaved : @NO,
        (NSString *)AVNumberOfChannelsKey : @1,
    };

    VCDelegate *delegate = [[VCDelegate alloc] init];
    delegate.callback = callback;
    delegate.context = context;
    dispatch_queue_t queue =
        dispatch_queue_create("com.tucknotes.voicecapture", DISPATCH_QUEUE_SERIAL);
    [output setSampleBufferDelegate:delegate queue:queue];

    if (![session canAddOutput:output]) {
        set_error(error_out, error_out_len, @"cannot add audio output to session");
        return false;
    }
    [session addOutput:output];

    [session startRunning];
    if (!session.isRunning) {
        set_error(error_out, error_out_len, @"AVCaptureSession failed to start");
        return false;
    }

    // Retain session, delegate (output holds it unretained), and queue.
    gSession = session;
    gDelegate = delegate;
    gQueue = queue;
    NSLog(@"[voice_capture] started (AVCaptureSession)");
    return true;
}

void voice_capture_stop(void) {
    if (!gSession) return;

    [gSession stopRunning]; // blocks until stopped; no more delegate callbacks after this
    if (gDelegate) {
        gDelegate.callback = NULL;
        gDelegate.context = NULL;
    }
    gSession = nil;
    gDelegate = nil;
    gQueue = NULL;
    NSLog(@"[voice_capture] stopped");
}

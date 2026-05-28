#import <CoreAudio/CoreAudio.h>
#import "audio_output.h"

// Apple-defined four-char codes for the built-in output's data sources.
// 'ispk' = Internal Speakers, 'hdpn' = Headphones (3.5mm jack).
static const UInt32 kDataSourceInternalSpeakers = 'ispk';
static const UInt32 kDataSourceHeadphones = 'hdpn';

bool audio_output_is_builtin_speakers(void) {
    // 1. Default output device.
    AudioObjectPropertyAddress defaultOutAddr = {
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioDeviceID device = kAudioObjectUnknown;
    UInt32 size = sizeof(device);
    OSStatus st = AudioObjectGetPropertyData(
        kAudioObjectSystemObject, &defaultOutAddr, 0, NULL, &size, &device);
    if (st != noErr || device == kAudioObjectUnknown) {
        return false;
    }

    // 2. Transport type — must be built-in to even consider speakers.
    AudioObjectPropertyAddress transportAddr = {
        kAudioDevicePropertyTransportType,
        kAudioObjectPropertyScopeOutput,
        kAudioObjectPropertyElementMain
    };
    UInt32 transport = 0;
    size = sizeof(transport);
    st = AudioObjectGetPropertyData(device, &transportAddr, 0, NULL, &size, &transport);
    if (st != noErr) return false;
    if (transport != kAudioDeviceTransportTypeBuiltIn) {
        return false;
    }

    // 3. Active data source on the built-in device. On Macs with a 3.5mm
    // headphone jack, the same hardware unit exposes 'ispk' vs 'hdpn'.
    // If the device doesn't expose data sources (e.g. modern Macs without
    // a jack), assume internal speakers — built-in + no jack == speakers.
    AudioObjectPropertyAddress dataSourceAddr = {
        kAudioDevicePropertyDataSource,
        kAudioObjectPropertyScopeOutput,
        kAudioObjectPropertyElementMain
    };
    if (!AudioObjectHasProperty(device, &dataSourceAddr)) {
        return true;
    }

    UInt32 dataSource = 0;
    size = sizeof(dataSource);
    st = AudioObjectGetPropertyData(device, &dataSourceAddr, 0, NULL, &size, &dataSource);
    if (st != noErr) {
        // Property exists but we couldn't read it — fall back to safe default.
        return false;
    }

    if (dataSource == kDataSourceHeadphones) return false;
    if (dataSource == kDataSourceInternalSpeakers) return true;

    // Unknown data source on built-in device — treat as headphones to be safe.
    return false;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGPreflightScreenCaptureAccess() -> bool;
    pub fn CGRequestScreenCaptureAccess() -> bool;
}

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

use objc2::msg_send;
use objc2::runtime::AnyClass;
use objc2_foundation::NSString;

/// Returns the AVAuthorizationStatus for the microphone.
/// 0 = notDetermined, 1 = restricted, 2 = denied, 3 = authorized
pub fn microphone_authorization_status() -> isize {
    unsafe {
        let cls = AnyClass::get(c"AVCaptureDevice").unwrap();
        let media_type = NSString::from_str("soun"); // AVMediaTypeAudio
        msg_send![cls, authorizationStatusForMediaType: &*media_type]
    }
}

pub fn request_microphone_access() -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(Some(tx));

    unsafe {
        let cls = AnyClass::get(c"AVCaptureDevice").unwrap();
        let media_type = NSString::from_str("soun");
        let block = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(granted.as_bool());
            }
        });
        let _: () = msg_send![
            cls,
            requestAccessForMediaType: &*media_type,
            completionHandler: &*block
        ];
    }

    rx.recv_timeout(std::time::Duration::from_secs(60))
        .unwrap_or(false)
}

/// Check whether Screen Recording permission is granted (without prompting).
/// The meeting detector uses ScreenCaptureKit to enumerate window titles, which
/// requires this permission.
pub fn check_screen_recording() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

// ---------------------------------------------------------------------------
// Accessibility permission
// ---------------------------------------------------------------------------

use core_foundation::base::TCFType;
use core_foundation::string::CFString;

extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
}

/// Check whether Accessibility permission is granted (without prompting).
pub fn check_accessibility() -> bool {
    unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
}

/// Check Accessibility permission and show the system prompt if not yet granted.
pub fn request_accessibility() -> bool {
    unsafe {
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let keys = [key.as_concrete_TypeRef() as *const std::ffi::c_void];
        let values = [core_foundation_sys::number::kCFBooleanTrue as *const std::ffi::c_void];
        let options = core_foundation_sys::dictionary::CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &core_foundation_sys::dictionary::kCFTypeDictionaryKeyCallBacks,
            &core_foundation_sys::dictionary::kCFTypeDictionaryValueCallBacks,
        );
        let result = AXIsProcessTrustedWithOptions(options as *const _);
        core_foundation_sys::base::CFRelease(options as *const _);
        result
    }
}

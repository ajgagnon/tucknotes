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

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::models::meeting_detection::{
    CallSignal, DetectionPhase, MeetingAppProfile, MeetingDetectedEvent, MeetingDetectorState,
};

// ---------------------------------------------------------------------------
// Thresholds
// ---------------------------------------------------------------------------

const SCAN_INTERVAL: Duration = Duration::from_secs(5);
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(15);
const ENDING_TIMEOUT: Duration = Duration::from_secs(30);
const AX_MAX_DEPTH: usize = 10;
const AX_MAX_ELEMENTS: usize = 500;

// ---------------------------------------------------------------------------
// App profiles
// ---------------------------------------------------------------------------

fn default_profiles() -> Vec<MeetingAppProfile> {
    vec![
        MeetingAppProfile {
            app_name: "Zoom",
            bundle_ids: &["us.zoom.xos"],
            url_patterns: &[],
            call_signals: vec![CallSignal::RoleWithName {
                role: "AXButton",
                name_contains: "Leave",
            }],
        },
        MeetingAppProfile {
            app_name: "Microsoft Teams",
            bundle_ids: &["com.microsoft.teams2", "com.microsoft.teams"],
            url_patterns: &["teams.microsoft.com"],
            call_signals: vec![
                CallSignal::RoleWithName {
                    role: "AXButton",
                    name_contains: "Leave",
                },
                CallSignal::RoleWithName {
                    role: "AXButton",
                    name_contains: "Hang up",
                },
            ],
        },
        MeetingAppProfile {
            app_name: "Google Meet",
            bundle_ids: &[],
            url_patterns: &["meet.google.com"],
            call_signals: vec![
                CallSignal::RoleWithName {
                    role: "AXButton",
                    name_contains: "Leave call",
                },
                CallSignal::AutomationIdContains("call-leave"),
            ],
        },
        MeetingAppProfile {
            app_name: "Slack",
            bundle_ids: &["com.tinyspeck.slackmacgap"],
            url_patterns: &[],
            call_signals: vec![CallSignal::RoleWithName {
                role: "AXButton",
                name_contains: "Leave",
            }],
        },
    ]
}

// ---------------------------------------------------------------------------
// AX FFI bindings (ApplicationServices.framework)
// ---------------------------------------------------------------------------

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *mut std::ffi::c_void;
    fn AXUIElementCopyAttributeValue(
        element: *const std::ffi::c_void,
        attribute: *const std::ffi::c_void,
        value: *mut *mut std::ffi::c_void,
    ) -> i32;
}

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::CFString;
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex};

/// Read a string attribute from an AXUIElement. Returns `None` on failure.
unsafe fn ax_string_attr(element: *const std::ffi::c_void, attr: &str) -> Option<String> {
    let cf_attr = CFString::new(attr);
    let mut value: *mut std::ffi::c_void = std::ptr::null_mut();
    let err = AXUIElementCopyAttributeValue(
        element,
        cf_attr.as_concrete_TypeRef() as *const _,
        &mut value,
    );
    if err != 0 || value.is_null() {
        return None;
    }
    // Check that it's a CFString
    let cf_type_id = core_foundation::base::CFGetTypeID(value as CFTypeRef);
    if cf_type_id != core_foundation::string::CFStringGetTypeID() {
        CFRelease(value as CFTypeRef);
        return None;
    }
    let cf_str = CFString::wrap_under_get_rule(value as core_foundation::string::CFStringRef);
    Some(cf_str.to_string())
}

/// Read an array attribute from an AXUIElement. Returns empty vec on failure.
unsafe fn ax_array_attr(element: *const std::ffi::c_void, attr: &str) -> Vec<*mut std::ffi::c_void> {
    let cf_attr = CFString::new(attr);
    let mut value: *mut std::ffi::c_void = std::ptr::null_mut();
    let err = AXUIElementCopyAttributeValue(
        element,
        cf_attr.as_concrete_TypeRef() as *const _,
        &mut value,
    );
    if err != 0 || value.is_null() {
        return Vec::new();
    }
    let cf_type_id = core_foundation::base::CFGetTypeID(value as CFTypeRef);
    let array_type_id = core_foundation_sys::array::CFArrayGetTypeID();
    if cf_type_id != array_type_id {
        CFRelease(value as CFTypeRef);
        return Vec::new();
    }
    let cf_array = value as core_foundation_sys::array::CFArrayRef;
    let count = CFArrayGetCount(cf_array);
    let mut result = Vec::with_capacity(count as usize);
    for i in 0..count {
        let item = CFArrayGetValueAtIndex(cf_array, i) as *mut std::ffi::c_void;
        result.push(item);
    }
    // Don't release the array — the AX elements inside it are not retained by us,
    // and the array itself was created by the AX call (we got ownership).
    // We need the elements to remain valid while we walk them.
    // Leak the array intentionally; elements are short-lived within the scan.
    result
}

// ---------------------------------------------------------------------------
// AX tree walking — check for call signals in a process
// ---------------------------------------------------------------------------

fn check_call_signals(pid: i32, signals: &[CallSignal]) -> bool {
    std::panic::catch_unwind(|| unsafe { do_check_call_signals(pid, signals) }).unwrap_or(false)
}

unsafe fn do_check_call_signals(pid: i32, signals: &[CallSignal]) -> bool {
    let app_element = AXUIElementCreateApplication(pid);
    if app_element.is_null() {
        return false;
    }

    let windows = ax_array_attr(app_element, "AXWindows");
    let visited = AtomicUsize::new(0);

    let mut found = false;
    for window in &windows {
        if walk_ax_tree(*window, signals, 0, &visited) {
            found = true;
            break;
        }
    }

    CFRelease(app_element as CFTypeRef);
    found
}

unsafe fn walk_ax_tree(
    element: *const std::ffi::c_void,
    signals: &[CallSignal],
    depth: usize,
    visited: &AtomicUsize,
) -> bool {
    if depth > AX_MAX_DEPTH {
        return false;
    }
    if visited.fetch_add(1, Ordering::Relaxed) >= AX_MAX_ELEMENTS {
        return false;
    }

    // Check this element against all signals
    if element_matches_any_signal(element, signals) {
        return true;
    }

    // Skip content-heavy subtrees
    let role = ax_string_attr(element, "AXRole");
    if let Some(ref r) = role {
        match r.as_str() {
            "AXTextArea" | "AXTable" | "AXOutline" | "AXList" => return false,
            _ => {}
        }
    }

    // Recurse into children
    let children = ax_array_attr(element, "AXChildren");
    for child in &children {
        if walk_ax_tree(*child, signals, depth + 1, visited) {
            return true;
        }
    }

    false
}

unsafe fn element_matches_any_signal(
    element: *const std::ffi::c_void,
    signals: &[CallSignal],
) -> bool {
    let role = ax_string_attr(element, "AXRole");
    let title = ax_string_attr(element, "AXTitle");
    let description = ax_string_attr(element, "AXDescription");
    let identifier = ax_string_attr(element, "AXIdentifier");

    for signal in signals {
        match signal {
            CallSignal::AutomationIdContains(substr) => {
                if let Some(ref id) = identifier {
                    if id.to_lowercase().contains(&substr.to_lowercase()) {
                        return true;
                    }
                }
            }
            CallSignal::RoleWithName {
                role: expected_role,
                name_contains,
            } => {
                let role_matches = role
                    .as_deref()
                    .map(|r| r == *expected_role)
                    .unwrap_or(false);
                if !role_matches {
                    continue;
                }
                let needle = name_contains.to_lowercase();
                let name_matches = title
                    .as_deref()
                    .map(|t| t.to_lowercase().contains(&needle))
                    .unwrap_or(false)
                    || description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&needle))
                        .unwrap_or(false);
                if name_matches {
                    return true;
                }
            }
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Process scanning via NSWorkspace
// ---------------------------------------------------------------------------

struct MatchedProcess {
    profile_index: usize,
    pid: i32,
}

fn find_meeting_processes(profiles: &[MeetingAppProfile]) -> Vec<MatchedProcess> {
    use objc2_app_kit::NSWorkspace;

    let mut matches = Vec::new();

    let workspace = NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();

    // Known browser bundle IDs
    let browser_ids: &[&str] = &[
        "com.google.Chrome",
        "com.apple.Safari",
        "company.thebrowser.Browser",
        "org.mozilla.firefox",
        "com.microsoft.edgemac",
        "com.brave.Browser",
        "com.operasoftware.Opera",
    ];

    for app in &apps {
        let bundle_id = app.bundleIdentifier();
        let bundle_str = bundle_id.as_ref().map(|b| b.to_string());

        if let Some(ref bid) = bundle_str {
            let bid_lower = bid.to_lowercase();

            // Check native app profiles
            for (i, profile) in profiles.iter().enumerate() {
                for expected_bid in profile.bundle_ids {
                    if bid_lower == expected_bid.to_lowercase() {
                        let pid = app.processIdentifier();
                        matches.push(MatchedProcess {
                            profile_index: i,
                            pid: pid as i32,
                        });
                    }
                }
            }

            // Check browser-based profiles
            let is_browser = browser_ids.iter().any(|b| bid_lower == b.to_lowercase());
            if is_browser {
                let pid = app.processIdentifier() as i32;
                // Check window titles for URL patterns
                if let Some(profile_idx) = check_browser_for_meeting_urls(pid, profiles) {
                    matches.push(MatchedProcess {
                        profile_index: profile_idx,
                        pid,
                    });
                }
            }
        }
    }

    matches
}

/// Check a browser's AX windows for titles/URLs matching meeting URL patterns.
fn check_browser_for_meeting_urls(pid: i32, profiles: &[MeetingAppProfile]) -> Option<usize> {
    std::panic::catch_unwind(|| unsafe { do_check_browser_urls(pid, profiles) })
        .ok()
        .flatten()
}

unsafe fn do_check_browser_urls(pid: i32, profiles: &[MeetingAppProfile]) -> Option<usize> {
    let app_element = AXUIElementCreateApplication(pid);
    if app_element.is_null() {
        return None;
    }

    let windows = ax_array_attr(app_element, "AXWindows");
    let mut result = None;

    for window in &windows {
        let title = ax_string_attr(*window, "AXTitle");
        let doc = ax_string_attr(*window, "AXDocument");

        for (i, profile) in profiles.iter().enumerate() {
            for pattern in profile.url_patterns {
                let pattern_lower = pattern.to_lowercase();
                let title_match = title
                    .as_deref()
                    .map(|t| t.to_lowercase().contains(&pattern_lower))
                    .unwrap_or(false);
                let doc_match = doc
                    .as_deref()
                    .map(|d| d.to_lowercase().contains(&pattern_lower))
                    .unwrap_or(false);
                if title_match || doc_match {
                    result = Some(i);
                    break;
                }
            }
            if result.is_some() {
                break;
            }
        }
        if result.is_some() {
            break;
        }
    }

    CFRelease(app_element as CFTypeRef);
    result
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

struct DetectorStateMachine {
    phase: DetectionPhase,
    confirming_since: Option<Instant>,
    ending_since: Option<Instant>,
    detected_app: Option<String>,
}

impl DetectorStateMachine {
    fn new() -> Self {
        Self {
            phase: DetectionPhase::Idle,
            confirming_since: None,
            ending_since: None,
            detected_app: None,
        }
    }

    /// Advance the state machine given whether a meeting signal was found.
    /// Returns `Some(phase)` if a phase transition occurred.
    fn tick(&mut self, signal_found: bool, app_name: Option<&str>) -> Option<DetectionPhase> {
        match self.phase {
            DetectionPhase::Idle => {
                if signal_found {
                    self.phase = DetectionPhase::Confirming;
                    self.confirming_since = Some(Instant::now());
                    self.detected_app = app_name.map(String::from);
                    None // no external event yet
                } else {
                    None
                }
            }
            DetectionPhase::Confirming => {
                if !signal_found {
                    // Lost signal during confirmation — go back to idle
                    self.phase = DetectionPhase::Idle;
                    self.confirming_since = None;
                    self.detected_app = None;
                    None
                } else if self
                    .confirming_since
                    .map(|t| t.elapsed() >= CONFIRM_TIMEOUT)
                    .unwrap_or(false)
                {
                    // Confirmed!
                    self.phase = DetectionPhase::Active;
                    self.confirming_since = None;
                    Some(DetectionPhase::Active)
                } else {
                    None // still confirming
                }
            }
            DetectionPhase::Active => {
                if !signal_found {
                    self.phase = DetectionPhase::Ending;
                    self.ending_since = Some(Instant::now());
                    None
                } else {
                    None
                }
            }
            DetectionPhase::Ending => {
                if signal_found {
                    // Signal came back — return to active
                    self.phase = DetectionPhase::Active;
                    self.ending_since = None;
                    None
                } else if self
                    .ending_since
                    .map(|t| t.elapsed() >= ENDING_TIMEOUT)
                    .unwrap_or(false)
                {
                    // Grace period expired — meeting ended
                    self.phase = DetectionPhase::Idle;
                    self.ending_since = None;
                    let prev_app = self.detected_app.take();
                    // Return Idle to signal meeting ended (with app name still available)
                    self.detected_app = prev_app; // keep for the event, cleared next tick
                    Some(DetectionPhase::Idle)
                } else {
                    None
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Overlay window helpers
// ---------------------------------------------------------------------------

fn show_overlay(app: &tauri::AppHandle, app_name: &str) {
    // Don't create a second overlay
    if app.get_webview_window("meeting-overlay").is_some() {
        return;
    }

    let screen_w = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let scale = m.scale_factor();
            m.size().width as f64 / scale
        })
        .unwrap_or(1440.0);

    let url = WebviewUrl::App(format!("overlay.html?app={}", urlencoding(app_name)).into());

    let builder = WebviewWindowBuilder::new(app, "meeting-overlay", url)
        .title("")
        .inner_size(320.0, 80.0)
        .position(screen_w - 340.0, 20.0)
        .always_on_top(true)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .skip_taskbar(true)
        .focused(false);

    if let Err(e) = builder.build() {
        eprintln!("[meeting_detector] failed to create overlay window: {e}");
    }
}

fn hide_overlay(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("meeting-overlay") {
        let _ = w.close();
    }
}

fn urlencoding(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
}

// ---------------------------------------------------------------------------
// Background detection loop
// ---------------------------------------------------------------------------

pub fn start_detection_loop(app: tauri::AppHandle, recording_active: Arc<AtomicBool>) {
    let state: tauri::State<'_, MeetingDetectorState> = app.state();
    let cancel = tokio_util::sync::CancellationToken::new();
    {
        let mut guard = state.cancel_token.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(cancel.clone());
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let profiles = default_profiles();
        let mut sm = DetectorStateMachine::new();
        let mut interval = tokio::time::interval(SCAN_INTERVAL);
        interval.tick().await; // skip immediate first tick

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Skip scanning while recording is active
                    if recording_active.load(Ordering::Relaxed) {
                        continue;
                    }

                    // Check accessibility permission
                    if !crate::services::permissions::check_accessibility() {
                        continue;
                    }

                    // Scan for meeting processes (blocking AX work)
                    let profiles_clone = profiles.clone();
                    let scan_result = tokio::task::spawn_blocking(move || {
                        let matched = find_meeting_processes(&profiles_clone);
                        for m in &matched {
                            let profile = &profiles_clone[m.profile_index];
                            if check_call_signals(m.pid, &profile.call_signals) {
                                return Some(profile.app_name.to_string());
                            }
                        }
                        None
                    }).await;

                    let (signal_found, app_name) = match scan_result {
                        Ok(Some(name)) => (true, Some(name)),
                        _ => (false, None),
                    };

                    // Advance state machine
                    if let Some(new_phase) = sm.tick(signal_found, app_name.as_deref()) {
                        let event = MeetingDetectedEvent {
                            phase: new_phase,
                            app_name: sm.detected_app.clone(),
                        };

                        match new_phase {
                            DetectionPhase::Active => {
                                let _ = app.emit("meeting-detected", &event);
                                if let Some(ref name) = sm.detected_app {
                                    show_overlay(&app, name);
                                }
                            }
                            DetectionPhase::Idle => {
                                let _ = app.emit("meeting-ended", &event);
                                hide_overlay(&app);
                                sm.detected_app = None;
                            }
                            _ => {}
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    break;
                }
            }
        }
    });
}

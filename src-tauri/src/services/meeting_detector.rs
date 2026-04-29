use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use screencapturekit::shareable_content::SCShareableContent;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::models::meeting_detection::{
    DetectionPhase, MeetingAppProfile, MeetingConfidence, MeetingDetectedEvent,
    MeetingDetectorState, MeetingPattern,
};

// ---------------------------------------------------------------------------
// Thresholds
// ---------------------------------------------------------------------------

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// Time the meeting must remain undetected after entering `Ending` before we
/// declare it ended. Window-presence is a clean signal so this can be short.
const ENDING_TIMEOUT: Duration = Duration::from_secs(4);
/// Consecutive missed scans required before transitioning `Active → Ending`.
/// Guards against a single transient `SCShareableContent::get()` failure
/// false-firing the end-of-meeting path.
const REQUIRED_MISSES_BEFORE_ENDING: u32 = 2;

// ---------------------------------------------------------------------------
// Per-app profiles (mirrors RecapAI/Recap's pattern lists)
// ---------------------------------------------------------------------------

const ZOOM_PATTERNS: &[MeetingPattern] = &[
    MeetingPattern {
        keyword: "zoom meeting",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &[],
    },
    MeetingPattern {
        keyword: "zoom webinar",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &[],
    },
];

const TEAMS_PATTERNS: &[MeetingPattern] = &[
    MeetingPattern {
        keyword: "microsoft teams meeting",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &[],
    },
    MeetingPattern {
        keyword: "teams meeting",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &[],
    },
    MeetingPattern {
        keyword: "meeting in \"",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &[],
    },
    MeetingPattern {
        keyword: "call with",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &[],
    },
    MeetingPattern {
        keyword: "| Microsoft Teams",
        confidence: MeetingConfidence::High,
        case_sensitive: true,
        exclude_patterns: &["chat", "activity"],
    },
];

// Meet's in-call tab title is "Meet - <code>" (en/em dash variants exist by
// locale). The /landing and /new pages titled "Google Meet" — combined with
// Chrome's " - Google Chrome" suffix on window titles — would also match a
// bare "meet - " pattern, so we exclude any title containing "google meet"
// to avoid pinning the detector in `Active` when the user is on a lobby page.
const MEET_PATTERNS: &[MeetingPattern] = &[
    MeetingPattern {
        keyword: "meet - ",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &["google meet"],
    },
    MeetingPattern {
        keyword: "meet \u{2013} ",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &["google meet"],
    },
    MeetingPattern {
        keyword: "meet \u{2014} ",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &["google meet"],
    },
];

const SLACK_PATTERNS: &[MeetingPattern] = &[
    MeetingPattern {
        keyword: "huddle",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &[],
    },
    MeetingPattern {
        keyword: "slack call",
        confidence: MeetingConfidence::High,
        case_sensitive: false,
        exclude_patterns: &[],
    },
];

fn default_profiles() -> Vec<MeetingAppProfile> {
    vec![
        MeetingAppProfile {
            app_name: "Zoom",
            bundle_ids: &["us.zoom.xos"],
            patterns: ZOOM_PATTERNS,
        },
        MeetingAppProfile {
            app_name: "Microsoft Teams",
            bundle_ids: &["com.microsoft.teams2", "com.microsoft.teams"],
            patterns: TEAMS_PATTERNS,
        },
        MeetingAppProfile {
            app_name: "Google Meet",
            // Browser-hosted — owning app is whatever browser is running.
            bundle_ids: &[],
            patterns: MEET_PATTERNS,
        },
        MeetingAppProfile {
            app_name: "Slack",
            bundle_ids: &["com.tinyspeck.slackmacgap"],
            patterns: SLACK_PATTERNS,
        },
    ]
}

// ---------------------------------------------------------------------------
// Pattern matching
// ---------------------------------------------------------------------------

/// Returns the highest-confidence match for `title` among `patterns`, or `None`.
fn find_best_match(
    title: &str,
    patterns: &[MeetingPattern],
) -> Option<MeetingConfidence> {
    let title_lower = title.to_lowercase();

    let mut best: Option<MeetingConfidence> = None;
    for pattern in patterns {
        let haystack = if pattern.case_sensitive { title } else { &title_lower };
        let needle_owned;
        let needle: &str = if pattern.case_sensitive {
            pattern.keyword
        } else {
            needle_owned = pattern.keyword.to_lowercase();
            &needle_owned
        };
        if !haystack.contains(needle) {
            continue;
        }
        let excluded = pattern
            .exclude_patterns
            .iter()
            .any(|ex| title_lower.contains(&ex.to_lowercase()));
        if excluded {
            continue;
        }
        match best {
            Some(b) if b >= pattern.confidence => {}
            _ => best = Some(pattern.confidence),
        }
        if best == Some(MeetingConfidence::High) {
            break;
        }
    }
    best
}

/// Scan all on-screen windows and return the first matched profile's app name.
fn scan_windows_for_meeting(profiles: &[MeetingAppProfile]) -> Option<String> {
    let content = SCShareableContent::get().ok()?;
    let windows = content.windows();

    for window in windows {
        if !window.is_on_screen() {
            continue;
        }
        let Some(title) = window.title() else { continue };
        if title.is_empty() {
            continue;
        }
        let owning_bundle = window
            .owning_application()
            .map(|app| app.bundle_identifier());

        for profile in profiles {
            if !profile.bundle_ids.is_empty() {
                let Some(ref bid) = owning_bundle else { continue };
                if !profile.bundle_ids.iter().any(|b| b.eq_ignore_ascii_case(bid)) {
                    continue;
                }
            }
            if find_best_match(&title, profile.patterns).is_some() {
                return Some(profile.app_name.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

struct DetectorStateMachine {
    phase: DetectionPhase,
    ending_since: Option<Instant>,
    detected_app: Option<String>,
    consecutive_misses: u32,
}

impl DetectorStateMachine {
    fn new() -> Self {
        Self {
            phase: DetectionPhase::Idle,
            ending_since: None,
            detected_app: None,
            consecutive_misses: 0,
        }
    }

    fn tick(&mut self, signal_found: bool, app_name: Option<&str>) -> Option<DetectionPhase> {
        match self.phase {
            DetectionPhase::Idle => {
                if signal_found {
                    self.phase = DetectionPhase::Active;
                    self.detected_app = app_name.map(String::from);
                    self.consecutive_misses = 0;
                    Some(DetectionPhase::Active)
                } else {
                    None
                }
            }
            DetectionPhase::Active => {
                if signal_found {
                    self.consecutive_misses = 0;
                    None
                } else {
                    self.consecutive_misses += 1;
                    if self.consecutive_misses >= REQUIRED_MISSES_BEFORE_ENDING {
                        self.phase = DetectionPhase::Ending;
                        self.ending_since = Some(Instant::now());
                    }
                    None
                }
            }
            DetectionPhase::Ending => {
                if signal_found {
                    self.phase = DetectionPhase::Active;
                    self.ending_since = None;
                    self.consecutive_misses = 0;
                    None
                } else if self
                    .ending_since
                    .map(|t| t.elapsed() >= ENDING_TIMEOUT)
                    .unwrap_or(false)
                {
                    self.phase = DetectionPhase::Idle;
                    self.ending_since = None;
                    self.consecutive_misses = 0;
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

fn primary_screen_width(app: &tauri::AppHandle) -> f64 {
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.size().width as f64 / m.scale_factor())
        .unwrap_or(1440.0)
}

fn build_overlay_window(
    app: &tauri::AppHandle,
    label: &str,
    url: WebviewUrl,
    width: f64,
    right_margin: f64,
) {
    let screen_w = primary_screen_width(app);
    let builder = WebviewWindowBuilder::new(app, label, url)
        .title("")
        .inner_size(width, 80.0)
        .position(screen_w - (width + right_margin), 20.0)
        .always_on_top(true)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .focused(false)
        .accept_first_mouse(true);

    if let Err(e) = builder.build() {
        eprintln!("[meeting_detector] failed to create overlay window {label}: {e}");
    }
}

fn close_overlay_window(app: &tauri::AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.close();
    }
}

pub fn show_overlay(app: &tauri::AppHandle, app_name: &str) {
    if app.get_webview_window("meeting-overlay").is_some() {
        return;
    }
    let url = WebviewUrl::App(format!("overlay.html?app={}", urlencoding(app_name)).into());
    build_overlay_window(app, "meeting-overlay", url, 320.0, 20.0);
}

fn hide_overlay(app: &tauri::AppHandle) {
    close_overlay_window(app, "meeting-overlay");
}

pub fn show_auto_stop_overlay(app: &tauri::AppHandle, app_name: Option<&str>) {
    if app.get_webview_window("auto-stop-overlay").is_some() {
        return;
    }
    let app_param = app_name.map(urlencoding).unwrap_or_default();
    let url = WebviewUrl::App(format!("overlay.html?mode=autostop&app={app_param}").into());
    build_overlay_window(app, "auto-stop-overlay", url, 360.0, 20.0);
}

pub fn hide_auto_stop_overlay(app: &tauri::AppHandle) {
    close_overlay_window(app, "auto-stop-overlay");
}

/// Diagnostic helper: returns one line per on-screen window with title +
/// owning bundle id. Used by the `debug_dump_windows` Tauri command so a
/// user can inspect what the detector sees after ending a meeting.
pub fn dump_windows() -> Vec<String> {
    let Some(content) = SCShareableContent::get().ok() else {
        return vec!["[meeting_detector] SCShareableContent::get() failed".into()];
    };
    content
        .windows()
        .into_iter()
        .filter(|w| w.is_on_screen())
        .filter_map(|w| {
            let title = w.title().filter(|t| !t.is_empty())?;
            let bundle = w
                .owning_application()
                .map(|app| app.bundle_identifier())
                .unwrap_or_default();
            Some(format!("{bundle}: {title}"))
        })
        .collect()
}

fn urlencoding(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
}

// ---------------------------------------------------------------------------
// Background detection loop
// ---------------------------------------------------------------------------

pub fn start_detection_loop(
    app: tauri::AppHandle,
    recording_active: Arc<AtomicBool>,
    current_app: Arc<Mutex<Option<String>>>,
) {
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
                    if !crate::services::permissions::check_screen_recording() {
                        continue;
                    }

                    let profiles_clone = profiles.clone();
                    let scan_result = tokio::task::spawn_blocking(move || {
                        scan_windows_for_meeting(&profiles_clone)
                    }).await;

                    let (signal_found, app_name) = match scan_result {
                        Ok(Some(name)) => (true, Some(name)),
                        _ => (false, None),
                    };

                    if let Some(new_phase) = sm.tick(signal_found, app_name.as_deref()) {
                        eprintln!(
                            "[meeting_detector] phase transition → {:?} (app: {:?})",
                            new_phase, sm.detected_app
                        );
                        let event = MeetingDetectedEvent {
                            phase: new_phase,
                            app_name: sm.detected_app.clone(),
                        };

                        match new_phase {
                            DetectionPhase::Active => {
                                if let Ok(mut guard) = current_app.lock() {
                                    *guard = sm.detected_app.clone();
                                }
                                let _ = app.emit("meeting-detected", &event);
                                if !recording_active.load(Ordering::Relaxed) {
                                    if let Some(ref name) = sm.detected_app {
                                        show_overlay(&app, name);
                                    }
                                }
                            }
                            DetectionPhase::Idle => {
                                if let Ok(mut guard) = current_app.lock() {
                                    *guard = None;
                                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_zoom_meeting_window_title() {
        // Real-world Zoom title from a host's screenshot.
        assert!(find_best_match("Andre Gagnon's Zoom Meeting", ZOOM_PATTERNS).is_some());
    }

    #[test]
    fn matches_zoom_when_participant_too() {
        assert!(find_best_match("Zoom Meeting", ZOOM_PATTERNS).is_some());
    }

    #[test]
    fn rejects_text_editor_meeting_notes() {
        // Profiles only match high-specificity strings ("zoom meeting", not bare
        // "meeting"), so a TextEdit doc called "Meeting Notes" never matches.
        assert!(find_best_match("Meeting Notes.txt", ZOOM_PATTERNS).is_none());
    }

    #[test]
    fn matches_google_meet_in_call_title() {
        assert!(find_best_match(
            "Meet - abc-defg-hij - Google Chrome",
            MEET_PATTERNS
        )
        .is_some());
        // En dash variant (some locales).
        assert!(find_best_match("Meet \u{2013} abc-defg-hij", MEET_PATTERNS).is_some());
        // Em dash variant.
        assert!(find_best_match("Meet \u{2014} abc-defg-hij", MEET_PATTERNS).is_some());
    }

    #[test]
    fn rejects_google_meet_landing_page() {
        // The /landing page's title is just "Google Meet" — must NOT match,
        // otherwise the detector stays Active forever and the auto-stop
        // overlay never appears when a real meeting ends.
        assert!(find_best_match("Google Meet", MEET_PATTERNS).is_none());
        // The /new "create meeting" page also lands here.
        assert!(find_best_match("Google Meet - Google Chrome", MEET_PATTERNS).is_none());
    }

    #[test]
    fn teams_chat_window_excluded() {
        // "| Microsoft Teams" with "chat" in title should be excluded.
        assert!(find_best_match("Some Chat | Microsoft Teams", TEAMS_PATTERNS).is_none());
    }

    #[test]
    fn slack_huddle_matches() {
        assert!(find_best_match("Huddle in #engineering", SLACK_PATTERNS).is_some());
    }

    #[test]
    fn empty_title_no_match() {
        assert!(find_best_match("", ZOOM_PATTERNS).is_none());
    }

    #[test]
    fn state_machine_requires_consecutive_misses() {
        let mut sm = DetectorStateMachine::new();
        assert_eq!(sm.tick(true, Some("Zoom")), Some(DetectionPhase::Active));
        // First miss: no transition (consecutive_misses = 1).
        assert_eq!(sm.tick(false, None), None);
        assert_eq!(sm.phase, DetectionPhase::Active);
        // Second miss: transitions to Ending (consecutive_misses = 2).
        assert_eq!(sm.tick(false, None), None);
        assert_eq!(sm.phase, DetectionPhase::Ending);
    }

    #[test]
    fn state_machine_resets_misses_on_signal() {
        let mut sm = DetectorStateMachine::new();
        sm.tick(true, Some("Zoom"));
        sm.tick(false, None); // 1 miss
        sm.tick(true, Some("Zoom")); // resets
        sm.tick(false, None); // 1 miss again
        assert_eq!(sm.phase, DetectionPhase::Active);
    }
}

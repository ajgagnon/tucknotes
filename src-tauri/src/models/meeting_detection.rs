use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionPhase {
    Idle,
    Active,
    Ending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeetingConfidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy)]
pub struct MeetingPattern {
    pub keyword: &'static str,
    pub confidence: MeetingConfidence,
    pub case_sensitive: bool,
    /// If any of these substrings appear in the title (case-insensitive),
    /// the pattern is rejected. Used e.g. for Teams to skip chat windows.
    pub exclude_patterns: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub struct MeetingAppProfile {
    pub app_name: &'static str,
    /// When non-empty, only consider windows owned by these bundle ids.
    /// When empty, the profile matches any window (used for browser-based
    /// meetings where the owning app is the browser, not a vendor).
    pub bundle_ids: &'static [&'static str],
    pub patterns: &'static [MeetingPattern],
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingDetectedEvent {
    pub phase: DetectionPhase,
    pub app_name: Option<String>,
}

pub struct MeetingDetectorState {
    pub cancel_token: Mutex<Option<CancellationToken>>,
    /// Set to `true` while a recording is in progress so the detector
    /// suppresses the "you should record this" overlay.
    pub recording_active: Arc<AtomicBool>,
    /// App name of the currently detected meeting, or `None` when the detector
    /// is `Idle`/`Ending`. Lets late-arriving readers (e.g. a recording that
    /// starts after a meeting was already detected) recover the current state
    /// without waiting for the next phase transition.
    pub current_app: Arc<Mutex<Option<String>>>,
}

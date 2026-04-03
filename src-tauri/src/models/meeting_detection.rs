use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Detection phase (state machine)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionPhase {
    Idle,
    Active,
    Ending,
}

// ---------------------------------------------------------------------------
// Call signals — what to look for in the AX tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CallSignal {
    /// AX element whose AXIdentifier contains this substring (case-insensitive).
    AutomationIdContains(&'static str),
    /// AX element with a specific AXRole whose AXTitle or AXDescription
    /// contains the given substring (case-insensitive).
    RoleWithName {
        role: &'static str,
        name_contains: &'static str,
    },
}

// ---------------------------------------------------------------------------
// Per-app detection profile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MeetingAppProfile {
    pub app_name: &'static str,
    /// macOS bundle identifiers (e.g. "us.zoom.xos").
    pub bundle_ids: &'static [&'static str],
    /// URL patterns for browser-based meetings (e.g. "meet.google.com").
    pub url_patterns: &'static [&'static str],
    /// Signals that indicate an active call (leave/hangup buttons only).
    pub call_signals: Vec<CallSignal>,
}

// ---------------------------------------------------------------------------
// Events emitted to the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MeetingDetectedEvent {
    pub phase: DetectionPhase,
    pub app_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Managed state
// ---------------------------------------------------------------------------

pub struct MeetingDetectorState {
    pub cancel_token: Mutex<Option<CancellationToken>>,
    /// Set to `true` while a recording is in progress so the detector
    /// suppresses overlay creation.
    pub recording_active: Arc<AtomicBool>,
}

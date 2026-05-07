use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Trial duration from first launch.
pub const TRIAL_DAYS: u64 = 14;

/// How long a previously-validated license remains usable while the
/// validation server is unreachable.
pub const OFFLINE_GRACE_DAYS: u64 = 7;

const DAY_SECS: u64 = 24 * 60 * 60;
pub const TRIAL_SECS: u64 = TRIAL_DAYS * DAY_SECS;
pub const OFFLINE_GRACE_SECS: u64 = OFFLINE_GRACE_DAYS * DAY_SECS;

/// Snapshot of licensing state surfaced to the frontend. Tagged enum so
/// it round-trips through serde with a discriminator.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind")]
pub enum LicenseStatus {
    Trial { days_remaining: u32 },
    TrialExpired,
    Licensed {
        last_validated_at: i64,
        expires_grace_at: i64,
    },
    LicenseInvalid { reason: String },
}

impl LicenseStatus {
    /// Whether usage of paywalled features (recording, summarization) is allowed.
    pub fn allows_paid_features(&self) -> bool {
        matches!(
            self,
            LicenseStatus::Trial { .. } | LicenseStatus::Licensed { .. }
        )
    }
}

/// Persisted record of an activated license.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LicenseRecord {
    pub key: String,
    pub activation_id: String,
    /// Unix seconds of the last successful validation against Polar.
    pub last_validated_at: i64,
}

/// On-disk schema for `license.json`. New fields must default-deserialize
/// so old files keep loading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LicenseStorage {
    /// Unix seconds when first_launch was recorded.
    pub first_launch_at: i64,
    #[serde(default)]
    pub license: Option<LicenseRecord>,
}

impl LicenseStorage {
    pub fn fresh(now_unix_secs: i64) -> Self {
        Self {
            first_launch_at: now_unix_secs,
            license: None,
        }
    }
}

/// Tauri-managed in-memory state. Holds the loaded storage so we don't
/// re-read the file on every status check.
pub struct LicensingState {
    pub storage: Mutex<LicenseStorage>,
}

impl LicensingState {
    pub fn new(storage: LicenseStorage) -> Self {
        Self {
            storage: Mutex::new(storage),
        }
    }
}

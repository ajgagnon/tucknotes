use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::errors::AppError;
use crate::models::licensing::{
    LicenseRecord, LicenseStatus, LicenseStorage, OFFLINE_GRACE_SECS, TRIAL_SECS,
};

/// Polar organization id (public-safe; users copy a key from Polar's
/// checkout for their own license, the org id only routes the API call).
/// Compiled in from the `POLAR_ORGANIZATION_ID` env var at build time;
/// when unset, activation fails fast with a clear error rather than
/// breaking unrelated builds.
pub const POLAR_ORGANIZATION_ID: &str = match option_env!("POLAR_ORGANIZATION_ID") {
    Some(s) => s,
    None => "",
};

const POLAR_API_BASE: &str = "https://api.polar.sh";

fn license_file_path(base_dir: &Path) -> PathBuf {
    base_dir.join("license.json")
}

pub fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Read `license.json` from disk, or return a freshly-stamped record if
/// the file doesn't exist yet (first launch).
pub fn load_storage_from(base_dir: &Path) -> Result<LicenseStorage, AppError> {
    let path = license_file_path(base_dir);
    if !path.exists() {
        return Ok(LicenseStorage::fresh(now_unix_secs()));
    }
    let contents = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&contents)?)
}

/// Atomic write so a crash mid-write can't corrupt the license file.
pub fn save_storage_to(base_dir: &Path, storage: &LicenseStorage) -> Result<(), AppError> {
    if !base_dir.exists() {
        std::fs::create_dir_all(base_dir)?;
    }
    let path = license_file_path(base_dir);
    let tmp_path = base_dir.join("license.json.tmp");
    let json = serde_json::to_string_pretty(storage)?;
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Initialize storage on app startup: load the file (creating + persisting
/// it on first launch so the trial clock starts now, not the next time we
/// touch disk).
pub fn init_storage(app: &AppHandle) -> Result<LicenseStorage, AppError> {
    let base_dir = resolve_data_dir(app)?;
    let path = license_file_path(&base_dir);
    if path.exists() {
        load_storage_from(&base_dir)
    } else {
        let fresh = LicenseStorage::fresh(now_unix_secs());
        save_storage_to(&base_dir, &fresh)?;
        Ok(fresh)
    }
}

fn resolve_data_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::IoError(e.to_string()))
}

/// Compute the public status from a storage snapshot at a given instant.
/// Pure function so it's trivially unit-testable.
pub fn compute_status(storage: &LicenseStorage, now_unix_secs: i64) -> LicenseStatus {
    if let Some(license) = &storage.license {
        let expires_grace_at = license
            .last_validated_at
            .saturating_add(OFFLINE_GRACE_SECS as i64);
        if now_unix_secs <= expires_grace_at {
            return LicenseStatus::Licensed {
                last_validated_at: license.last_validated_at,
                expires_grace_at,
            };
        }
        // Grace expired — treat as needing re-validation. Caller can re-check
        // online via revalidate(); until then, behave as invalid so paid
        // features lock until the user reconnects.
        return LicenseStatus::LicenseInvalid {
            reason: "Offline grace period expired. Reconnect to re-validate license.".into(),
        };
    }

    let elapsed = now_unix_secs.saturating_sub(storage.first_launch_at);
    if elapsed < 0 || (elapsed as u64) >= TRIAL_SECS {
        LicenseStatus::TrialExpired
    } else {
        let remaining_secs = TRIAL_SECS.saturating_sub(elapsed as u64);
        // Round up so "less than a day left" still reads as "1 day".
        let days_remaining = (remaining_secs + 86_399) / 86_400;
        LicenseStatus::Trial {
            days_remaining: days_remaining as u32,
        }
    }
}

// ---------------------------------------------------------------------------
// Polar API client
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ActivateRequest<'a> {
    key: &'a str,
    organization_id: &'a str,
    label: &'a str,
    meta: ActivateMeta<'a>,
}

#[derive(Debug, Serialize)]
struct ActivateMeta<'a> {
    app_version: &'a str,
}

#[derive(Debug, Deserialize)]
struct ActivateResponse {
    id: String,
}

#[derive(Debug, Serialize)]
struct ValidateRequest<'a> {
    key: &'a str,
    activation_id: &'a str,
    organization_id: &'a str,
}

/// Activate a license key against Polar. Returns the activation ID Polar
/// assigned to this device.
pub async fn polar_activate(
    key: &str,
    label: &str,
    app_version: &str,
) -> Result<String, AppError> {
    if POLAR_ORGANIZATION_ID.is_empty() {
        return Err(AppError::LicenseValidationFailed(
            "License activation is not configured (missing POLAR_ORGANIZATION_ID).".into(),
        ));
    }
    let body = ActivateRequest {
        key,
        organization_id: POLAR_ORGANIZATION_ID,
        label,
        meta: ActivateMeta { app_version },
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{POLAR_API_BASE}/v1/customer-portal/license-keys/activate"))
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::LicenseValidationFailed(format!("Network error: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::LicenseValidationFailed(format!(
            "Activation failed ({status}): {text}"
        )));
    }
    let parsed: ActivateResponse = resp
        .json()
        .await
        .map_err(|e| AppError::LicenseValidationFailed(format!("Bad response: {e}")))?;
    Ok(parsed.id)
}

/// Validate a previously-activated license. `Ok(())` means the license is
/// still valid; otherwise the error message describes why.
pub async fn polar_validate(key: &str, activation_id: &str) -> Result<(), AppError> {
    if POLAR_ORGANIZATION_ID.is_empty() {
        return Err(AppError::LicenseValidationFailed(
            "License validation is not configured (missing POLAR_ORGANIZATION_ID).".into(),
        ));
    }
    let body = ValidateRequest {
        key,
        activation_id,
        organization_id: POLAR_ORGANIZATION_ID,
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{POLAR_API_BASE}/v1/customer-portal/license-keys/validate"))
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::LicenseValidationFailed(format!("Network error: {e}")))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(AppError::LicenseValidationFailed(format!(
            "Validation rejected ({status}): {text}"
        )))
    }
}

/// A device label Polar shows in the customer portal alongside each
/// activation. We use the machine hostname so the user can see "Andre's
/// MacBook Pro" instead of an opaque ID.
pub fn device_label() -> String {
    hostname_string().unwrap_or_else(|| "Unknown device".into())
}

#[cfg(target_os = "macos")]
fn hostname_string() -> Option<String> {
    std::process::Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}

#[cfg(not(target_os = "macos"))]
fn hostname_string() -> Option<String> {
    std::env::var("HOSTNAME").ok()
}

/// Replace the in-memory + on-disk license record with `record`, returning
/// the new status.
pub fn store_license(
    base_dir: &Path,
    storage: &mut LicenseStorage,
    record: LicenseRecord,
) -> Result<LicenseStatus, AppError> {
    storage.license = Some(record);
    save_storage_to(base_dir, storage)?;
    Ok(compute_status(storage, now_unix_secs()))
}

/// Remove the license record, returning the new status (will be Trial or
/// TrialExpired depending on first_launch_at).
pub fn clear_license(
    base_dir: &Path,
    storage: &mut LicenseStorage,
) -> Result<LicenseStatus, AppError> {
    storage.license = None;
    save_storage_to(base_dir, storage)?;
    Ok(compute_status(storage, now_unix_secs()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// RAII temp dir mirroring the pattern used in `model_manager` tests.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir()
                .join(format!("tucknotes_lic_test_{}_{id}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn fresh_storage_is_in_trial() {
        let now = 1_000_000_i64;
        let storage = LicenseStorage::fresh(now);
        let status = compute_status(&storage, now);
        match status {
            LicenseStatus::Trial { days_remaining } => assert_eq!(days_remaining, 14),
            other => panic!("expected Trial, got {other:?}"),
        }
    }

    #[test]
    fn trial_decrements_over_time() {
        let now = 1_000_000_i64;
        let storage = LicenseStorage::fresh(now);
        let status = compute_status(&storage, now + (3 * 86_400));
        match status {
            LicenseStatus::Trial { days_remaining } => assert_eq!(days_remaining, 11),
            other => panic!("expected Trial, got {other:?}"),
        }
    }

    #[test]
    fn trial_expires_after_14_days() {
        let now = 1_000_000_i64;
        let storage = LicenseStorage::fresh(now);
        let status = compute_status(&storage, now + (15 * 86_400));
        assert_eq!(status, LicenseStatus::TrialExpired);
    }

    #[test]
    fn licensed_status_within_grace() {
        let mut storage = LicenseStorage::fresh(0);
        storage.license = Some(LicenseRecord {
            key: "key".into(),
            activation_id: "act".into(),
            last_validated_at: 1_000_000,
        });
        let status = compute_status(&storage, 1_000_000 + (3 * 86_400));
        assert!(matches!(status, LicenseStatus::Licensed { .. }));
        assert!(status.allows_paid_features());
    }

    #[test]
    fn licensed_status_after_grace_is_invalid() {
        let mut storage = LicenseStorage::fresh(0);
        storage.license = Some(LicenseRecord {
            key: "key".into(),
            activation_id: "act".into(),
            last_validated_at: 1_000_000,
        });
        let status = compute_status(&storage, 1_000_000 + (8 * 86_400));
        assert!(matches!(status, LicenseStatus::LicenseInvalid { .. }));
        assert!(!status.allows_paid_features());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new();
        let mut storage = LicenseStorage::fresh(123);
        storage.license = Some(LicenseRecord {
            key: "abc".into(),
            activation_id: "xyz".into(),
            last_validated_at: 456,
        });
        save_storage_to(dir.path(), &storage).unwrap();
        let loaded = load_storage_from(dir.path()).unwrap();
        assert_eq!(loaded, storage);
    }

    #[test]
    fn load_missing_returns_fresh() {
        let dir = TempDir::new();
        let loaded = load_storage_from(dir.path()).unwrap();
        assert!(loaded.license.is_none());
    }
}

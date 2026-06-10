use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::errors::{lock_or_err, AppError};
use crate::models::licensing::{
    LicenseRecord, LicenseStatus, LicenseStorage, LicensingState, OFFLINE_GRACE_SECS, TRIAL_SECS,
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

/// Test seam: points the Polar client at a local mock server. Only settable
/// from tests; production always uses `POLAR_API_BASE`.
#[cfg(test)]
static TEST_POLAR_BASE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn polar_api_base() -> String {
    #[cfg(test)]
    if let Some(base) = TEST_POLAR_BASE.get() {
        return base.clone();
    }
    POLAR_API_BASE.to_string()
}

fn polar_org_id() -> &'static str {
    #[cfg(test)]
    if TEST_POLAR_BASE.get().is_some() {
        return "test-org";
    }
    POLAR_ORGANIZATION_ID
}

/// How often the background loop re-validates a stored license. Must be
/// comfortably shorter than `OFFLINE_GRACE_SECS` so an online user never
/// sees the grace period lapse.
pub const REVALIDATE_INTERVAL_SECS: u64 = 6 * 60 * 60;

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

/// Polar returns 200 on validate even for revoked/disabled keys; the
/// verdict lives in this `status` field.
#[derive(Debug, Deserialize)]
struct ValidateResponse {
    #[serde(default)]
    status: Option<String>,
}

/// Error body shape for Polar's 4xx responses.
#[derive(Debug, Default, Deserialize)]
struct PolarErrorBody {
    #[serde(default)]
    detail: Option<String>,
}

/// Result of a validation attempt that reached Polar and got an
/// authoritative answer. Transient failures (network, 429, 5xx) surface as
/// `Err(AppError)` from `polar_validate` instead, and must never clear a
/// stored license — the offline grace period covers them.
#[derive(Debug, PartialEq)]
pub enum ValidationOutcome {
    Valid,
    Rejected(String),
}

fn polar_error_detail(text: &str) -> Option<String> {
    serde_json::from_str::<PolarErrorBody>(text)
        .ok()
        .and_then(|b| b.detail)
        .filter(|d| !d.is_empty())
}

/// Map a non-success activate response to a user-facing error. Polar's
/// activate endpoint uses 403 for "activation not supported or limit
/// reached" and 404 for "key not found" — don't conflate them.
fn classify_activate_error(status: u16, text: &str) -> AppError {
    let msg = match status {
        403 => {
            let base = "This license key has reached its activation limit. \
                        Deactivate it on another device and try again.";
            match polar_error_detail(text) {
                Some(detail) => format!("{base} ({detail})"),
                None => base.to_string(),
            }
        }
        404 => "That license key wasn't found. Double-check the key and try again.".to_string(),
        429 => "Too many attempts. Please wait a moment and try again.".to_string(),
        _ => "License request failed. Please try again.".to_string(),
    };
    AppError::LicenseValidationFailed(msg)
}

/// Interpret a 2xx validate body. Anything other than an explicit
/// revoked/disabled status counts as valid — never clear on ambiguity.
fn classify_validate_body(text: &str) -> ValidationOutcome {
    let status = serde_json::from_str::<ValidateResponse>(text)
        .ok()
        .and_then(|r| r.status);
    match status.as_deref() {
        Some("revoked") => ValidationOutcome::Rejected("This license key has been revoked.".into()),
        Some("disabled") => {
            ValidationOutcome::Rejected("This license key has been disabled.".into())
        }
        _ => ValidationOutcome::Valid,
    }
}

/// Interpret a non-2xx validate response. Only 404 (key/activation gone) is
/// a definitive rejection; everything else is transient.
fn classify_validate_failure(status: u16, text: &str) -> Result<ValidationOutcome, AppError> {
    if status == 404 {
        let reason = polar_error_detail(text)
            .unwrap_or_else(|| "License key or activation no longer exists.".into());
        return Ok(ValidationOutcome::Rejected(reason));
    }
    Err(AppError::LicenseValidationFailed(format!(
        "License validation failed (HTTP {status}). Will retry later."
    )))
}

/// Activate a license key against Polar. Returns the activation ID Polar
/// assigned to this device.
pub async fn polar_activate(
    key: &str,
    label: &str,
    app_version: &str,
) -> Result<String, AppError> {
    if polar_org_id().is_empty() {
        return Err(AppError::LicenseValidationFailed(
            "License activation is not configured (missing POLAR_ORGANIZATION_ID).".into(),
        ));
    }
    let body = ActivateRequest {
        key,
        organization_id: polar_org_id(),
        label,
        meta: ActivateMeta { app_version },
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/customer-portal/license-keys/activate", polar_api_base()))
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::LicenseValidationFailed(format!("Network error: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        eprintln!("[licensing] Polar activate failed ({status}): {text}");
        return Err(classify_activate_error(status.as_u16(), &text));
    }
    let parsed: ActivateResponse = resp
        .json()
        .await
        .map_err(|e| AppError::LicenseValidationFailed(format!("Bad response: {e}")))?;
    Ok(parsed.id)
}

/// Validate a previously-activated license. `Ok(_)` carries Polar's
/// authoritative verdict; `Err(_)` means we couldn't get one (network, rate
/// limit, server error) and the caller must keep the stored license intact.
pub async fn polar_validate(
    key: &str,
    activation_id: &str,
) -> Result<ValidationOutcome, AppError> {
    if polar_org_id().is_empty() {
        return Err(AppError::LicenseValidationFailed(
            "License validation is not configured (missing POLAR_ORGANIZATION_ID).".into(),
        ));
    }
    let body = ValidateRequest {
        key,
        activation_id,
        organization_id: polar_org_id(),
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/customer-portal/license-keys/validate", polar_api_base()))
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::LicenseValidationFailed(format!("Network error: {e}")))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(classify_validate_body(&text))
    } else {
        eprintln!("[licensing] Polar validate failed ({status}): {text}");
        classify_validate_failure(status.as_u16(), &text)
    }
}

#[derive(Debug, Serialize)]
struct DeactivateRequest<'a> {
    key: &'a str,
    activation_id: &'a str,
    organization_id: &'a str,
}

/// Release a previously-issued activation on Polar so the slot becomes
/// available for re-activation. Without this, removing a key locally leaves
/// the activation orphaned on Polar and the user hits "activation limit
/// reached" on the next attempt.
pub async fn polar_deactivate(key: &str, activation_id: &str) -> Result<(), AppError> {
    if polar_org_id().is_empty() {
        return Err(AppError::LicenseValidationFailed(
            "License deactivation is not configured (missing POLAR_ORGANIZATION_ID).".into(),
        ));
    }
    let body = DeactivateRequest {
        key,
        activation_id,
        organization_id: polar_org_id(),
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/customer-portal/license-keys/deactivate", polar_api_base()))
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::LicenseValidationFailed(format!("Network error: {e}")))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        eprintln!("[licensing] Polar deactivate failed ({status}): {text}");
        Err(AppError::LicenseValidationFailed(
            "License deactivation request failed.".into(),
        ))
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

/// Persist the result of an authoritative validation: refresh the record's
/// `last_validated_at` on `Valid`, drop the record on `Rejected`. Transient
/// failures never reach this function.
pub fn apply_validation_outcome(
    base_dir: &Path,
    storage: &mut LicenseStorage,
    record: LicenseRecord,
    outcome: ValidationOutcome,
    now: i64,
) -> Result<LicenseStatus, AppError> {
    match outcome {
        ValidationOutcome::Valid => {
            storage.license = Some(LicenseRecord {
                last_validated_at: now,
                ..record
            });
            save_storage_to(base_dir, storage)?;
            Ok(compute_status(storage, now))
        }
        ValidationOutcome::Rejected(reason) => {
            clear_license(base_dir, storage)?;
            Ok(LicenseStatus::LicenseInvalid { reason })
        }
    }
}

/// Broadcast a status change so every webview/component updates without
/// waiting for the next poll.
pub fn emit_status(app: &AppHandle, status: &LicenseStatus) {
    use tauri::Emitter;
    let _ = app.emit("license-status-changed", status);
}

/// Re-check the stored license against Polar. Refreshes the grace window on
/// success, clears the record only on a definitive rejection, and leaves
/// everything untouched when Polar can't be reached.
pub async fn revalidate(app: &AppHandle) -> Result<LicenseStatus, AppError> {
    let state = app.state::<LicensingState>();
    let dir = resolve_data_dir(app)?;
    let status = revalidate_in(&dir, &state.storage).await?;
    emit_status(app, &status);
    Ok(status)
}

/// `revalidate` without the Tauri plumbing, so tests can drive it against a
/// temp dir and a mock Polar server.
pub async fn revalidate_in(
    base_dir: &Path,
    storage_mutex: &std::sync::Mutex<LicenseStorage>,
) -> Result<LicenseStatus, AppError> {
    let snapshot = {
        let storage = lock_or_err(storage_mutex)?;
        storage.license.clone()
    };
    let Some(record) = snapshot else {
        // Nothing to revalidate; surface current trial state.
        let storage = lock_or_err(storage_mutex)?;
        return Ok(compute_status(&storage, now_unix_secs()));
    };

    match polar_validate(&record.key, &record.activation_id).await {
        Ok(outcome) => {
            let mut storage = lock_or_err(storage_mutex)?;
            apply_validation_outcome(base_dir, &mut storage, record, outcome, now_unix_secs())
        }
        Err(err) => {
            // Transient: keep the record and let the grace period ride.
            eprintln!("[licensing] revalidation deferred: {err}");
            let storage = lock_or_err(storage_mutex)?;
            Ok(compute_status(&storage, now_unix_secs()))
        }
    }
}

/// Activate `key` for this device. When the same key is already stored,
/// re-validates the existing activation instead of creating a new one so
/// repeated entries don't leak Polar activation slots; when a different key
/// is stored, best-effort releases the old slot first.
pub async fn activate_key(
    base_dir: &Path,
    storage_mutex: &std::sync::Mutex<LicenseStorage>,
    key: &str,
    label: &str,
    app_version: &str,
) -> Result<LicenseStatus, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::LicenseValidationFailed(
            "License key is empty.".into(),
        ));
    }

    let snapshot = {
        let storage = lock_or_err(storage_mutex)?;
        storage.license.clone()
    };

    if let Some(existing) = snapshot {
        if existing.key == trimmed {
            match polar_validate(&existing.key, &existing.activation_id).await {
                Ok(ValidationOutcome::Valid) => {
                    let mut storage = lock_or_err(storage_mutex)?;
                    return apply_validation_outcome(
                        base_dir,
                        &mut storage,
                        existing,
                        ValidationOutcome::Valid,
                        now_unix_secs(),
                    );
                }
                // Activation is gone server-side — activate fresh below.
                Ok(ValidationOutcome::Rejected(_)) => {}
                // Polar unreachable: don't churn activation slots blindly.
                Err(err) => return Err(err),
            }
        } else if let Err(err) = polar_deactivate(&existing.key, &existing.activation_id).await {
            // Switching keys: best-effort release of the old slot.
            eprintln!("[licensing] Polar deactivation of old key failed (continuing): {err}");
        }
    }

    let activation_id = polar_activate(trimmed, label, app_version).await?;
    let mut storage = lock_or_err(storage_mutex)?;
    let record = LicenseRecord {
        key: trimmed.to_string(),
        activation_id,
        last_validated_at: now_unix_secs(),
    };
    store_license(base_dir, &mut storage, record)
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

    fn error_message(err: AppError) -> String {
        match err {
            AppError::LicenseValidationFailed(msg) => msg,
            other => panic!("expected LicenseValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn activate_403_means_activation_limit_not_missing_key() {
        let msg = error_message(classify_activate_error(403, ""));
        assert!(msg.contains("activation limit"), "got: {msg}");
        assert!(!msg.contains("wasn't found"), "got: {msg}");
    }

    #[test]
    fn activate_403_includes_polar_detail() {
        let body = r#"{"error":"NotPermitted","detail":"License key activation limit already reached"}"#;
        let msg = error_message(classify_activate_error(403, body));
        assert!(msg.contains("limit already reached"), "got: {msg}");
    }

    #[test]
    fn activate_404_means_key_not_found() {
        let msg = error_message(classify_activate_error(404, ""));
        assert!(msg.contains("wasn't found"), "got: {msg}");
    }

    #[test]
    fn activate_500_is_generic() {
        let msg = error_message(classify_activate_error(500, ""));
        assert!(msg.contains("try again"), "got: {msg}");
        assert!(!msg.contains("wasn't found"), "got: {msg}");
    }

    #[test]
    fn validate_body_granted_is_valid() {
        assert_eq!(
            classify_validate_body(r#"{"status":"granted"}"#),
            ValidationOutcome::Valid
        );
    }

    #[test]
    fn validate_body_revoked_and_disabled_are_rejected() {
        assert!(matches!(
            classify_validate_body(r#"{"status":"revoked"}"#),
            ValidationOutcome::Rejected(_)
        ));
        assert!(matches!(
            classify_validate_body(r#"{"status":"disabled"}"#),
            ValidationOutcome::Rejected(_)
        ));
    }

    #[test]
    fn validate_body_ambiguous_never_rejects() {
        // Missing status, unknown status, or garbage must never clear a license.
        assert_eq!(classify_validate_body("{}"), ValidationOutcome::Valid);
        assert_eq!(
            classify_validate_body(r#"{"status":"something-new"}"#),
            ValidationOutcome::Valid
        );
        assert_eq!(classify_validate_body("not json"), ValidationOutcome::Valid);
    }

    #[test]
    fn validate_404_is_definitive_rejection() {
        let outcome = classify_validate_failure(404, "").unwrap();
        assert!(matches!(outcome, ValidationOutcome::Rejected(_)));
    }

    #[test]
    fn validate_transient_statuses_are_errors_not_rejections() {
        for status in [403u16, 429, 500, 503] {
            assert!(
                classify_validate_failure(status, "").is_err(),
                "HTTP {status} must be transient"
            );
        }
    }

    fn record(last_validated_at: i64) -> LicenseRecord {
        LicenseRecord {
            key: "key".into(),
            activation_id: "act".into(),
            last_validated_at,
        }
    }

    #[test]
    fn apply_valid_refreshes_grace_window() {
        let dir = TempDir::new();
        let mut storage = LicenseStorage::fresh(0);
        storage.license = Some(record(1_000));
        let status =
            apply_validation_outcome(dir.path(), &mut storage, record(1_000), ValidationOutcome::Valid, 2_000)
                .unwrap();
        assert_eq!(storage.license.as_ref().unwrap().last_validated_at, 2_000);
        assert!(matches!(status, LicenseStatus::Licensed { .. }));
        // Persisted, not just in-memory.
        let loaded = load_storage_from(dir.path()).unwrap();
        assert_eq!(loaded, storage);
    }

    // -----------------------------------------------------------------------
    // HTTP-level tests against a mock Polar server. One server is shared by
    // every test (the base-URL override is process-global); isolation comes
    // from each test using a unique license key that its mocks match on.
    // -----------------------------------------------------------------------

    use httpmock::prelude::*;

    static MOCK_SERVER: tokio::sync::OnceCell<MockServer> = tokio::sync::OnceCell::const_new();

    async fn mock_server() -> &'static MockServer {
        MOCK_SERVER
            .get_or_init(|| async {
                let server = MockServer::start_async().await;
                TEST_POLAR_BASE
                    .set(server.base_url())
                    .expect("polar base override set twice");
                server
            })
            .await
    }

    async fn mock_validate(server: &MockServer, key: &str, status: u16, body: serde_json::Value) {
        server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/customer-portal/license-keys/validate")
                    .json_body_partial(format!(r#"{{"key":"{key}"}}"#));
                then.status(status).json_body(body);
            })
            .await;
    }

    async fn mock_activate<'a>(
        server: &'a MockServer,
        key: &str,
        status: u16,
        body: serde_json::Value,
    ) -> httpmock::Mock<'a> {
        server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/customer-portal/license-keys/activate")
                    .json_body_partial(format!(r#"{{"key":"{key}"}}"#));
                then.status(status).json_body(body);
            })
            .await
    }

    fn storage_with(dir: &Path, key: &str, activation_id: &str, last_validated_at: i64) -> Mutex<LicenseStorage> {
        let mut storage = LicenseStorage::fresh(0);
        storage.license = Some(LicenseRecord {
            key: key.into(),
            activation_id: activation_id.into(),
            last_validated_at,
        });
        save_storage_to(dir, &storage).unwrap();
        Mutex::new(storage)
    }

    use std::sync::Mutex;

    #[tokio::test]
    async fn polar_activate_parses_activation_id() {
        let server = mock_server().await;
        mock_activate(server, "k-act-ok", 200, serde_json::json!({"id": "act-123"})).await;
        let id = polar_activate("k-act-ok", "Test Device", "1.0.0").await.unwrap();
        assert_eq!(id, "act-123");
    }

    #[tokio::test]
    async fn polar_activate_403_surfaces_limit_error() {
        let server = mock_server().await;
        mock_activate(
            server,
            "k-act-limit",
            403,
            serde_json::json!({"error": "NotPermitted", "detail": "License key activation limit already reached"}),
        )
        .await;
        let msg = error_message(
            polar_activate("k-act-limit", "Test Device", "1.0.0")
                .await
                .unwrap_err(),
        );
        assert!(msg.contains("activation limit"), "got: {msg}");
        assert!(msg.contains("limit already reached"), "got: {msg}");
    }

    #[tokio::test]
    async fn polar_validate_granted_is_valid() {
        let server = mock_server().await;
        mock_validate(server, "k-val-granted", 200, serde_json::json!({"status": "granted"})).await;
        let outcome = polar_validate("k-val-granted", "act").await.unwrap();
        assert_eq!(outcome, ValidationOutcome::Valid);
    }

    #[tokio::test]
    async fn polar_validate_revoked_is_rejected() {
        let server = mock_server().await;
        mock_validate(server, "k-val-revoked", 200, serde_json::json!({"status": "revoked"})).await;
        let outcome = polar_validate("k-val-revoked", "act").await.unwrap();
        assert!(matches!(outcome, ValidationOutcome::Rejected(_)));
    }

    #[tokio::test]
    async fn polar_validate_404_is_rejected_and_500_is_transient() {
        let server = mock_server().await;
        mock_validate(server, "k-val-404", 404, serde_json::json!({"error": "ResourceNotFound"})).await;
        mock_validate(server, "k-val-500", 500, serde_json::json!({})).await;
        assert!(matches!(
            polar_validate("k-val-404", "act").await.unwrap(),
            ValidationOutcome::Rejected(_)
        ));
        assert!(polar_validate("k-val-500", "act").await.is_err());
    }

    #[tokio::test]
    async fn revalidate_refreshes_grace_window() {
        let server = mock_server().await;
        let dir = TempDir::new();
        let old = now_unix_secs() - 6 * 86_400;
        let storage = storage_with(dir.path(), "k-reval-ok", "act-1", old);
        mock_validate(server, "k-reval-ok", 200, serde_json::json!({"status": "granted"})).await;

        let status = revalidate_in(dir.path(), &storage).await.unwrap();
        assert!(matches!(status, LicenseStatus::Licensed { .. }));
        let record = storage.lock().unwrap().license.clone().unwrap();
        assert!(record.last_validated_at > old, "grace window not refreshed");
        assert_eq!(record.activation_id, "act-1");
        // Persisted to disk too.
        let on_disk = load_storage_from(dir.path()).unwrap();
        assert_eq!(on_disk.license.unwrap().last_validated_at, record.last_validated_at);
    }

    #[tokio::test]
    async fn revalidate_transient_failure_keeps_license() {
        let server = mock_server().await;
        let dir = TempDir::new();
        let recent = now_unix_secs() - 86_400;
        let storage = storage_with(dir.path(), "k-reval-500", "act-1", recent);
        let before = storage.lock().unwrap().clone();
        mock_validate(server, "k-reval-500", 500, serde_json::json!({})).await;

        let status = revalidate_in(dir.path(), &storage).await.unwrap();
        // Still licensed via the untouched grace window.
        assert!(matches!(status, LicenseStatus::Licensed { .. }));
        assert_eq!(*storage.lock().unwrap(), before, "storage must be untouched");
        assert_eq!(load_storage_from(dir.path()).unwrap(), before);
    }

    #[tokio::test]
    async fn revalidate_definitive_rejection_clears_license() {
        let server = mock_server().await;
        let dir = TempDir::new();
        let storage = storage_with(dir.path(), "k-reval-404", "act-1", now_unix_secs());
        mock_validate(server, "k-reval-404", 404, serde_json::json!({"error": "ResourceNotFound"})).await;

        let status = revalidate_in(dir.path(), &storage).await.unwrap();
        assert!(matches!(status, LicenseStatus::LicenseInvalid { .. }));
        assert!(storage.lock().unwrap().license.is_none());
        assert!(load_storage_from(dir.path()).unwrap().license.is_none());
    }

    #[tokio::test]
    async fn activate_same_key_reuses_existing_activation() {
        let server = mock_server().await;
        let dir = TempDir::new();
        let old = now_unix_secs() - 6 * 86_400;
        let storage = storage_with(dir.path(), "k-reuse", "act-original", old);
        mock_validate(server, "k-reuse", 200, serde_json::json!({"status": "granted"})).await;
        // If the code wrongly activates again, this mock would hand out a new id.
        let activate = mock_activate(server, "k-reuse", 200, serde_json::json!({"id": "act-LEAKED"})).await;

        let status = activate_key(dir.path(), &storage, "k-reuse", "Test Device", "1.0.0")
            .await
            .unwrap();
        assert!(matches!(status, LicenseStatus::Licensed { .. }));
        let record = storage.lock().unwrap().license.clone().unwrap();
        assert_eq!(record.activation_id, "act-original", "must not burn a new activation slot");
        assert!(record.last_validated_at > old);
        assert_eq!(activate.hits_async().await, 0, "activate endpoint must not be called");
    }

    #[tokio::test]
    async fn activate_same_key_with_dead_activation_activates_fresh() {
        let server = mock_server().await;
        let dir = TempDir::new();
        let storage = storage_with(dir.path(), "k-dead-act", "act-gone", now_unix_secs());
        // Old activation was deleted server-side (e.g. via the Polar dashboard).
        mock_validate(server, "k-dead-act", 404, serde_json::json!({"error": "ResourceNotFound"})).await;
        mock_activate(server, "k-dead-act", 200, serde_json::json!({"id": "act-fresh"})).await;

        let status = activate_key(dir.path(), &storage, "k-dead-act", "Test Device", "1.0.0")
            .await
            .unwrap();
        assert!(matches!(status, LicenseStatus::Licensed { .. }));
        assert_eq!(storage.lock().unwrap().license.clone().unwrap().activation_id, "act-fresh");
    }

    #[tokio::test]
    async fn activate_same_key_transient_failure_changes_nothing() {
        let server = mock_server().await;
        let dir = TempDir::new();
        let storage = storage_with(dir.path(), "k-reuse-500", "act-1", now_unix_secs());
        let before = storage.lock().unwrap().clone();
        mock_validate(server, "k-reuse-500", 500, serde_json::json!({})).await;
        let activate = mock_activate(server, "k-reuse-500", 200, serde_json::json!({"id": "act-CHURNED"})).await;

        let result = activate_key(dir.path(), &storage, "k-reuse-500", "Test Device", "1.0.0").await;
        assert!(result.is_err(), "transient validate failure must surface as an error");
        assert_eq!(*storage.lock().unwrap(), before, "storage must be untouched");
        assert_eq!(activate.hits_async().await, 0, "must not churn activation slots while Polar is down");
    }

    #[tokio::test]
    async fn activate_different_key_releases_old_slot() {
        let server = mock_server().await;
        let dir = TempDir::new();
        let storage = storage_with(dir.path(), "k-old", "act-old", now_unix_secs());
        let deactivate = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/customer-portal/license-keys/deactivate")
                    .json_body_partial(r#"{"key":"k-old"}"#);
                then.status(204);
            })
            .await;
        mock_activate(server, "k-new", 200, serde_json::json!({"id": "act-new"})).await;

        let status = activate_key(dir.path(), &storage, "k-new", "Test Device", "1.0.0")
            .await
            .unwrap();
        assert!(matches!(status, LicenseStatus::Licensed { .. }));
        let record = storage.lock().unwrap().license.clone().unwrap();
        assert_eq!(record.key, "k-new");
        assert_eq!(record.activation_id, "act-new");
        assert_eq!(deactivate.hits_async().await, 1, "old slot must be released");
    }

    #[tokio::test]
    async fn activate_empty_key_is_rejected_without_network() {
        let dir = TempDir::new();
        let storage = Mutex::new(LicenseStorage::fresh(now_unix_secs()));
        let result = activate_key(dir.path(), &storage, "   ", "Test Device", "1.0.0").await;
        let msg = error_message(result.unwrap_err());
        assert!(msg.contains("empty"), "got: {msg}");
    }

    #[test]
    fn apply_rejected_clears_license() {
        let dir = TempDir::new();
        let mut storage = LicenseStorage::fresh(0);
        storage.license = Some(record(1_000));
        let status = apply_validation_outcome(
            dir.path(),
            &mut storage,
            record(1_000),
            ValidationOutcome::Rejected("revoked".into()),
            2_000,
        )
        .unwrap();
        assert!(storage.license.is_none());
        assert!(matches!(status, LicenseStatus::LicenseInvalid { .. }));
        let loaded = load_storage_from(dir.path()).unwrap();
        assert!(loaded.license.is_none());
    }
}

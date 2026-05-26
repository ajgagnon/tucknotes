use tauri::Manager;

use crate::errors::{lock_or_err, AppError};
use crate::models::licensing::{LicenseRecord, LicenseStatus, LicensingState};
use crate::services::licensing as licensing_svc;

fn data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::IoError(e.to_string()))
}

#[tauri::command]
pub fn get_license_status(
    state: tauri::State<'_, LicensingState>,
) -> Result<LicenseStatus, AppError> {
    let storage = lock_or_err(&state.storage)?;
    Ok(licensing_svc::compute_status(
        &storage,
        licensing_svc::now_unix_secs(),
    ))
}

#[tauri::command]
pub async fn activate_license_key(
    app: tauri::AppHandle,
    state: tauri::State<'_, LicensingState>,
    key: String,
) -> Result<LicenseStatus, AppError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::LicenseValidationFailed(
            "License key is empty.".into(),
        ));
    }

    let label = licensing_svc::device_label();
    let app_version = app.package_info().version.to_string();
    let activation_id = licensing_svc::polar_activate(trimmed, &label, &app_version).await?;

    let dir = data_dir(&app)?;
    let mut storage = lock_or_err(&state.storage)?;
    let record = LicenseRecord {
        key: trimmed.to_string(),
        activation_id,
        last_validated_at: licensing_svc::now_unix_secs(),
    };
    licensing_svc::store_license(&dir, &mut storage, record)
}

#[tauri::command]
pub async fn deactivate_license(
    app: tauri::AppHandle,
    state: tauri::State<'_, LicensingState>,
) -> Result<LicenseStatus, AppError> {
    let snapshot = {
        let storage = lock_or_err(&state.storage)?;
        storage.license.clone()
    };

    if let Some(record) = snapshot {
        // Best-effort: release Polar's activation slot so the key can be
        // re-activated later. If Polar is unreachable we still clear local —
        // the user asked to deactivate this device.
        if let Err(err) =
            licensing_svc::polar_deactivate(&record.key, &record.activation_id).await
        {
            eprintln!("[licensing] Polar deactivation failed (clearing local anyway): {err}");
        }
    }

    let dir = data_dir(&app)?;
    let mut storage = lock_or_err(&state.storage)?;
    licensing_svc::clear_license(&dir, &mut storage)
}

/// Re-check a stored license against Polar. Updates `last_validated_at`
/// on success; returns the resulting status (which may flip to
/// `LicenseInvalid` if Polar rejects).
#[tauri::command]
pub async fn revalidate_license(
    app: tauri::AppHandle,
    state: tauri::State<'_, LicensingState>,
) -> Result<LicenseStatus, AppError> {
    let snapshot = {
        let storage = lock_or_err(&state.storage)?;
        storage.license.clone()
    };
    let Some(record) = snapshot else {
        // Nothing to revalidate; surface current trial state.
        let storage = lock_or_err(&state.storage)?;
        return Ok(licensing_svc::compute_status(
            &storage,
            licensing_svc::now_unix_secs(),
        ));
    };

    match licensing_svc::polar_validate(&record.key, &record.activation_id).await {
        Ok(()) => {
            let dir = data_dir(&app)?;
            let mut storage = lock_or_err(&state.storage)?;
            let updated = LicenseRecord {
                last_validated_at: licensing_svc::now_unix_secs(),
                ..record
            };
            licensing_svc::store_license(&dir, &mut storage, updated)
        }
        Err(AppError::LicenseValidationFailed(reason)) => {
            // Server explicitly rejected: clear stored license. Caller
            // sees `LicenseInvalid` next time it asks for status.
            let dir = data_dir(&app)?;
            let mut storage = lock_or_err(&state.storage)?;
            licensing_svc::clear_license(&dir, &mut storage)?;
            Ok(LicenseStatus::LicenseInvalid { reason })
        }
        Err(other) => Err(other),
    }
}

/// Internal helper used by gated commands (recording, summarization) to
/// short-circuit when the user has no entitlement.
pub fn require_paid_entitlement(state: &LicensingState) -> Result<(), AppError> {
    let storage = lock_or_err(&state.storage)?;
    let status = licensing_svc::compute_status(&storage, licensing_svc::now_unix_secs());
    if status.allows_paid_features() {
        Ok(())
    } else {
        Err(AppError::LicenseRequired(
            "Trial expired. Enter a license key in Settings to continue.".into(),
        ))
    }
}

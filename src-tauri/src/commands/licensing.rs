use tauri::Manager;

use crate::errors::{lock_or_err, AppError};
use crate::models::licensing::{LicenseStatus, LicensingState};
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
    let label = licensing_svc::device_label();
    let app_version = app.package_info().version.to_string();
    let dir = data_dir(&app)?;
    let status =
        licensing_svc::activate_key(&dir, &state.storage, &key, &label, &app_version).await?;
    licensing_svc::emit_status(&app, &status);
    Ok(status)
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
    let status = licensing_svc::clear_license(&dir, &mut storage)?;
    licensing_svc::emit_status(&app, &status);
    Ok(status)
}

/// Re-check a stored license against Polar. Updates `last_validated_at`
/// on success; clears the license only if Polar definitively rejects it
/// (transient failures keep the offline grace period running).
#[tauri::command]
pub async fn revalidate_license(app: tauri::AppHandle) -> Result<LicenseStatus, AppError> {
    licensing_svc::revalidate(&app).await
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

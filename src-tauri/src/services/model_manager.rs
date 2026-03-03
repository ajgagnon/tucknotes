use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

use crate::errors::AppError;
use crate::models::{AppSettings, DownloadProgress, LlmModel, LlmModelInfo, ModelInfo, WhisperModel};

/// Resolve the Tauri app data directory (e.g. ~/Library/Application Support/com.grain.app).
/// This is the only place we depend on the Tauri runtime for path resolution.
fn resolve_data_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::IoError(e.to_string()))
}

// ---------------------------------------------------------------------------
// Core logic — all functions below accept a plain `&Path` so they can be
// unit-tested without spinning up a Tauri runtime.
// ---------------------------------------------------------------------------

/// Return `<base_dir>/models`, creating it on disk if it doesn't already exist.
pub fn ensure_models_dir(base_dir: &Path) -> Result<PathBuf, AppError> {
    let dir = base_dir.join("models");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Read `settings.json` from `base_dir`. Returns `AppSettings::default()` if
/// the file hasn't been created yet (first launch).
pub fn load_settings_from(base_dir: &Path) -> Result<AppSettings, AppError> {
    let path = base_dir.join("settings.json");
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let contents = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&contents)?)
}

/// Persist `settings` to `<base_dir>/settings.json`.
/// Uses atomic write-to-tmp-then-rename so a crash mid-write can't corrupt
/// the settings file.
pub fn save_settings_to(base_dir: &Path, settings: &AppSettings) -> Result<(), AppError> {
    if !base_dir.exists() {
        std::fs::create_dir_all(base_dir)?;
    }
    let path = base_dir.join("settings.json");
    let tmp_path = base_dir.join("settings.json.tmp");
    let json = serde_json::to_string_pretty(settings)?;
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Check whether the given model's .bin file exists on disk.
/// Pure read-only — does not create any directories.
pub fn is_model_downloaded_in(base_dir: &Path, model: &WhisperModel) -> Result<bool, AppError> {
    Ok(base_dir.join("models").join(model.filename()).exists())
}

/// Resolve the active model's file path from settings + models dir.
/// Returns `None` if no model is selected or the file doesn't exist on disk.
pub fn resolve_model_path(base_dir: &Path) -> Result<Option<PathBuf>, AppError> {
    let settings = load_settings_from(base_dir)?;
    let Some(model) = settings.selected_model else {
        return Ok(None);
    };
    let path = ensure_models_dir(base_dir)?.join(model.filename());
    Ok(path.exists().then_some(path))
}

// ---------------------------------------------------------------------------
// AppHandle wrappers — thin one-liners called by Tauri commands.
// Each resolves the app data dir, then delegates to a core function above.
// ---------------------------------------------------------------------------

pub fn get_models_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    ensure_models_dir(&resolve_data_dir(app)?)
}

pub fn load_settings(app: &AppHandle) -> Result<AppSettings, AppError> {
    load_settings_from(&resolve_data_dir(app)?)
}

pub fn save_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), AppError> {
    save_settings_to(&resolve_data_dir(app)?, settings)
}

pub fn is_model_downloaded(app: &AppHandle, model: &WhisperModel) -> Result<bool, AppError> {
    is_model_downloaded_in(&resolve_data_dir(app)?, model)
}

/// Download a Whisper model .bin file from Hugging Face.
///
/// The download streams to a `.partial` temp file first, then atomically
/// renames it to the final filename once complete. This prevents a
/// half-downloaded file from being mistaken for a valid model.
///
/// Emits `model:download-progress` events to the frontend on every chunk
/// so the UI can show a progress bar.
pub async fn download_model(app: &AppHandle, model: &WhisperModel) -> Result<(), AppError> {
    let base_dir = resolve_data_dir(app)?;
    let models_dir = ensure_models_dir(&base_dir)?;
    let file_path = models_dir.join(model.filename());
    if file_path.exists() {
        return Ok(());
    }
    let partial_path = models_dir.join(format!("{}.partial", model.filename()));

    let response = reqwest::get(model.download_url()).await?;

    if !response.status().is_success() {
        return Err(AppError::DownloadFailed(format!(
            "HTTP {}",
            response.status()
        )));
    }

    // total_bytes is 0 when the server omits Content-Length (unlikely for HF static files)
    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded_bytes: u64 = 0;

    let model_id = model.id().to_owned();

    let mut file = tokio::fs::File::create(&partial_path).await?;

    let mut stream = response.bytes_stream();

    // Stream body chunk-by-chunk to avoid buffering the entire file in memory
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;

        downloaded_bytes += chunk.len() as u64;

        // Fire-and-forget; if the frontend isn't listening, that's fine
        let _ = app.emit(
            "model:download-progress",
            DownloadProgress {
                model_id: model_id.clone(),
                downloaded_bytes,
                total_bytes,
            },
        );
    }

    file.flush().await?;

    // Atomic rename: the final filename only appears once the download is complete
    tokio::fs::rename(&partial_path, &file_path).await?;

    Ok(())
}

/// Return the catalog of all available Whisper models the user can download.
/// This is a static list — no network or disk access required.
pub fn list_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: WhisperModel::BaseEn,
            name: "Base (English Only)".into(),
            description: "Fast and lightweight. Best for English-only meetings.".into(),
            size_bytes: 148_000_000,
            filename: WhisperModel::BaseEn.filename().into(),
            recommended: false,
        },
        ModelInfo {
            id: WhisperModel::LargeV3TurboQ5,
            name: "Large v3 Turbo (Quantized)".into(),
            description: "Higher accuracy, multilingual support. Larger download.".into(),
            size_bytes: 547_000_000,
            filename: WhisperModel::LargeV3TurboQ5.filename().into(),
            recommended: true,
        },
    ]
}

// ---------------------------------------------------------------------------
// LLM model helpers — parallel to the Whisper model functions above.
// ---------------------------------------------------------------------------

pub fn is_llm_model_downloaded_in(base_dir: &Path, model: &LlmModel) -> Result<bool, AppError> {
    Ok(base_dir.join("models").join(model.filename()).exists())
}

pub fn resolve_llm_model_path(base_dir: &Path) -> Result<Option<PathBuf>, AppError> {
    let settings = load_settings_from(base_dir)?;
    let Some(model) = settings.selected_llm_model else {
        return Ok(None);
    };
    let path = ensure_models_dir(base_dir)?.join(model.filename());
    Ok(path.exists().then_some(path))
}

pub fn is_llm_model_downloaded(app: &AppHandle, model: &LlmModel) -> Result<bool, AppError> {
    is_llm_model_downloaded_in(&resolve_data_dir(app)?, model)
}

/// Download an LLM model GGUF file from Hugging Face.
///
/// Same atomic download pattern as `download_model` (stream to `.partial`,
/// then rename). Emits `llm-model:download-progress` events.
pub async fn download_llm_model(app: &AppHandle, model: &LlmModel) -> Result<(), AppError> {
    let base_dir = resolve_data_dir(app)?;
    let models_dir = ensure_models_dir(&base_dir)?;
    let file_path = models_dir.join(model.filename());
    if file_path.exists() {
        return Ok(());
    }
    let partial_path = models_dir.join(format!("{}.partial", model.filename()));

    let response = reqwest::get(model.download_url()).await?;

    if !response.status().is_success() {
        return Err(AppError::DownloadFailed(format!(
            "HTTP {}",
            response.status()
        )));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded_bytes: u64 = 0;
    let model_id = model.id().to_owned();

    let mut file = tokio::fs::File::create(&partial_path).await?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;

        downloaded_bytes += chunk.len() as u64;

        let _ = app.emit(
            "llm-model:download-progress",
            DownloadProgress {
                model_id: model_id.clone(),
                downloaded_bytes,
                total_bytes,
            },
        );
    }

    file.flush().await?;
    tokio::fs::rename(&partial_path, &file_path).await?;

    Ok(())
}

/// Return the catalog of available LLM models.
pub fn list_llm_models() -> Vec<LlmModelInfo> {
    vec![LlmModelInfo {
        id: LlmModel::Qwen3_5_4B_Q4KM,
        name: "Qwen3.5 4B (Q4_K_M)".into(),
        description: "Compact 4-bit quantized model for meeting summarization.".into(),
        size_bytes: 2_740_000_000,
        filename: LlmModel::Qwen3_5_4B_Q4KM.filename().into(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Monotonic counter so each test gets a unique temp directory, even when
    /// tests run in parallel within the same process.
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// RAII guard for a temporary directory. The directory is created on
    /// `TempDir::new()` and automatically deleted when the guard is dropped,
    /// including on test panics.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!("grain_test_{}_{id}", std::process::id()));
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
    fn ensure_models_dir_creates_directory() {
        let base = TempDir::new();
        let models = ensure_models_dir(base.path()).unwrap();
        assert!(models.exists());
        assert!(models.is_dir());
        assert_eq!(models, base.path().join("models"));
    }

    #[test]
    fn load_settings_returns_default_when_missing() {
        let base = TempDir::new();
        let settings = load_settings_from(base.path()).unwrap();
        assert!(settings.selected_model.is_none());
    }

    #[test]
    fn save_and_load_settings_roundtrip() {
        let base = TempDir::new();
        let settings = AppSettings {
            selected_model: Some(WhisperModel::BaseEn),
            ..Default::default()
        };
        save_settings_to(base.path(), &settings).unwrap();
        let loaded = load_settings_from(base.path()).unwrap();
        assert_eq!(loaded.selected_model, Some(WhisperModel::BaseEn));
    }

    #[test]
    fn save_settings_creates_base_dir_if_missing() {
        let root = TempDir::new();
        let nested = root.path().join("nested").join("subdir");
        assert!(!nested.exists());
        let settings = AppSettings {
            selected_model: Some(WhisperModel::BaseEn),
            ..Default::default()
        };
        save_settings_to(&nested, &settings).unwrap();
        assert!(nested.join("settings.json").exists());
    }

    #[test]
    fn is_model_downloaded_false_when_absent() {
        let base = TempDir::new();
        assert!(!is_model_downloaded_in(base.path(), &WhisperModel::BaseEn).unwrap());
    }

    #[test]
    fn is_model_downloaded_true_when_present() {
        let base = TempDir::new();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        fs::write(models_dir.join(WhisperModel::BaseEn.filename()), b"fake").unwrap();
        assert!(is_model_downloaded_in(base.path(), &WhisperModel::BaseEn).unwrap());
        assert!(!is_model_downloaded_in(base.path(), &WhisperModel::LargeV3TurboQ5).unwrap());
    }

    #[test]
    fn resolve_model_path_none_when_no_settings() {
        let base = TempDir::new();
        assert!(resolve_model_path(base.path()).unwrap().is_none());
    }

    #[test]
    fn resolve_model_path_none_when_file_missing() {
        let base = TempDir::new();
        let settings = AppSettings {
            selected_model: Some(WhisperModel::BaseEn),
            ..Default::default()
        };
        save_settings_to(base.path(), &settings).unwrap();
        assert!(resolve_model_path(base.path()).unwrap().is_none());
    }

    #[test]
    fn resolve_model_path_returns_path_when_file_exists() {
        let base = TempDir::new();
        let settings = AppSettings {
            selected_model: Some(WhisperModel::LargeV3TurboQ5),
            ..Default::default()
        };
        save_settings_to(base.path(), &settings).unwrap();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        fs::write(
            models_dir.join(WhisperModel::LargeV3TurboQ5.filename()),
            b"fake",
        )
        .unwrap();
        let path = resolve_model_path(base.path()).unwrap().unwrap();
        assert_eq!(
            path,
            models_dir.join(WhisperModel::LargeV3TurboQ5.filename())
        );
    }

    #[test]
    fn list_models_returns_both_models() {
        let models = list_models();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, WhisperModel::BaseEn);
        assert_eq!(models[1].id, WhisperModel::LargeV3TurboQ5);
    }

    #[test]
    fn list_models_filenames_match_enum() {
        for m in &list_models() {
            assert_eq!(m.filename, m.id.filename());
        }
    }

    #[test]
    fn load_settings_rejects_corrupt_json() {
        let base = TempDir::new();
        fs::write(base.path().join("settings.json"), "not valid json").unwrap();
        let result = load_settings_from(base.path());
        assert!(result.is_err());
    }

    // LLM model tests

    #[test]
    fn is_llm_model_downloaded_false_when_absent() {
        let base = TempDir::new();
        assert!(!is_llm_model_downloaded_in(base.path(), &LlmModel::Qwen3_5_4B_Q4KM).unwrap());
    }

    #[test]
    fn is_llm_model_downloaded_true_when_present() {
        let base = TempDir::new();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        fs::write(
            models_dir.join(LlmModel::Qwen3_5_4B_Q4KM.filename()),
            b"fake",
        )
        .unwrap();
        assert!(is_llm_model_downloaded_in(base.path(), &LlmModel::Qwen3_5_4B_Q4KM).unwrap());
    }

    #[test]
    fn resolve_llm_model_path_none_when_no_settings() {
        let base = TempDir::new();
        assert!(resolve_llm_model_path(base.path()).unwrap().is_none());
    }

    #[test]
    fn resolve_llm_model_path_returns_path_when_file_exists() {
        let base = TempDir::new();
        let settings = AppSettings {
            selected_llm_model: Some(LlmModel::Qwen3_5_4B_Q4KM),
            ..Default::default()
        };
        save_settings_to(base.path(), &settings).unwrap();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        fs::write(
            models_dir.join(LlmModel::Qwen3_5_4B_Q4KM.filename()),
            b"fake",
        )
        .unwrap();
        let path = resolve_llm_model_path(base.path()).unwrap().unwrap();
        assert_eq!(
            path,
            models_dir.join(LlmModel::Qwen3_5_4B_Q4KM.filename())
        );
    }

    #[test]
    fn list_llm_models_returns_one_model() {
        let models = list_llm_models();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, LlmModel::Qwen3_5_4B_Q4KM);
    }

    #[test]
    fn list_llm_models_filename_matches_enum() {
        for m in &list_llm_models() {
            assert_eq!(m.filename, m.id.filename());
        }
    }

    #[test]
    fn settings_roundtrip_with_llm_model() {
        let base = TempDir::new();
        let settings = AppSettings {
            selected_model: Some(WhisperModel::BaseEn),
            selected_llm_model: Some(LlmModel::Qwen3_5_4B_Q4KM),
        };
        save_settings_to(base.path(), &settings).unwrap();
        let loaded = load_settings_from(base.path()).unwrap();
        assert_eq!(loaded.selected_model, Some(WhisperModel::BaseEn));
        assert_eq!(
            loaded.selected_llm_model,
            Some(LlmModel::Qwen3_5_4B_Q4KM)
        );
    }

    #[test]
    fn settings_backward_compatible_without_llm_model() {
        let base = TempDir::new();
        // Simulate old settings.json without selected_llm_model field
        let old_json = r#"{"selected_model":"BaseEn"}"#;
        fs::write(base.path().join("settings.json"), old_json).unwrap();
        let loaded = load_settings_from(base.path()).unwrap();
        assert_eq!(loaded.selected_model, Some(WhisperModel::BaseEn));
        assert_eq!(loaded.selected_llm_model, None);
    }
}

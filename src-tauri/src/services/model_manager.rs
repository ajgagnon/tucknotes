use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

use crate::errors::AppError;
use crate::models::{AppSettings, DownloadProgress, LlmModel, Model, ModelInfo, WhisperModel};

/// Resolve the Tauri app data directory (e.g. ~/Library/Application Support/com.andre.tucknotes).
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

/// Check whether the given model's file exists on disk.
/// Works for any model type (Whisper, LLM).
pub fn is_downloaded_in<M: Model>(base_dir: &Path, model: &M) -> Result<bool, AppError> {
    Ok(base_dir.join("models").join(model.filename()).exists())
}

/// Resolve a model's file path from settings + models dir.
/// Returns `None` if no model is selected or the file doesn't exist on disk.
///
/// `getter` extracts the relevant model from `AppSettings` (e.g.
/// `|s| s.selected_model.clone()` for Whisper).
pub fn resolve_path<M: Model>(
    base_dir: &Path,
    getter: impl FnOnce(&AppSettings) -> Option<M>,
) -> Result<Option<PathBuf>, AppError> {
    let settings = load_settings_from(base_dir)?;
    let Some(model) = getter(&settings) else {
        return Ok(None);
    };
    let path = ensure_models_dir(base_dir)?.join(model.filename());
    Ok(path.exists().then_some(path))
}

/// Convenience wrapper: resolve selected Whisper model path.
pub fn resolve_whisper_path(base_dir: &Path) -> Result<Option<PathBuf>, AppError> {
    resolve_path(base_dir, |s| s.selected_model.clone())
}

/// Convenience wrapper: resolve selected LLM model path.
pub fn resolve_llm_path(base_dir: &Path) -> Result<Option<PathBuf>, AppError> {
    resolve_path(base_dir, |s| s.selected_llm_model.clone())
}

/// Remove a Whisper model file and any `.partial` download artifact from
/// `models/`. Clears `selected_model` in settings when it points at this model.
pub fn remove_whisper_downloaded_in(
    base_dir: &Path,
    model: &WhisperModel,
) -> Result<(), AppError> {
    let models_dir = ensure_models_dir(base_dir)?;
    let file_path = models_dir.join(model.filename());
    let partial_path = models_dir.join(format!("{}.partial", model.filename()));

    let file_existed = file_path.exists();
    let partial_existed = partial_path.exists();

    let mut settings = load_settings_from(base_dir)?;
    let had_selection = settings.selected_model.as_ref() == Some(model);
    if had_selection {
        settings.selected_model = None;
    }

    if file_existed {
        std::fs::remove_file(&file_path)?;
    }
    if partial_existed {
        std::fs::remove_file(&partial_path)?;
    }

    if !file_existed && !partial_existed && !had_selection {
        return Err(AppError::NotFound(format!(
            "Model {} is not on disk",
            model.id()
        )));
    }

    if had_selection {
        save_settings_to(base_dir, &settings)?;
    }
    Ok(())
}

/// Remove an LLM model file and any `.partial` download artifact from `models/`.
/// Clears `selected_llm_model` in settings when it points at this model.
pub fn remove_llm_downloaded_in(base_dir: &Path, model: &LlmModel) -> Result<(), AppError> {
    let models_dir = ensure_models_dir(base_dir)?;
    let file_path = models_dir.join(model.filename());
    let partial_path = models_dir.join(format!("{}.partial", model.filename()));

    let file_existed = file_path.exists();
    let partial_existed = partial_path.exists();

    let mut settings = load_settings_from(base_dir)?;
    let had_selection = settings.selected_llm_model.as_ref() == Some(model);
    if had_selection {
        settings.selected_llm_model = None;
    }

    if file_existed {
        std::fs::remove_file(&file_path)?;
    }
    if partial_existed {
        std::fs::remove_file(&partial_path)?;
    }

    if !file_existed && !partial_existed && !had_selection {
        return Err(AppError::NotFound(format!(
            "Model {} is not on disk",
            model.id()
        )));
    }

    if had_selection {
        save_settings_to(base_dir, &settings)?;
    }
    Ok(())
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

pub fn is_downloaded<M: Model>(app: &AppHandle, model: &M) -> Result<bool, AppError> {
    is_downloaded_in(&resolve_data_dir(app)?, model)
}

pub fn remove_whisper_model(app: &AppHandle, model: &WhisperModel) -> Result<(), AppError> {
    remove_whisper_downloaded_in(&resolve_data_dir(app)?, model)
}

pub fn remove_llm_model(app: &AppHandle, model: &LlmModel) -> Result<(), AppError> {
    remove_llm_downloaded_in(&resolve_data_dir(app)?, model)
}

/// Absolute path to the Whisper model file on disk, if it exists.
pub fn whisper_model_file_path(app: &AppHandle, model_id: &str) -> Result<Option<String>, AppError> {
    let model =
        WhisperModel::from_id(model_id).ok_or_else(|| AppError::InvalidModel(model_id.to_string()))?;
    let base_dir = resolve_data_dir(app)?;
    let path = ensure_models_dir(&base_dir)?.join(model.filename());
    Ok(path.exists().then(|| path.to_string_lossy().into_owned()))
}

/// Absolute path to the LLM model file on disk, if it exists.
pub fn llm_model_file_path(app: &AppHandle, model_id: &str) -> Result<Option<String>, AppError> {
    let model =
        LlmModel::from_id(model_id).ok_or_else(|| AppError::InvalidModel(model_id.to_string()))?;
    let base_dir = resolve_data_dir(app)?;
    let path = ensure_models_dir(&base_dir)?.join(model.filename());
    Ok(path.exists().then(|| path.to_string_lossy().into_owned()))
}

/// Download a model file from Hugging Face.
///
/// Works for any model type (Whisper, LLM). The download streams to a
/// `.partial` temp file first, then atomically renames it to the final
/// filename once complete. Emits `{prefix}:download-progress` events
/// (where prefix comes from `M::event_prefix()`).
pub async fn download<M: Model>(app: &AppHandle, model: &M) -> Result<(), AppError> {
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
    let event_name = format!("{}:download-progress", M::event_prefix());

    let mut file = tokio::fs::File::create(&partial_path).await?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;

        downloaded_bytes += chunk.len() as u64;

        // Fire-and-forget; if the frontend isn't listening, that's fine
        let _ = app.emit(
            &event_name,
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
pub fn list_whisper_models() -> Vec<ModelInfo<WhisperModel>> {
    vec![
        ModelInfo {
            id: WhisperModel::LargeV3TurboQ5,
            name: "Large v3 Turbo (Quantized)".into(),
            description: "Higher accuracy, multilingual support. Larger download.".into(),
            size_bytes: 547_000_000,
            filename: WhisperModel::LargeV3TurboQ5.filename().into(),
            recommended: Some(true),
        },
        ModelInfo {
            id: WhisperModel::BaseEn,
            name: "Base (English Only)".into(),
            description: "Smallest download — choose only when storage is limited.".into(),
            size_bytes: 148_000_000,
            filename: WhisperModel::BaseEn.filename().into(),
            recommended: None,
        },
    ]
}

/// Return the catalog of available LLM models.
pub fn list_llm_models() -> Vec<ModelInfo<LlmModel>> {
    vec![
        ModelInfo {
            id: LlmModel::Gemma4_E2B_Q8,
            name: "Gemma 4 E2B (Q8_0)".into(),
            description: "Google's Gemma 4 model, 8-bit quantized. Higher quality, larger download."
                .into(),
            size_bytes: 5_050_000_000,
            filename: LlmModel::Gemma4_E2B_Q8.filename().into(),
            recommended: Some(true),
        },
        ModelInfo {
            id: LlmModel::Qwen3_5_4B_Q4KM,
            name: "Qwen3.5 4B (Q4_K_M)".into(),
            description: "Compact 4-bit quantized model for meeting summarization.".into(),
            size_bytes: 2_740_000_000,
            filename: LlmModel::Qwen3_5_4B_Q4KM.filename().into(),
            recommended: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::AppError;
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
            let dir = std::env::temp_dir().join(format!("tucknotes_test_{}_{id}", std::process::id()));
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
    fn is_downloaded_false_when_absent() {
        let base = TempDir::new();
        assert!(!is_downloaded_in(base.path(), &WhisperModel::BaseEn).unwrap());
    }

    #[test]
    fn is_downloaded_true_when_present() {
        let base = TempDir::new();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        fs::write(models_dir.join(WhisperModel::BaseEn.filename()), b"fake").unwrap();
        assert!(is_downloaded_in(base.path(), &WhisperModel::BaseEn).unwrap());
        assert!(!is_downloaded_in(base.path(), &WhisperModel::LargeV3TurboQ5).unwrap());
    }

    #[test]
    fn resolve_whisper_path_none_when_no_settings() {
        let base = TempDir::new();
        assert!(resolve_whisper_path(base.path()).unwrap().is_none());
    }

    #[test]
    fn resolve_whisper_path_none_when_file_missing() {
        let base = TempDir::new();
        let settings = AppSettings {
            selected_model: Some(WhisperModel::BaseEn),
            ..Default::default()
        };
        save_settings_to(base.path(), &settings).unwrap();
        assert!(resolve_whisper_path(base.path()).unwrap().is_none());
    }

    #[test]
    fn resolve_whisper_path_returns_path_when_file_exists() {
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
        let path = resolve_whisper_path(base.path()).unwrap().unwrap();
        assert_eq!(
            path,
            models_dir.join(WhisperModel::LargeV3TurboQ5.filename())
        );
    }

    #[test]
    fn list_whisper_models_returns_both_models() {
        let models = list_whisper_models();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, WhisperModel::LargeV3TurboQ5);
        assert_eq!(models[1].id, WhisperModel::BaseEn);
        // The recommended model is listed first.
        assert_eq!(models[0].recommended, Some(true));
    }

    #[test]
    fn list_whisper_models_filenames_match_enum() {
        for m in &list_whisper_models() {
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
    fn is_llm_downloaded_false_when_absent() {
        let base = TempDir::new();
        assert!(!is_downloaded_in(base.path(), &LlmModel::Qwen3_5_4B_Q4KM).unwrap());
    }

    #[test]
    fn is_llm_downloaded_true_when_present() {
        let base = TempDir::new();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        fs::write(
            models_dir.join(LlmModel::Qwen3_5_4B_Q4KM.filename()),
            b"fake",
        )
        .unwrap();
        assert!(is_downloaded_in(base.path(), &LlmModel::Qwen3_5_4B_Q4KM).unwrap());
    }

    #[test]
    fn resolve_llm_path_none_when_no_settings() {
        let base = TempDir::new();
        assert!(resolve_llm_path(base.path()).unwrap().is_none());
    }

    #[test]
    fn resolve_llm_path_returns_path_when_file_exists() {
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
        let path = resolve_llm_path(base.path()).unwrap().unwrap();
        assert_eq!(
            path,
            models_dir.join(LlmModel::Qwen3_5_4B_Q4KM.filename())
        );
    }

    #[test]
    fn list_llm_models_returns_catalog() {
        let models = list_llm_models();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, LlmModel::Gemma4_E2B_Q8);
        assert_eq!(models[1].id, LlmModel::Qwen3_5_4B_Q4KM);
        // The recommended model is listed first.
        assert_eq!(models[0].recommended, Some(true));
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
            ..AppSettings::default()
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

    #[test]
    fn settings_backward_compatible_with_qwen3_llm_model_id() {
        let base = TempDir::new();
        // Simulate settings.json from the Qwen3 era
        let old_json = r#"{"selected_model":"BaseEn","selected_llm_model":"Qwen3_4B_Q4KM"}"#;
        fs::write(base.path().join("settings.json"), old_json).unwrap();
        let loaded = load_settings_from(base.path()).unwrap();
        assert_eq!(loaded.selected_model, Some(WhisperModel::BaseEn));
        assert_eq!(
            loaded.selected_llm_model,
            Some(LlmModel::Qwen3_5_4B_Q4KM)
        );
    }

    #[test]
    fn remove_whisper_downloaded_deletes_file() {
        let base = TempDir::new();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        fs::write(
            models_dir.join(WhisperModel::BaseEn.filename()),
            b"fake",
        )
        .unwrap();
        remove_whisper_downloaded_in(base.path(), &WhisperModel::BaseEn).unwrap();
        assert!(!is_downloaded_in(base.path(), &WhisperModel::BaseEn).unwrap());
    }

    #[test]
    fn remove_whisper_downloaded_clears_selected_model() {
        let base = TempDir::new();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        fs::write(
            models_dir.join(WhisperModel::LargeV3TurboQ5.filename()),
            b"fake",
        )
        .unwrap();
        let settings = AppSettings {
            selected_model: Some(WhisperModel::LargeV3TurboQ5),
            ..Default::default()
        };
        save_settings_to(base.path(), &settings).unwrap();
        remove_whisper_downloaded_in(base.path(), &WhisperModel::LargeV3TurboQ5).unwrap();
        let loaded = load_settings_from(base.path()).unwrap();
        assert_eq!(loaded.selected_model, None);
    }

    #[test]
    fn remove_whisper_downloaded_removes_partial() {
        let base = TempDir::new();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        let partial = models_dir.join(format!(
            "{}.partial",
            WhisperModel::BaseEn.filename()
        ));
        fs::write(&partial, b"partial").unwrap();
        remove_whisper_downloaded_in(base.path(), &WhisperModel::BaseEn).unwrap();
        assert!(!partial.exists());
    }

    #[test]
    fn remove_whisper_downloaded_errors_when_nothing_on_disk_or_settings() {
        let base = TempDir::new();
        let err = remove_whisper_downloaded_in(base.path(), &WhisperModel::BaseEn).unwrap_err();
        match err {
            AppError::NotFound(_) => {}
            _ => panic!("expected NotFound"),
        }
    }

    #[test]
    fn remove_llm_downloaded_deletes_file_and_clears_selection() {
        let base = TempDir::new();
        let models_dir = ensure_models_dir(base.path()).unwrap();
        fs::write(
            models_dir.join(LlmModel::Gemma4_E2B_Q8.filename()),
            b"fake",
        )
        .unwrap();
        let settings = AppSettings {
            selected_llm_model: Some(LlmModel::Gemma4_E2B_Q8),
            ..Default::default()
        };
        save_settings_to(base.path(), &settings).unwrap();
        remove_llm_downloaded_in(base.path(), &LlmModel::Gemma4_E2B_Q8).unwrap();
        let loaded = load_settings_from(base.path()).unwrap();
        assert_eq!(loaded.selected_llm_model, None);
        assert!(!is_downloaded_in(base.path(), &LlmModel::Gemma4_E2B_Q8).unwrap());
    }
}

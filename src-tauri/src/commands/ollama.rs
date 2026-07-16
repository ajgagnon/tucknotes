use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::models::{LlmProvider, OllamaSettings};
use crate::services::model_manager;
use crate::services::ollama::{self, OllamaModelInfo, OllamaStatus};

/// LLM engine choice as exposed to the frontend (Settings + onboarding).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmEngineSettings {
    pub provider: LlmProvider,
    pub ollama_base_url: String,
    pub ollama_model: Option<String>,
}

/// Normalize and validate a user-entered base URL.
fn normalize_base_url(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::ConfigError(
            "Ollama base URL can't be empty".to_string(),
        ));
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err(AppError::ConfigError(
            "Ollama base URL must start with http:// or https://".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn saved_base_url(app: &tauri::AppHandle) -> String {
    model_manager::load_settings(app)
        .map(|s| s.ollama.base_url)
        .unwrap_or_else(|_| ollama::DEFAULT_BASE_URL.to_string())
}

/// Probe an Ollama server. With no `base_url` the saved one is probed —
/// onboarding and the settings panel pass an explicit URL to test unsaved
/// input ("Test connection").
#[tauri::command]
pub async fn detect_ollama(app: tauri::AppHandle, base_url: Option<String>) -> OllamaStatus {
    let url = base_url.unwrap_or_else(|| saved_base_url(&app));
    ollama::detect(&url).await
}

#[tauri::command]
pub async fn list_ollama_models(
    app: tauri::AppHandle,
    base_url: Option<String>,
) -> Result<Vec<OllamaModelInfo>, AppError> {
    let url = base_url.unwrap_or_else(|| saved_base_url(&app));
    ollama::list_models(&url).await
}

#[tauri::command]
pub fn get_llm_engine_settings(app: tauri::AppHandle) -> Result<LlmEngineSettings, AppError> {
    let settings = model_manager::load_settings(&app)?;
    Ok(LlmEngineSettings {
        provider: settings.llm_provider,
        ollama_base_url: settings.ollama.base_url,
        ollama_model: settings.ollama.model,
    })
}

#[tauri::command]
pub fn set_llm_engine_settings(
    app: tauri::AppHandle,
    engine: LlmEngineSettings,
) -> Result<(), AppError> {
    let base_url = normalize_base_url(&engine.ollama_base_url)?;
    let mut settings = model_manager::load_settings(&app)?;
    settings.llm_provider = engine.provider;
    settings.ollama = OllamaSettings {
        base_url,
        model: engine
            .ollama_model
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty()),
    };
    model_manager::save_settings(&app, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_base_url_strips_trailing_slash_and_whitespace() {
        assert_eq!(
            normalize_base_url(" http://localhost:11434/ ").unwrap(),
            "http://localhost:11434"
        );
    }

    #[test]
    fn normalize_base_url_rejects_empty_and_schemeless() {
        assert!(normalize_base_url("   ").is_err());
        assert!(normalize_base_url("localhost:11434").is_err());
        assert!(normalize_base_url("https://my-host:11434").is_ok());
    }
}

use serde::{Deserialize, Serialize};

use super::llm::LlmModel;
use super::Model;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WhisperModel {
    BaseEn,
    LargeV3TurboQ5,
}

impl Model for WhisperModel {
    fn id(&self) -> &'static str {
        match self {
            WhisperModel::BaseEn => "BaseEn",
            WhisperModel::LargeV3TurboQ5 => "LargeV3TurboQ5",
        }
    }

    fn filename(&self) -> &'static str {
        match self {
            WhisperModel::BaseEn => "ggml-base.en.bin",
            WhisperModel::LargeV3TurboQ5 => "ggml-large-v3-turbo-q5_0.bin",
        }
    }

    fn download_url(&self) -> &'static str {
        match self {
            WhisperModel::BaseEn => {
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
            }
            WhisperModel::LargeV3TurboQ5 => {
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin"
            }
        }
    }

    fn from_id(id: &str) -> Option<WhisperModel> {
        match id {
            "BaseEn" => Some(WhisperModel::BaseEn),
            "LargeV3TurboQ5" => Some(WhisperModel::LargeV3TurboQ5),
            _ => None,
        }
    }

    fn event_prefix() -> &'static str {
        "model"
    }
}

#[derive(Clone, Serialize)]
pub struct ModelInfo<M: Model> {
    pub id: M,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<bool>,
}

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

/// Which engine powers summarization and chat: the built-in model downloaded
/// by the app (run in-process via llama.cpp) or a user-managed local Ollama
/// server.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    #[default]
    BuiltIn,
    Ollama,
}

pub fn default_ollama_base_url() -> String {
    "http://localhost:11434".to_string()
}

/// Connection details for a user-managed Ollama server. Only consulted when
/// `AppSettings::llm_provider` is `Ollama`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaSettings {
    #[serde(default = "default_ollama_base_url")]
    pub base_url: String,
    /// Name of the Ollama model to run (e.g. "qwen3:4b"). `None` until the
    /// user picks one.
    #[serde(default)]
    pub model: Option<String>,
}

impl Default for OllamaSettings {
    fn default() -> Self {
        OllamaSettings {
            base_url: default_ollama_base_url(),
            model: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AppSettings {
    pub selected_model: Option<WhisperModel>,
    pub selected_llm_model: Option<LlmModel>,
    /// App-wide default summary template id (`None` = the Default template).
    /// `#[serde(default)]` keeps older `settings.json` files (without this
    /// field) loading.
    #[serde(default)]
    pub default_template: Option<String>,
    /// Whether the user has acknowledged they are responsible for recording
    /// legally (obtaining any required consent from participants). Set once
    /// during onboarding. `#[serde(default)]` keeps older `settings.json` files
    /// loading (as `false`).
    #[serde(default)]
    pub recording_consent_acknowledged: bool,
    /// Whether to generate live meeting minutes during recording. Defaults to
    /// on; the feature silently no-ops when no LLM model is downloaded.
    #[serde(default = "default_true")]
    pub live_minutes_enabled: bool,
    /// Which LLM engine to use. `#[serde(default)]` keeps older settings.json
    /// files (without this field) loading as `BuiltIn`.
    #[serde(default)]
    pub llm_provider: LlmProvider,
    /// Ollama connection details, kept even while `llm_provider` is `BuiltIn`
    /// so switching providers loses nothing.
    #[serde(default)]
    pub ollama: OllamaSettings,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            selected_model: None,
            selected_llm_model: None,
            default_template: None,
            recording_consent_acknowledged: false,
            live_minutes_enabled: true,
            llm_provider: LlmProvider::default(),
            ollama: OllamaSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_model_from_id_valid() {
        assert_eq!(WhisperModel::from_id("BaseEn"), Some(WhisperModel::BaseEn));
        assert_eq!(
            WhisperModel::from_id("LargeV3TurboQ5"),
            Some(WhisperModel::LargeV3TurboQ5)
        );
    }

    #[test]
    fn whisper_model_from_id_invalid() {
        assert_eq!(WhisperModel::from_id("nonexistent"), None);
    }

    #[test]
    fn whisper_model_filenames() {
        assert_eq!(WhisperModel::BaseEn.filename(), "ggml-base.en.bin");
        assert_eq!(
            WhisperModel::LargeV3TurboQ5.filename(),
            "ggml-large-v3-turbo-q5_0.bin"
        );
    }

    #[test]
    fn whisper_model_id_matches_from_id() {
        assert_eq!(WhisperModel::from_id(WhisperModel::BaseEn.id()), Some(WhisperModel::BaseEn));
        assert_eq!(
            WhisperModel::from_id(WhisperModel::LargeV3TurboQ5.id()),
            Some(WhisperModel::LargeV3TurboQ5)
        );
    }

    #[test]
    fn app_settings_default_template_is_optional() {
        // An older settings.json without `default_template` still loads.
        let legacy = r#"{"selected_model":null,"selected_llm_model":null}"#;
        let parsed: AppSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.default_template, None);

        // Default is None.
        assert_eq!(AppSettings::default().default_template, None);

        // Round-trips when set.
        let settings = AppSettings {
            default_template: Some("minutes".to_string()),
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.default_template.as_deref(), Some("minutes"));
    }

    #[test]
    fn app_settings_live_minutes_defaults_on() {
        // Older settings.json without `live_minutes_enabled` loads as `true`.
        let legacy = r#"{"selected_model":null,"selected_llm_model":null}"#;
        let parsed: AppSettings = serde_json::from_str(legacy).unwrap();
        assert!(parsed.live_minutes_enabled);

        assert!(AppSettings::default().live_minutes_enabled);

        // An explicit `false` round-trips.
        let settings = AppSettings {
            live_minutes_enabled: false,
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert!(!back.live_minutes_enabled);
    }

    #[test]
    fn app_settings_llm_provider_defaults_built_in() {
        // An older settings.json without provider fields loads as BuiltIn with
        // default Ollama connection details.
        let legacy = r#"{"selected_model":null,"selected_llm_model":null}"#;
        let parsed: AppSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.llm_provider, LlmProvider::BuiltIn);
        assert_eq!(parsed.ollama, OllamaSettings::default());
        assert_eq!(parsed.ollama.base_url, "http://localhost:11434");

        assert_eq!(AppSettings::default().llm_provider, LlmProvider::BuiltIn);

        // Round-trips when set.
        let settings = AppSettings {
            llm_provider: LlmProvider::Ollama,
            ollama: OllamaSettings {
                base_url: "http://192.168.1.5:11434".to_string(),
                model: Some("qwen3:4b".to_string()),
            },
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.llm_provider, LlmProvider::Ollama);
        assert_eq!(back.ollama.base_url, "http://192.168.1.5:11434");
        assert_eq!(back.ollama.model.as_deref(), Some("qwen3:4b"));
    }

    #[test]
    fn llm_provider_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&LlmProvider::BuiltIn).unwrap(),
            r#""built_in""#
        );
        assert_eq!(
            serde_json::to_string(&LlmProvider::Ollama).unwrap(),
            r#""ollama""#
        );
    }

    #[test]
    fn ollama_settings_partial_json_gets_default_base_url() {
        // `{"ollama":{}}` (e.g. hand-edited settings) still yields a usable
        // base URL.
        let json = r#"{"selected_model":null,"selected_llm_model":null,"ollama":{}}"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.ollama.base_url, "http://localhost:11434");
        assert_eq!(parsed.ollama.model, None);
    }

    #[test]
    fn app_settings_recording_consent_is_optional() {
        // An older settings.json without `recording_consent_acknowledged` loads
        // as `false`.
        let legacy = r#"{"selected_model":null,"selected_llm_model":null}"#;
        let parsed: AppSettings = serde_json::from_str(legacy).unwrap();
        assert!(!parsed.recording_consent_acknowledged);

        // Default is false.
        assert!(!AppSettings::default().recording_consent_acknowledged);

        // Round-trips when set.
        let settings = AppSettings {
            recording_consent_acknowledged: true,
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert!(back.recording_consent_acknowledged);
    }
}

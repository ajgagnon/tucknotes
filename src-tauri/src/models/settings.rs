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

#[derive(Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub selected_model: Option<WhisperModel>,
    #[serde(default, deserialize_with = "deserialize_optional_llm_model")]
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
}

/// Deserialize `selected_llm_model` tolerantly: an id for a model that has been
/// removed from the catalog (e.g. after swapping the default model) maps to
/// `None` rather than failing the entire settings load.
fn deserialize_optional_llm_model<'de, D>(deserializer: D) -> Result<Option<LlmModel>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.as_deref().and_then(LlmModel::from_id))
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

    #[test]
    fn app_settings_unknown_llm_model_deserializes_to_none() {
        // An id for a model removed from the catalog must not break settings load.
        let json = r#"{"selected_model":null,"selected_llm_model":"Lfm2_5_8B_A1B_Q4KM"}"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.selected_llm_model, None);
    }

    #[test]
    fn app_settings_known_llm_model_still_deserializes() {
        let json = r#"{"selected_model":null,"selected_llm_model":"Gemma4_E2B_Q8"}"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.selected_llm_model, Some(LlmModel::Gemma4_E2B_Q8));
    }
}

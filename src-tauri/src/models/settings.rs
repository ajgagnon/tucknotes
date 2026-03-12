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
    pub selected_llm_model: Option<LlmModel>,
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
}

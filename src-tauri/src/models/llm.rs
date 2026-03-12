use serde::{Deserialize, Serialize};

use super::Model;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(non_camel_case_types)]
pub enum LlmModel {
    #[serde(alias = "Qwen3_5_4B_Q4KM")]
    Qwen3_4B_Q4KM,
}

impl Model for LlmModel {
    fn id(&self) -> &'static str {
        match self {
            LlmModel::Qwen3_4B_Q4KM => "Qwen3_4B_Q4KM",
        }
    }

    fn filename(&self) -> &'static str {
        match self {
            LlmModel::Qwen3_4B_Q4KM => "Qwen3-4B-Q4_K_M.gguf",
        }
    }

    fn download_url(&self) -> &'static str {
        match self {
            LlmModel::Qwen3_4B_Q4KM => {
                "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf"
            }
        }
    }

    fn from_id(id: &str) -> Option<LlmModel> {
        match id {
            "Qwen3_4B_Q4KM" | "Qwen3_5_4B_Q4KM" => Some(LlmModel::Qwen3_4B_Q4KM),
            _ => None,
        }
    }

    fn event_prefix() -> &'static str {
        "llm-model"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_model_from_id_valid() {
        assert_eq!(
            LlmModel::from_id("Qwen3_4B_Q4KM"),
            Some(LlmModel::Qwen3_4B_Q4KM)
        );
    }

    #[test]
    fn llm_model_from_id_old_35_id_backward_compat() {
        assert_eq!(
            LlmModel::from_id("Qwen3_5_4B_Q4KM"),
            Some(LlmModel::Qwen3_4B_Q4KM)
        );
    }

    #[test]
    fn llm_model_from_id_invalid() {
        assert_eq!(LlmModel::from_id("nonexistent"), None);
    }

    #[test]
    fn llm_model_filename() {
        assert_eq!(
            LlmModel::Qwen3_4B_Q4KM.filename(),
            "Qwen3-4B-Q4_K_M.gguf"
        );
    }

    #[test]
    fn llm_model_id_matches_from_id() {
        assert_eq!(
            LlmModel::from_id(LlmModel::Qwen3_4B_Q4KM.id()),
            Some(LlmModel::Qwen3_4B_Q4KM)
        );
    }

    #[test]
    fn llm_model_serde_deserializes_old_35_variant_name() {
        let json = r#""Qwen3_5_4B_Q4KM""#;
        let model: LlmModel = serde_json::from_str(json).unwrap();
        assert_eq!(model, LlmModel::Qwen3_4B_Q4KM);
    }

    #[test]
    fn llm_model_serde_serializes_current_variant_name() {
        let json = serde_json::to_string(&LlmModel::Qwen3_4B_Q4KM).unwrap();
        assert_eq!(json, r#""Qwen3_4B_Q4KM""#);
    }
}

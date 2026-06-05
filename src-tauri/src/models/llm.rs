use serde::{Deserialize, Serialize};

use super::Model;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(non_camel_case_types)]
pub enum LlmModel {
    #[serde(alias = "Qwen3_4B_Q4KM")]
    Qwen3_5_4B_Q4KM,
    Gemma4_E2B_Q8,
    Lfm2_2_6B_Q8,
}

impl Model for LlmModel {
    fn id(&self) -> &'static str {
        match self {
            LlmModel::Qwen3_5_4B_Q4KM => "Qwen3_5_4B_Q4KM",
            LlmModel::Gemma4_E2B_Q8 => "Gemma4_E2B_Q8",
            LlmModel::Lfm2_2_6B_Q8 => "Lfm2_2_6B_Q8",
        }
    }

    fn filename(&self) -> &'static str {
        match self {
            LlmModel::Qwen3_5_4B_Q4KM => "Qwen3.5-4B-Q4_K_M.gguf",
            LlmModel::Gemma4_E2B_Q8 => "gemma-4-E2B-it-Q8_0.gguf",
            LlmModel::Lfm2_2_6B_Q8 => "LFM2-2.6B-Q8_0.gguf",
        }
    }

    fn download_url(&self) -> &'static str {
        match self {
            LlmModel::Qwen3_5_4B_Q4KM => {
                "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf"
            }
            LlmModel::Gemma4_E2B_Q8 => {
                "https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q8_0.gguf"
            }
            LlmModel::Lfm2_2_6B_Q8 => {
                "https://huggingface.co/LiquidAI/LFM2-2.6B-GGUF/resolve/main/LFM2-2.6B-Q8_0.gguf"
            }
        }
    }

    fn from_id(id: &str) -> Option<LlmModel> {
        match id {
            "Qwen3_5_4B_Q4KM" | "Qwen3_4B_Q4KM" => Some(LlmModel::Qwen3_5_4B_Q4KM),
            "Gemma4_E2B_Q8" => Some(LlmModel::Gemma4_E2B_Q8),
            "Lfm2_2_6B_Q8" => Some(LlmModel::Lfm2_2_6B_Q8),
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
            LlmModel::from_id("Qwen3_5_4B_Q4KM"),
            Some(LlmModel::Qwen3_5_4B_Q4KM)
        );
    }

    #[test]
    fn llm_model_from_id_old_qwen3_id_backward_compat() {
        assert_eq!(
            LlmModel::from_id("Qwen3_4B_Q4KM"),
            Some(LlmModel::Qwen3_5_4B_Q4KM)
        );
    }

    #[test]
    fn llm_model_from_id_invalid() {
        assert_eq!(LlmModel::from_id("nonexistent"), None);
    }

    #[test]
    fn llm_model_filename() {
        assert_eq!(
            LlmModel::Qwen3_5_4B_Q4KM.filename(),
            "Qwen3.5-4B-Q4_K_M.gguf"
        );
    }

    #[test]
    fn llm_model_id_matches_from_id() {
        assert_eq!(
            LlmModel::from_id(LlmModel::Qwen3_5_4B_Q4KM.id()),
            Some(LlmModel::Qwen3_5_4B_Q4KM)
        );
        assert_eq!(
            LlmModel::from_id(LlmModel::Gemma4_E2B_Q8.id()),
            Some(LlmModel::Gemma4_E2B_Q8)
        );
        assert_eq!(
            LlmModel::from_id(LlmModel::Lfm2_2_6B_Q8.id()),
            Some(LlmModel::Lfm2_2_6B_Q8)
        );
    }

    #[test]
    fn llm_model_gemma4_from_id() {
        assert_eq!(
            LlmModel::from_id("Gemma4_E2B_Q8"),
            Some(LlmModel::Gemma4_E2B_Q8)
        );
    }

    #[test]
    fn llm_model_gemma4_filename() {
        assert_eq!(
            LlmModel::Gemma4_E2B_Q8.filename(),
            "gemma-4-E2B-it-Q8_0.gguf"
        );
    }

    #[test]
    fn llm_model_lfm2_from_id() {
        assert_eq!(
            LlmModel::from_id("Lfm2_2_6B_Q8"),
            Some(LlmModel::Lfm2_2_6B_Q8)
        );
    }

    #[test]
    fn llm_model_lfm2_filename() {
        assert_eq!(
            LlmModel::Lfm2_2_6B_Q8.filename(),
            "LFM2-2.6B-Q8_0.gguf"
        );
    }

    #[test]
    fn llm_model_serde_deserializes_old_qwen3_variant_name() {
        let json = r#""Qwen3_4B_Q4KM""#;
        let model: LlmModel = serde_json::from_str(json).unwrap();
        assert_eq!(model, LlmModel::Qwen3_5_4B_Q4KM);
    }

    #[test]
    fn llm_model_serde_serializes_current_variant_name() {
        let json = serde_json::to_string(&LlmModel::Qwen3_5_4B_Q4KM).unwrap();
        assert_eq!(json, r#""Qwen3_5_4B_Q4KM""#);
    }
}

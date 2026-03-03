use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(non_camel_case_types)]
pub enum LlmModel {
    #[serde(alias = "Qwen3_4B_Q4KM")]
    Qwen3_5_4B_Q4KM,
}

impl LlmModel {
    pub fn id(&self) -> &'static str {
        match self {
            LlmModel::Qwen3_5_4B_Q4KM => "Qwen3_5_4B_Q4KM",
        }
    }

    pub fn filename(&self) -> &'static str {
        match self {
            LlmModel::Qwen3_5_4B_Q4KM => "Qwen3.5-4B-Q4_K_M.gguf",
        }
    }

    pub fn download_url(&self) -> &'static str {
        match self {
            LlmModel::Qwen3_5_4B_Q4KM => {
                "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf"
            }
        }
    }

    /// Resolve a model ID string (e.g. from a Tauri command) to an enum variant.
    /// Accepts the old `"Qwen3_4B_Q4KM"` ID as a defensive fallback;
    /// settings.json migration is handled separately by `#[serde(alias)]`.
    pub fn from_id(id: &str) -> Option<LlmModel> {
        match id {
            "Qwen3_5_4B_Q4KM" | "Qwen3_4B_Q4KM" => Some(LlmModel::Qwen3_5_4B_Q4KM),
            _ => None,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct LlmModelInfo {
    pub id: LlmModel,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub filename: String,
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
    fn llm_model_from_id_old_id_backward_compat() {
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
    }

    #[test]
    fn llm_model_serde_deserializes_old_variant_name() {
        let json = r#""Qwen3_4B_Q4KM""#;
        let model: LlmModel = serde_json::from_str(json).unwrap();
        assert_eq!(model, LlmModel::Qwen3_5_4B_Q4KM);
    }

    #[test]
    fn llm_model_serde_serializes_new_variant_name() {
        let json = serde_json::to_string(&LlmModel::Qwen3_5_4B_Q4KM).unwrap();
        assert_eq!(json, r#""Qwen3_5_4B_Q4KM""#);
    }
}

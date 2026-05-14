use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LLMSlot {
    pub role: String,               // "regime", "intern", "trader"
    pub prompt: String,             // slot prompt body
    pub model_requirement: String,  // e.g., "anthropic.claude-sonnet-4.6+"
    pub allowed_tools: Vec<String>, // tool names from registry

    /// Optional explicit provider configured for this slot. This is the
    /// user-facing counterpart to the previous free-form
    /// `model_requirement` string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Optional explicit model id configured for this slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl LLMSlot {
    /// Prefer the explicit `model` field for runtime/model selection, then
    /// fall back to legacy `model_requirement` (string constraint) for
    /// backwards compatibility.
    pub fn effective_model(&self) -> String {
        self.model
            .as_ref()
            .filter(|m| !m.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| self.model_requirement.clone())
    }

    /// Canonical label for API/UI surfaces.
    pub fn provider_model_label(&self) -> String {
        let model = self.model.as_deref().unwrap_or("(not set)");
        let provider = self.provider.as_deref().unwrap_or("default");
        format!("{provider}:{model}")
    }
}

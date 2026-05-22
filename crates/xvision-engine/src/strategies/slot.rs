use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LLMSlot {
    pub role: String, // "regime", "intern", "trader"
    /// Informational attestation: the model id this slot was last
    /// published / tested with (e.g. "anthropic.claude-sonnet-4.6").
    /// Never gates eval-launch — the operator's `provider` + `model`
    /// binding is authoritative.
    pub attested_with: String,
    pub allowed_tools: Vec<String>, // tool names from registry

    /// Optional explicit provider configured for this slot. This is the
    /// user-facing counterpart to the previous free-form attestation
    /// string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Optional explicit model id configured for this slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl LLMSlot {
    /// Prefer the explicit `model` field for runtime/model selection, then
    /// fall back to the `attested_with` value when no binding has been
    /// configured yet. The fallback is a usability convenience for
    /// templates that were authored before the explicit `model` field
    /// existed; `attested_with` is **not** a gate — the operator may
    /// override the binding freely without touching attestation.
    pub fn effective_model(&self) -> String {
        self.model
            .as_ref()
            .filter(|m| !m.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| self.attested_with.clone())
    }

    /// Canonical label for API/UI surfaces.
    pub fn provider_model_label(&self) -> String {
        let model = self.model.as_deref().unwrap_or("(not set)");
        let provider = self.provider.as_deref().unwrap_or("default");
        format!("{provider}:{model}")
    }
}

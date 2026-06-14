use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LLMSlot {
    pub role: String, // "regime", "trader"
    /// Informational attestation: the model id this slot was last
    /// published / tested with (e.g. "anthropic.claude-sonnet-4.6").
    /// **Provenance only — never the operational binding.** The operator's
    /// `provider` + `model` fields are authoritative; `attested_with` does
    /// not gate eval-launch and is never used to select the model that
    /// actually dispatches (see F31 and [`LLMSlot::effective_model`]).
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
    /// The operator's explicit `model` binding for this slot, or an empty
    /// string when no model has been configured (the slot is *unbound*).
    ///
    /// F31 (2026-06-04): this used to fall back to `attested_with` when
    /// `model` was unset. But `attested_with` is *provenance metadata*
    /// ("last tested with"), not a binding — that fallback silently promoted
    /// a provenance string to the operational model. A model-less legacy
    /// `trader_slot` whose `attested_with` was `"anthropic.claude-sonnet-4.6"`
    /// would therefore dispatch to anthropic even on a node where the operator
    /// never chose anthropic (the root mechanism behind F30: a strategy
    /// "designed for openrouter" silently became anthropic). Provenance must
    /// never become the binding, so an unset `model` now resolves to empty —
    /// the slot is unbound and fails fast at dispatch / is caught by the
    /// optimizer preflight, rather than masquerading as whatever was attested.
    pub fn effective_model(&self) -> String {
        self.model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .unwrap_or_default()
    }

    /// Whether this slot carries an explicit, non-empty `model` binding.
    /// F31: a slot with no binding is *unbound* — it must not silently derive
    /// a model from `attested_with` provenance.
    pub fn has_model_binding(&self) -> bool {
        self.model
            .as_deref()
            .map(str::trim)
            .is_some_and(|m| !m.is_empty())
    }

    /// Canonical label for API/UI surfaces.
    pub fn provider_model_label(&self) -> String {
        let model = self.model.as_deref().unwrap_or("(not set)");
        let provider = self.provider.as_deref().unwrap_or("default");
        format!("{provider}:{model}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(model: Option<&str>, attested: &str) -> LLMSlot {
        LLMSlot {
            role: "trader".into(),
            attested_with: attested.into(),
            allowed_tools: vec![],
            provider: None,
            model: model.map(str::to_string),
        }
    }

    #[test]
    fn effective_model_returns_explicit_binding() {
        let s = slot(Some("deepseek/deepseek-v4-flash"), "anthropic.claude-sonnet-4.6");
        // Explicit model wins; provenance is ignored even when present.
        assert_eq!(s.effective_model(), "deepseek/deepseek-v4-flash");
        assert!(s.has_model_binding());
    }

    #[test]
    fn effective_model_never_promotes_attested_provenance() {
        // F31 regression: a model-less legacy slot must NOT become its
        // `attested_with` provenance string. This is the exact masquerade that
        // turned a model-less seeded example into an anthropic binding.
        let s = slot(None, "anthropic.claude-sonnet-4.6");
        assert_eq!(
            s.effective_model(),
            "",
            "unbound slot must resolve to empty, not the attested provenance"
        );
        assert!(!s.has_model_binding());
    }

    #[test]
    fn effective_model_treats_blank_model_as_unbound() {
        let s = slot(Some("   "), "anthropic.claude-sonnet-4.6");
        assert_eq!(s.effective_model(), "");
        assert!(!s.has_model_binding());
    }
}

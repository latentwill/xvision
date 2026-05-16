//! Agent + AgentSlot value types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use xvision_core::providers::{lookup_model, ModelMetadata};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub slots: Vec<AgentSlot>,
    pub archived: bool,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub updated_at: DateTime<Utc>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentSlot {
    pub name: String,
    pub provider: String,
    pub model: String,
    pub system_prompt: String,
    /// Forward-compat hook for the v1.1 skill registry (see
    /// `docs/superpowers/plans/2026-05-11-agents-page-v1.md` §Skills).
    /// Each entry is a ULID into the workspace skill registry; entries
    /// of `kind = tool | prompt_fragment | evaluator` compose onto this
    /// slot at runtime. The picker is hidden in v1 until `/agents/skills`
    /// ships — but the field is persisted so existing agents survive the
    /// registry landing without a schema migration. Not related to the
    /// Plan 2b `xvn skill` surface that was removed in ADR 0012.
    pub skill_ids: Vec<String>,
    /// Optional operator override for the per-request token budget.
    /// `None` means "auto from the selected model" — the dispatcher
    /// resolves it via `agents::model_metadata::resolve_max_tokens`.
    /// `Some(n)` is honored and clamped to the model's
    /// `output_token_ceiling`.
    ///
    /// Stored in SQLite as a non-null integer with `0` as the sentinel
    /// for `None`; the store layer maps between the sentinel and the
    /// Rust-side `Option`.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub max_tokens: Option<u32>,
}

impl AgentSlot {
    /// Canonical model metadata lookup for this slot's model id. Falls
    /// back to `ModelMetadata::unknown_default` when the id isn't in the
    /// canonical table.
    pub fn model_metadata(&self) -> ModelMetadata {
        lookup_model(&self.model)
    }

    /// Resolve the operator's `max_tokens` to the wire-level
    /// `Option<u32>` the dispatcher hands to the provider.
    ///
    /// - `None` (or the SQLite storage sentinel `Some(0)`) → `None`.
    ///   Each dispatcher decides what to do with `None`: OpenAI-compat
    ///   omits the field entirely so the provider applies its own
    ///   default; Anthropic falls back to the per-model auto value
    ///   because the API requires the field.
    /// - `Some(n > 0)` passes through verbatim. No clamping — the
    ///   operator's intent wins. (The earlier q15 design clamped to the
    ///   model's `output_token_ceiling`, but that silently collapsed
    ///   operator values to 4096 for any model id missing from the
    ///   canonical metadata table.)
    pub fn resolve_max_tokens(&self) -> Option<u32> {
        match self.max_tokens {
            Some(n) if n > 0 => Some(n),
            _ => None,
        }
    }
}

impl Agent {
    /// Construct a single-slot agent with sensible defaults — the default
    /// shape produced by `+ New agent` on `/agents/new`.
    pub fn single_slot_default(
        agent_id: impl Into<String>,
        name: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            agent_id: agent_id.into(),
            name: name.into(),
            description: String::new(),
            tags: Vec::new(),
            slots: vec![AgentSlot {
                name: "main".to_string(),
                provider: provider.into(),
                model: model.into(),
                system_prompt: String::new(),
                skill_ids: Vec::new(),
                max_tokens: None,
            }],
            archived: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_slot_default_has_one_slot_named_main() {
        let a = Agent::single_slot_default(
            "01HZ000000000000000000000",
            "demo",
            "anthropic",
            "claude-sonnet-4-6",
        );
        assert_eq!(a.slots.len(), 1);
        assert_eq!(a.slots[0].name, "main");
        // New slots default to "auto from model"; the dispatcher resolves
        // this from the model's metadata at request time.
        assert_eq!(a.slots[0].max_tokens, None);
        assert!(a.slots[0].system_prompt.is_empty());
        assert!(!a.archived);
        assert!(a.tags.is_empty());
    }
}

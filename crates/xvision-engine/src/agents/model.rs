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
    /// Server-computed content digest of `system_prompt`. Format is the
    /// lowercase 16-char prefix of `sha256(system_prompt)` — short
    /// enough to eyeball in the agents UI, long enough to make
    /// accidental collision implausible.
    ///
    /// The field is `#[serde(default)]` so clients may omit it on POST
    /// /PUT bodies; any value the client sends is silently overridden
    /// at persist time by `AgentStore::insert_slot`. On the read path
    /// the server always returns the persisted value (the empty string
    /// only appears on rows persisted before migration 019; the next
    /// save through the store backfills it).
    ///
    /// Wired into observability via `SpanAttributes.prompt_version`
    /// (the field already exists; the strategy-assembly hop that
    /// threads it from `AgentSlot` → `LLMSlot` → span emission ships
    /// in a sibling follow-up so this PR stays foundation-only).
    ///
    /// See harness audit F-3 (`team/intake/2026-05-18-harness-observability-audit.md`).
    #[serde(default)]
    pub prompt_version: String,
}

impl AgentSlot {
    /// Compute the content digest for a slot's `system_prompt`. Returns
    /// the lowercase 16-hex-char prefix of `sha256(system_prompt)`.
    ///
    /// Sibling of `crates/xvision-engine/src/agent/observability.rs::compute_prompt_hash`
    /// which hashes the assembled wire prompt at dispatch time. The
    /// two digests intentionally differ in input (this one only sees
    /// the slot template, not the live messages/tools); the
    /// `sha256:` prefix difference makes the two visually
    /// distinguishable in traces.
    pub fn compute_prompt_version(system_prompt: &str) -> String {
        use sha2::{Digest, Sha256};
        let hex = format!("{:x}", Sha256::digest(system_prompt.as_bytes()));
        hex[..16].to_string()
    }

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
            prompt_version: String::new(),
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

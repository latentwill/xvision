//! Agent + AgentSlot value types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use xvision_core::providers::{lookup_model, ModelMetadata};

/// How the eval executor sanitizes the seed JSON before handing it to
/// the trader LLM. Persisted as the `inputs_policy` column on
/// `agent_slots` (migration 020). See harness audit F-6
/// (`team/intake/2026-05-19-eval-traces-end-to-end-audit.md`).
///
/// - `Raw` — today's behavior. `decision_index` lives on the top-level
///   seed; every `bar_history` entry carries `timestamp`. This is the
///   migration default, so existing rows are unaffected. The
///   regression-guard unit test in `eval::executor::paper` pins this
///   shape byte-for-byte.
/// - `Causal` — drop `decision_index` from the top-level seed and
///   replace each `bar_history` entry's `timestamp` field with
///   `bar_index` (0 = oldest visible bar in the `bar_history` slice).
///   The current bar still carries its OHLCV — only the wall-clock
///   label is hidden, which matches the v4 causal prompts.
/// - `Oracle` — behaves identically to `Raw` at runtime. The tag
///   exists so downstream consumers can mark a slot as deliberately
///   oracle-style (the two seeded oracle agents are documented in the
///   F-6 audit). It's distinct from `Raw` so the UI / cohort tagging
///   can tell "left at default" apart from "deliberately full
///   visibility."
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InputsPolicy {
    /// Default — preserve the raw seed shape.
    #[default]
    Raw,
    /// Strip `timestamp` (per bar) and `decision_index` (top-level).
    Causal,
    /// Tag-only; runtime behavior matches `Raw`.
    Oracle,
}

impl InputsPolicy {
    /// Wire representation persisted in the `agent_slots.inputs_policy`
    /// column. Stable — downstream consumers parse this verbatim.
    pub fn as_str(&self) -> &'static str {
        match self {
            InputsPolicy::Raw => "raw",
            InputsPolicy::Causal => "causal",
            InputsPolicy::Oracle => "oracle",
        }
    }

    /// Parse a value from the DB column. Unrecognised strings fall back
    /// to `Raw` so a future column-value typo can't crash the store
    /// reader on every read — the operator just sees the safe default.
    pub fn parse_or_raw(s: &str) -> Self {
        match s {
            "causal" => InputsPolicy::Causal,
            "oracle" => InputsPolicy::Oracle,
            _ => InputsPolicy::Raw,
        }
    }
}

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
    /// Optional operator override for the sampling temperature
    /// forwarded to the provider. `None` lets the provider's default
    /// apply (Anthropic ~1.0, OpenAI 1.0). `Some(t)` is passed through
    /// verbatim — no clamping. Eval-baseline operators set a low
    /// value (e.g. 0.2) when they want reproducible decisions; agent-
    /// loop operators leave it unset.
    ///
    /// Not yet persisted to SQLite (a follow-up migration will add the
    /// column). For now the field round-trips through JSON via
    /// `#[serde(default)]`; rows loaded from the store always come
    /// back as `None`, so existing seeded agents see the provider
    /// default until they're re-saved with an explicit value.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub temperature: Option<f64>,
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
    /// How the eval executor sanitizes the seed JSON before the trader
    /// LLM sees it. Persisted on the `agent_slots.inputs_policy`
    /// column (migration 020). Defaults to `Raw` so existing rows and
    /// clients that omit the field keep today's behavior. See
    /// harness audit F-6
    /// (`team/intake/2026-05-19-eval-traces-end-to-end-audit.md`).
    #[serde(default)]
    pub inputs_policy: InputsPolicy,
    /// Optional cap on the number of `bar_history` entries the eval
    /// executor surfaces to the trader LLM at each decision. `None`
    /// (the migration default) preserves today's behavior — the full
    /// `warmup_bars`-sized history slice is sent through. `Some(n)`
    /// trims the slice to its most-recent `n` entries before the
    /// trader sees it, which keeps the prompt prefix stable across
    /// many decisions so provider prompt-caching (Anthropic) can land
    /// a hit on the static portion.
    ///
    /// Persisted as a NULLable INTEGER on `agent_slots.bar_history_limit`
    /// (migration 025); the store layer maps NULL ↔ `None` and rejects
    /// non-positive ints (mapping `Some(0)` → `None`) so a stray `0`
    /// can't silently drop every bar from the trader's view.
    ///
    /// F-8 (`team/contracts/eval-prompt-cache-and-rolling-window.md`).
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub bar_history_limit: Option<u32>,
    /// How the dispatcher consults the cortex-memory layer
    /// (`xvision-memory`) for this slot. Persisted on the
    /// `agent_slots.memory_mode` column (migration 026). Defaults to
    /// `Off` so existing rows and clients that omit the field keep
    /// today's behavior — the dispatcher does not consult or write the
    /// memory store. See the V2D cortex-memory integration plan
    /// (`docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`).
    ///
    /// The ts-rs derive is intentionally omitted in Phase 2 — the
    /// `xvision-memory::MemoryMode` enum will be exported through the
    /// engine's `ts-export` feature in Phase 4, which is when the
    /// frontend gains a UI for this knob.
    #[serde(default)]
    pub memory_mode: xvision_memory::types::MemoryMode,
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
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::default(),
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
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
        // F-6: new slots default to `Raw` so existing behavior is
        // preserved; operators opt into `Causal` / `Oracle` explicitly.
        assert_eq!(a.slots[0].inputs_policy, InputsPolicy::Raw);
        // F-8: new slots leave the rolling-window cap unset; today's
        // executor behavior (no cap — full warmup_bars slice) is
        // preserved until the operator opts in.
        assert_eq!(a.slots[0].bar_history_limit, None);
        // V2D: new slots default to `Off` so the dispatcher does not
        // consult or write the cortex-memory store until an operator
        // explicitly opts in.
        assert_eq!(a.slots[0].memory_mode, xvision_memory::types::MemoryMode::Off);
    }

    #[test]
    fn memory_mode_defaults_to_off_on_deserialize() {
        // V2D: clients that omit `memory_mode` on POST/PUT bodies must
        // round-trip back as `Off` so the field is additive — pre-026
        // payloads keep today's behavior with no client changes.
        // `deny_unknown_fields` means we have to construct a full JSON
        // body that excludes only `memory_mode`.
        let json = serde_json::json!({
            "name": "main",
            "provider": "anthropic",
            "model": "claude-sonnet-4-6",
            "system_prompt": "p",
            "skill_ids": [],
        });
        let slot: AgentSlot = serde_json::from_value(json).unwrap();
        assert_eq!(slot.memory_mode, xvision_memory::types::MemoryMode::Off);
    }

    #[test]
    fn inputs_policy_wire_format_is_lowercase() {
        // Persisted column value + JSON wire shape both use lowercase
        // strings. Pin so a future rename doesn't silently invalidate
        // every persisted row.
        assert_eq!(InputsPolicy::Raw.as_str(), "raw");
        assert_eq!(InputsPolicy::Causal.as_str(), "causal");
        assert_eq!(InputsPolicy::Oracle.as_str(), "oracle");
        for v in [InputsPolicy::Raw, InputsPolicy::Causal, InputsPolicy::Oracle] {
            let s = serde_json::to_string(&v).unwrap();
            let back: InputsPolicy = serde_json::from_str(&s).unwrap();
            assert_eq!(v, back);
        }
        // Unknown strings parse back to the safe default rather than
        // crashing the store reader.
        assert_eq!(InputsPolicy::parse_or_raw("weird"), InputsPolicy::Raw);
        assert_eq!(InputsPolicy::parse_or_raw(""), InputsPolicy::Raw);
        assert_eq!(InputsPolicy::parse_or_raw("causal"), InputsPolicy::Causal);
        assert_eq!(InputsPolicy::parse_or_raw("oracle"), InputsPolicy::Oracle);
    }
}

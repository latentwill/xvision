//! Agent + AgentSlot value types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use xvision_core::providers::{lookup_model, ModelMetadata};

/// Default per-request `max_tokens` to seed on a freshly-created agent slot
/// when the selected model looks like a chain-of-thought (CoT) reasoning
/// model. CoT models (deepseek-r1, qwq, many gemma builds, …) emit a long
/// `<think>…</think>` prefix before any decision JSON, so the conservative
/// 1024 that config seeding otherwise applies is exhausted before the model
/// writes a single visible token. 8k gives the reasoning prefix room and
/// still leaves headroom for the structured decision body. See QA U12
/// (`docs/QA/2026-06-11-optimizer-ux-cli-findings.md`).
pub const COT_DEFAULT_MAX_TOKENS: u32 = 8_192;

/// Effective resolved `max_tokens` below which a CoT model is at high risk
/// of truncating before it emits any visible text. Used to gate the
/// pre-launch warning surfaced by the CLI (U12 (b)/(c)).
pub const COT_MIN_SAFE_MAX_TOKENS: u32 = 2_048;

/// Recommended minimum `max_tokens` operators should set for CoT models,
/// surfaced in the `strategy diagnostics` warning text (U12 (c)).
pub const COT_RECOMMENDED_MIN_MAX_TOKENS: u32 = 4_096;

/// Heuristic: does this model id look like a chain-of-thought / reasoning
/// model whose visible output is preceded by a long hidden reasoning
/// prefix?
///
/// Combines two signals:
///
/// 1. Canonical metadata — `lookup_model(model_id).is_reasoning()` catches
///    every id the metadata table annotates with a non-zero
///    `reasoning_token_default` (o1/o3, deepseek-reasoner, etc.).
/// 2. Name patterns the metadata table misses — Ollama-style ids carry a
///    `:tag` suffix (`deepseek-r1:8b`, `qwq:32b`) and self-hosted/family
///    aliases (`deepseek-r1`, `deepseek-r1-distill-…`, `gemma-2`, `qwq`)
///    never appear verbatim in the canonical table, so `lookup_model`
///    returns the unknown default with `reasoning_token_default == 0`.
///    Matching the family stem on the bare id (tag stripped) recovers them.
///
/// Matching is case-insensitive and tolerant of the `provider/model` and
/// `model:tag` shapes. Returns `true` if either signal fires.
pub fn looks_like_cot_model(model_id: &str) -> bool {
    if lookup_model(model_id).is_reasoning() {
        return true;
    }

    let lower = model_id.trim().to_ascii_lowercase();
    // Strip an optional `provider/` prefix (`openrouter/deepseek-r1`) and an
    // optional Ollama `:tag` suffix (`deepseek-r1:8b`) so family matching
    // sees the bare model stem.
    let stem = lower.rsplit('/').next().unwrap_or(&lower);
    let stem = stem.split(':').next().unwrap_or(stem);

    // Family stems whose models lead with a chain-of-thought prefix. Kept
    // deliberately broad: any deepseek-r* (r1, r1-distill, future r2), the
    // qwq reasoning line, Qwen3 thinking models, Fino/VibeThinker local
    // trader models, and the gemma family (many community gemma builds emit
    // verbose reasoning and exhaust a 1k budget).
    const COT_PREFIXES: &[&str] = &["deepseek-r", "qwq", "qwen3", "fino", "vibethinker", "gemma"];
    COT_PREFIXES.iter().any(|p| stem.starts_with(p))
}

/// Default `reasoning_effort` value forwarded to the gateway for CoT
/// reasoning models (deepseek-r1, qwq, etc.) via the Cline sidecar's
/// `StartRunParams`. CoT models require an explicit effort hint so the
/// gateway allocates reasoning tokens before the visible JSON answer;
/// without it the reasoning-token budget defaults to zero on some
/// providers and the model emits nothing.
///
/// Returns `Some("medium".to_string())` when `looks_like_cot_model(model_id)`,
/// else `None`. Callers forward the value to the sidecar as
/// `StartRunParams::reasoning_effort`; the sidecar passes it to the
/// provider gateway. Non-CoT models receive `None` (field omitted on the
/// wire via `skip_serializing_if = "Option::is_none"`).
pub fn default_reasoning_effort(model_id: &str) -> Option<String> {
    if looks_like_cot_model(model_id) {
        Some("medium".to_string())
    } else {
        None
    }
}

/// Generic conservative cap for unknown providers / unannounced models
/// when `provider_default_max_tokens` can't find a canonical metadata
/// entry. Big enough to fit a typical trader decision JSON + reasoning;
/// small enough that a runaway provider can't burn through an operator's
/// budget on the first miss. See `eval-token-efficiency-tail` (F41).
pub const CONSERVATIVE_DEFAULT_MAX_TOKENS: u32 = 8_192;

/// Sensible default for the OpenAI-compatible family (vanilla OpenAI,
/// OpenRouter, DeepSeek, Groq, Together, Mistral, xAI, etc.) when no
/// canonical model metadata entry matches. 16k matches the
/// gpt-4.1-mini class budget and leaves headroom for the structured-
/// output schemas the eval traders typically use. See
/// `eval-token-efficiency-tail` (F41).
pub const OPENAI_COMPAT_DEFAULT_MAX_TOKENS: u32 = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderFamily {
    Anthropic,
    OpenAiCompat,
    Unknown,
}

fn normalise_provider(provider: &str) -> ProviderFamily {
    match provider.trim().to_lowercase().as_str() {
        "anthropic" => ProviderFamily::Anthropic,
        "openai" | "openai-compat" | "openrouter" | "deepseek" | "groq" | "together" | "mistral" | "xai"
        | "x-ai" | "fireworks" | "perplexity" | "vllm" => ProviderFamily::OpenAiCompat,
        _ => ProviderFamily::Unknown,
    }
}

/// Resolve a sensible per-(provider, model) default `max_tokens` for
/// slots where the operator left the field unset.
///
/// Resolution order:
///
/// 1. Canonical model metadata (`lookup_model(model).auto_max_tokens()`).
///    Covers every model id listed in
///    `crates/xvision-core/src/providers/model_metadata.rs`. A hit is
///    detected by the proxy "metadata is not the `unknown_default`" —
///    auto > 4096 OR `reasoning_token_default > 0`.
/// 2. Per-provider fallback table when the metadata missed:
///    - `anthropic` → use the metadata `auto_max_tokens` even for the
///      unknown fallback (matches `anthropic_request_body`'s wire-time
///      contract: the API requires `max_tokens` and the dispatcher
///      already falls back to the per-model auto value).
///    - OpenAI-compat family (`openai`, `openai-compat`, `openrouter`,
///      `deepseek`, `groq`, `together`, `mistral`, `xai`, `fireworks`,
///      `perplexity`, `vllm`) → `OPENAI_COMPAT_DEFAULT_MAX_TOKENS` (16k).
///    - Unknown provider → `CONSERVATIVE_DEFAULT_MAX_TOKENS` (8k).
///
/// Always returns a positive `u32`. Callers use the returned value when
/// `AgentSlot::resolve_max_tokens` returned `None`.
///
/// See `team/contracts/eval-token-efficiency-tail.md`.
pub fn provider_default_max_tokens(provider: &str, model: &str) -> u32 {
    let meta = lookup_model(model);
    // `lookup_model` returns `ModelMetadata::unknown_default` for ids
    // missing from the canonical table. The unknown_default has
    // `output_token_ceiling == 4096`; every real canonical row has a
    // larger ceiling (Anthropic 8k, gpt-4o 16k, gpt-4.1 32k, etc.).
    // Use the ceiling as the canonical-hit proxy so claude-sonnet-4-6
    // (which has `recommended_visible_output: 4096`) still resolves
    // via the metadata path.
    let canonical_hit = meta.output_token_ceiling > 4_096 || meta.reasoning_token_default > 0;
    if canonical_hit {
        return meta.auto_max_tokens();
    }

    match normalise_provider(provider) {
        // Anthropic always honours the metadata path — including
        // `unknown_default`'s 4096 — to mirror the wire-time fallback
        // in `anthropic_request_body`.
        ProviderFamily::Anthropic => meta.auto_max_tokens(),
        ProviderFamily::OpenAiCompat => OPENAI_COMPAT_DEFAULT_MAX_TOKENS,
        ProviderFamily::Unknown => CONSERVATIVE_DEFAULT_MAX_TOKENS,
    }
}

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
    /// If `Some(strategy_id)`, this agent is scoped to a single
    /// strategy and hidden from the default workspace agent list
    /// (`GET /api/agents`). Persists the "Save as reusable agent"
    /// toggle on the strategy editor's inline Filter composer —
    /// toggle ON (default) → `None`, toggle OFF → `Some(<id>)`.
    /// Migration 036.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(type = "string | null"))]
    pub scope_strategy_id: Option<String>,
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
    /// QA30 follow-on: optional operator override for the per-step
    /// wall-clock budget (Cline runtime). `None` means "no enforcement"
    /// — the runtime defaults to `u32::MAX` so a wedged sidecar step
    /// would still hang the cycle, but a still-responding slow model
    /// is not killed. `Some(n)` is honoured verbatim; the unit is
    /// milliseconds.
    ///
    /// Stored in SQLite as a non-null integer with `0` as the sentinel
    /// for `None` (migration 047), matching the `max_tokens` shape so
    /// the store-layer 0-as-unset projection can be reused.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub max_wall_ms: Option<u32>,
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
    /// Per-slot opt-out for the `trader-noop-skip` pre-LLM gate
    /// (`team/intake/2026-05-21-eval-honesty-and-agent-graph.md`).
    ///
    /// When `None` (the default) or `Some(true)`, the engine skips the
    /// LLM call entirely for any decision cycle where the current
    /// `portfolio_state` allows **zero legal open actions** (i.e. the
    /// portfolio already holds a position on this asset so both
    /// `long_open` and `short_open` are blocked — the only legal action
    /// is `hold`). A synthesized trader output with `action: hold`,
    /// `conviction: 0`, and `noop_skip` in `justification` is recorded so
    /// the trace and eval review surfaces can see that the skip happened.
    ///
    /// Set to `Some(false)` when you explicitly want the LLM to run even
    /// in a zero-legal-actions state (e.g. "what would the model say in
    /// a corner case?" analysis). This opt-out is intentional model spend
    /// and is honoured without warning.
    ///
    /// Not yet persisted to SQLite (a follow-up migration will add the
    /// column when the UI surfaces this knob). For now the field
    /// round-trips through JSON via `#[serde(default)]`; rows loaded
    /// from the store come back as `None` which is treated the same as
    /// `Some(true)`.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "boolean | null"))]
    pub noop_skip: Option<bool>,
    /// Tool names this slot is allowed to invoke. Empty means callers may
    /// fall back to a strategy-level required tool list for legacy strategy
    /// slots; persisted as JSON on `agent_slots.allowed_tools_json`.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Per-slot opt-in for **delta-briefing mode** (F41 token-efficiency
    /// tail). When `Some(true)`, the trader briefing for bar N+1 is
    /// compressed to **only the delta** from bar N's briefing — changed
    /// indicators, new fills, regime transitions — rather than the full
    /// snapshot. Falls back to the full briefing on cache miss (first
    /// bar of a run; the prev briefing wasn't tracked; or the diff
    /// would be too sparse to be useful).
    ///
    /// Defaults to `None` ≡ `Some(false)`. The full-briefing path is
    /// byte-identical to pre-F41 behaviour so existing eval runs are
    /// unaffected. Operators opt in per slot when they want to lean on
    /// provider prompt caching (Anthropic `cache_control`) **and**
    /// shrink the variable suffix of the prompt — the two combine to
    /// cut per-cycle token spend by ~60% on long horizons.
    ///
    /// Not yet persisted to SQLite (a follow-up migration will add the
    /// column when the UI surfaces this knob). For now the field
    /// round-trips through JSON via `#[serde(default)]`; rows loaded
    /// from the store come back as `None` (full briefing).
    ///
    /// See `team/contracts/eval-token-efficiency-tail.md` and
    /// `crates/xvision-engine/src/agent/briefing.rs` for the diff shape
    /// and `delta(prev, curr)` function.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "boolean | null"))]
    pub delta_briefing: Option<bool>,
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

    /// QA30 follow-on: resolve `max_wall_ms` to the dispatcher-side
    /// `Option<u32>` in the same shape as `resolve_max_tokens`.
    ///
    /// - `None` (or the SQLite storage sentinel `Some(0)`) → `None`,
    ///   meaning "no enforcement" — the Cline runtime then falls back
    ///   to `DEFAULT_MAX_WALL_MS = u32::MAX`.
    /// - `Some(n > 0)` passes through verbatim. The unit is milliseconds.
    pub fn resolve_max_wall_ms(&self) -> Option<u32> {
        match self.max_wall_ms {
            Some(n) if n > 0 => Some(n),
            _ => None,
        }
    }

    /// Resolve `delta_briefing` to a concrete bool. `None` (the default)
    /// and `Some(false)` both disable the delta-briefing path; `Some(true)`
    /// enables it. Mirrors the `Option<bool>` storage shape used by
    /// `noop_skip`. See `agent/briefing.rs` for the diff function this
    /// flag gates.
    pub fn resolve_delta_briefing(&self) -> bool {
        matches!(self.delta_briefing, Some(true))
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
        let model: String = model.into();
        // U12: CoT models burn a long hidden reasoning prefix before any
        // visible token. Seed a higher default so a freshly-created slot
        // does not truncate at the conservative config-seeded budget before
        // emitting the decision JSON. Non-CoT models keep `None` (auto from
        // model metadata).
        let default_max_tokens = if looks_like_cot_model(&model) {
            Some(COT_DEFAULT_MAX_TOKENS)
        } else {
            None
        };
        Self {
            agent_id: agent_id.into(),
            name: name.into(),
            description: String::new(),
            tags: Vec::new(),
            slots: vec![AgentSlot {
                name: "main".to_string(),
                provider: provider.into(),
                model,
                system_prompt: String::new(),
                skill_ids: Vec::new(),
                max_tokens: default_max_tokens,
                max_wall_ms: None,
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: InputsPolicy::default(),
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
                allowed_tools: Vec::new(),
                delta_briefing: None,
            }],
            archived: false,
            created_at: now,
            updated_at: now,
            scope_strategy_id: None,
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
        // trader-noop-skip: new slots default to `None` (equivalent to
        // `Some(true)` — the skip is enabled). Operators who want the
        // LLM to run in zero-legal-actions corners explicitly set `false`.
        assert_eq!(a.slots[0].noop_skip, None);
        // F41 token-efficiency tail: new slots default to `None`
        // (equivalent to `Some(false)` — full briefing). Operators
        // opt into delta-briefing per slot.
        assert_eq!(a.slots[0].delta_briefing, None);
        assert!(!a.slots[0].resolve_delta_briefing());
    }

    #[test]
    fn cot_models_get_default_reasoning_effort() {
        // CoT models (deepseek-r1 family, qwq, gemma) get "medium".
        assert_eq!(
            default_reasoning_effort("deepseek-r1:8b"),
            Some("medium".to_string())
        );
        // Plain chat model gets None (field omitted on the wire).
        assert_eq!(default_reasoning_effort("gpt-4o"), None);
        // Sanity-check a few more to confirm both code paths fire.
        assert_eq!(default_reasoning_effort("qwq:32b"), Some("medium".to_string()));
        assert_eq!(default_reasoning_effort("claude-sonnet-4-6"), None);
        assert_eq!(default_reasoning_effort("Fino1-8B"), Some("medium".to_string()));
        assert_eq!(default_reasoning_effort("Qwen3-4B"), Some("medium".to_string()));
        assert_eq!(
            default_reasoning_effort("VibeThinker-3B"),
            Some("medium".to_string())
        );
    }

    #[test]
    fn looks_like_cot_model_matches_ollama_tagged_and_family_ids() {
        // Ollama `:tag` ids that the canonical metadata table misses.
        assert!(looks_like_cot_model("deepseek-r1:8b"));
        assert!(looks_like_cot_model("qwq:32b"));
        // Family stems / aliases without a tag.
        assert!(looks_like_cot_model("deepseek-r1"));
        assert!(looks_like_cot_model("deepseek-r1-distill-qwen-7b"));
        assert!(looks_like_cot_model("gemma-2"));
        assert!(looks_like_cot_model("gemma2:9b"));
        // `provider/model` shape is tolerated.
        assert!(looks_like_cot_model("openrouter/deepseek-r1"));
        // Case-insensitive.
        assert!(looks_like_cot_model("DeepSeek-R1:8B"));
        // Frontier/ORB bakeoff local models also lead with CoT prose.
        assert!(looks_like_cot_model("Fino1-8B"));
        assert!(looks_like_cot_model("Qwen3-4B"));
        assert!(looks_like_cot_model("VibeThinker-3B"));
    }

    #[test]
    fn looks_like_cot_model_rejects_plain_chat_models() {
        assert!(!looks_like_cot_model("claude-sonnet-4-6"));
        assert!(!looks_like_cot_model("gpt-4o-mini"));
        assert!(!looks_like_cot_model("llama3.2"));
        assert!(!looks_like_cot_model("kimi-k2"));
        assert!(!looks_like_cot_model(""));
        // `deepseek-v3` is a non-reasoning chat model — must NOT match the
        // `deepseek-r` family stem.
        assert!(!looks_like_cot_model("deepseek-v3"));
    }

    #[test]
    fn single_slot_default_seeds_higher_max_tokens_for_cot_model() {
        // U12: a CoT model gets the elevated default so the reasoning
        // prefix doesn't truncate the slot before any visible output.
        let a = Agent::single_slot_default("01HZ000000000000000000000", "cot", "ollama", "deepseek-r1:8b");
        assert_eq!(a.slots[0].max_tokens, Some(COT_DEFAULT_MAX_TOKENS));
        // And a plain chat model still defaults to auto (`None`).
        let b = Agent::single_slot_default(
            "01HZ000000000000000000001",
            "chat",
            "anthropic",
            "claude-sonnet-4-6",
        );
        assert_eq!(b.slots[0].max_tokens, None);
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

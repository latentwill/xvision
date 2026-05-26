//! Unit tests for `AgentSlot::resolve_max_tokens` and the F41
//! per-provider `max_tokens` default helper.
//!
//! Public helpers + constants live in
//! [`super::model`] (`provider_default_max_tokens`,
//! `CONSERVATIVE_DEFAULT_MAX_TOKENS`, `OPENAI_COMPAT_DEFAULT_MAX_TOKENS`).
//! The acceptance test for end-to-end behaviour lives in
//! `crates/xvision-engine/tests/eval_max_tokens_default.rs`.
//!
//! Contract (replaces the q15 clamp design):
//!
//! - The operator's `Option<u32>` passes through verbatim.
//! - The SQLite-storage sentinel `Some(0)` (used by the store to
//!   represent "unset" without a schema migration) collapses to `None`.
//! - No model-metadata clamp. An unknown model id used to silently
//!   collapse the operator's value to 4096 via `unknown_default`; that
//!   regression is the reason this contract was reworked.
//!
//! Per-provider fallback behaviour for `None` is owned by
//! [`super::model::provider_default_max_tokens`].

#![cfg(test)]

use super::model::{
    provider_default_max_tokens, AgentSlot, InputsPolicy, CONSERVATIVE_DEFAULT_MAX_TOKENS,
    OPENAI_COMPAT_DEFAULT_MAX_TOKENS,
};
use xvision_core::providers::lookup_model;

fn slot_with(provider: &str, model: &str, max_tokens: Option<u32>) -> AgentSlot {
    AgentSlot {
        name: "main".into(),
        provider: provider.into(),
        model: model.into(),
        system_prompt: "p".into(),
        skill_ids: Vec::new(),
        max_tokens,
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        capabilities: crate::agents::default_capabilities(),
        delta_briefing: None,
    }
}

#[test]
fn unset_slot_resolves_to_none() {
    let slot = slot_with("anthropic", "claude-sonnet-4-6", None);
    assert_eq!(slot.resolve_max_tokens(), None);
}

#[test]
fn explicit_value_passes_through_unchanged() {
    let slot = slot_with("anthropic", "claude-sonnet-4-6", Some(2000));
    assert_eq!(slot.resolve_max_tokens(), Some(2000));
}

#[test]
fn explicit_value_for_unknown_model_is_not_clamped() {
    // QA15-followup regression case: an unknown model id used to
    // collapse the operator's 200_000 down to 4096 via
    // `unknown_default.output_token_ceiling`. The new contract is
    // pass-through — operators who pick a big number get a big number.
    let slot = slot_with("openrouter", "deepseek/deepseek-anything-flash", Some(200_000));
    assert_eq!(slot.resolve_max_tokens(), Some(200_000));
}

#[test]
fn sentinel_zero_collapses_to_none() {
    // SQLite stores `None` as `0` so the schema can be `INTEGER NOT
    // NULL DEFAULT 0` without a migration. Any `Some(0)` that leaks
    // through must behave the same as `None`.
    let slot = slot_with("anthropic", "claude-sonnet-4-6", Some(0));
    assert_eq!(slot.resolve_max_tokens(), None);
}

#[test]
fn unknown_model_with_no_explicit_value_returns_none() {
    let slot = slot_with("acme-co", "unannounced-7b", None);
    assert_eq!(slot.resolve_max_tokens(), None);
}

#[test]
fn anthropic_unknown_model_falls_back_to_metadata_auto() {
    // For Anthropic, the dispatcher must fill the field at the wire
    // boundary. Match that contract here.
    let n = provider_default_max_tokens("anthropic", "claude-unknown-future-model");
    // unknown_default auto is 4096
    assert_eq!(n, 4_096);
}

#[test]
fn anthropic_known_model_returns_metadata_auto() {
    // claude-sonnet-4-6 has `recommended_visible_output: 4096` so its
    // auto is 4096 — agreement with metadata is the real assertion,
    // not "above unknown_default".
    let n = provider_default_max_tokens("anthropic", "claude-sonnet-4-6");
    let meta = lookup_model("claude-sonnet-4-6");
    assert_eq!(n, meta.auto_max_tokens());
}

#[test]
fn openai_compat_unknown_model_returns_16k_default() {
    for p in [
        "openai",
        "openrouter",
        "deepseek",
        "groq",
        "together",
        "mistral",
        "xai",
        "x-ai",
    ] {
        let n = provider_default_max_tokens(p, "unannounced-7b");
        assert_eq!(
            n, OPENAI_COMPAT_DEFAULT_MAX_TOKENS,
            "{p} unknown model must resolve to 16k default",
        );
    }
}

#[test]
fn openai_compat_known_model_uses_metadata_auto() {
    // gpt-4.1-mini is in the canonical table — must use metadata,
    // not the per-provider 16k fallback.
    let n = provider_default_max_tokens("openai", "gpt-4.1-mini");
    let meta = lookup_model("gpt-4.1-mini");
    assert_eq!(n, meta.auto_max_tokens());
}

#[test]
fn unknown_provider_returns_conservative_cap() {
    let n = provider_default_max_tokens("acme-co", "unannounced-7b");
    assert_eq!(n, CONSERVATIVE_DEFAULT_MAX_TOKENS);
}

#[test]
fn provider_normalisation_is_case_insensitive() {
    let n1 = provider_default_max_tokens("OpenAI", "unannounced-7b");
    let n2 = provider_default_max_tokens("openai", "unannounced-7b");
    assert_eq!(n1, n2);
}

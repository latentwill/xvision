//! End-to-end checks for `AgentSlot::resolve_max_tokens` — the
//! operator-facing token budget that the dispatcher hands to the
//! provider.
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
//! Per-provider fallback behaviour for `None` lives in the dispatchers
//! (see `anthropic_request_body` / `openai_compat_request_body` tests in
//! `agent/llm.rs`).

#![cfg(test)]

use crate::agents::model::{AgentSlot, InputsPolicy};

fn slot_with(provider: &str, model: &str, max_tokens: Option<u32>) -> AgentSlot {
    AgentSlot {
        name: "main".into(),
        provider: provider.into(),
        model: model.into(),
        system_prompt: "p".into(),
        skill_ids: Vec::new(),
        max_tokens,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
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

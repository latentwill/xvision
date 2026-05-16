//! End-to-end checks for the `AgentSlot.max_tokens` ↔ model metadata
//! resolver flow. Kept in its own module so the q15 contract's
//! verification command (`cargo test -p xvision-engine
//! agents::max_tokens_resolution`) maps to a single matching path.

#![cfg(test)]

use crate::agents::model::AgentSlot;
use xvision_core::providers::lookup_model;

fn slot_with(provider: &str, model: &str, max_tokens: Option<u32>) -> AgentSlot {
    AgentSlot {
        name: "main".into(),
        provider: provider.into(),
        model: model.into(),
        system_prompt: "p".into(),
        skill_ids: Vec::new(),
        max_tokens,
    }
}

#[test]
fn unset_slot_on_known_model_clears_legacy_1000_truncation() {
    // QA15 reproducer: Sonnet 4.6 with no explicit max_tokens. The legacy
    // hardcoded `1000` budget truncated before any visible text. The
    // resolver must hand the dispatcher a budget well above that.
    let slot = slot_with("anthropic", "claude-sonnet-4-6", None);
    let resolved = slot.resolve_max_tokens();
    assert!(
        resolved >= 4096,
        "resolved {resolved} must clear the QA15 1000-token truncation case",
    );
}

#[test]
fn explicit_value_overrides_auto_resolution() {
    let slot = slot_with("anthropic", "claude-sonnet-4-6", Some(2000));
    assert_eq!(slot.resolve_max_tokens(), 2000);
}

#[test]
fn explicit_value_is_clamped_to_model_ceiling() {
    let slot = slot_with("anthropic", "claude-haiku-4-5", Some(99_999));
    let meta = lookup_model(&slot.model);
    assert_eq!(slot.resolve_max_tokens(), meta.output_token_ceiling);
}

#[test]
fn non_reasoning_unset_defaults_to_visible_output_only() {
    let slot = slot_with("anthropic", "claude-haiku-4-5", None);
    let meta = lookup_model(&slot.model);
    assert!(!meta.is_reasoning());
    // reasoning_token_default == 0 for non-reasoning models, so the
    // auto value equals the visible budget.
    assert_eq!(slot.resolve_max_tokens(), meta.recommended_visible_output);
}

#[test]
fn reasoning_model_unset_adds_reasoning_budget() {
    let slot = slot_with("openai", "o3", None);
    let meta = lookup_model(&slot.model);
    assert!(meta.is_reasoning());
    let resolved = slot.resolve_max_tokens();
    // Auto value covers visible + reasoning when the model is reasoning
    // class — capped to the provider ceiling.
    let target = meta
        .recommended_visible_output
        .saturating_add(meta.reasoning_token_default)
        .min(meta.output_token_ceiling);
    assert_eq!(resolved, target);
}

#[test]
fn unknown_model_uses_default_metadata() {
    let slot = slot_with("acme-co", "unannounced-7b", None);
    let resolved = slot.resolve_max_tokens();
    let meta = lookup_model(&slot.model);
    assert!(!meta.is_reasoning());
    assert_eq!(resolved, meta.auto_max_tokens());
    assert!(resolved <= meta.output_token_ceiling);
}

#[test]
fn sentinel_zero_is_treated_as_unset() {
    // Storage maps `None` ↔ `Some(0)` at the SQLite boundary; the
    // resolver must keep auto-mode when a Some(0) sneaks through (e.g.
    // an older row that hasn't been touched since the migration).
    let slot = slot_with("anthropic", "claude-sonnet-4-6", Some(0));
    let meta = lookup_model(&slot.model);
    assert_eq!(slot.resolve_max_tokens(), meta.auto_max_tokens());
}

#[test]
fn openrouter_vendor_prefix_in_slot_model_still_resolves() {
    // Routes that came in via the OpenRouter catalog look like
    // `anthropic/claude-sonnet-4-6`. The slot stores the full id;
    // `model_metadata` must still find the canonical row.
    let slot = slot_with("openrouter", "anthropic/claude-sonnet-4-6", None);
    let meta = slot.model_metadata();
    assert_eq!(meta.output_token_ceiling, 8192);
}

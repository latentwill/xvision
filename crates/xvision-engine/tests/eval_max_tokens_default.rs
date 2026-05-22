//! Acceptance test for the per-provider `max_tokens` default fallbacks
//! shipped in `eval-token-efficiency-tail` (F41).
//!
//! Contract:
//!
//! 1. `AgentSlot::resolve_max_tokens` returns the operator's explicit
//!    value verbatim (or `None` when the slot is unset / carries the
//!    SQLite sentinel `Some(0)`).
//! 2. `provider_default_max_tokens(provider, model)` returns a sensible
//!    per-(provider, model) default that callers can use when
//!    `resolve_max_tokens` returned `None`:
//!     - Known model (in the canonical `xvision-core` table) → the
//!       metadata `auto_max_tokens`.
//!     - Unknown model on an OpenAI-compat provider → 16k.
//!     - Unknown model on an unknown provider → 8k conservative cap.
//!     - Anthropic always uses the metadata path (including
//!       `unknown_default`'s 4096) to mirror the wire-time fallback
//!       in `anthropic_request_body`.
//! 3. Operator's explicit `Some(n)` wins over the per-provider default
//!    (the audit's "operators who pick a big number get a big number"
//!    rule, established by QA15-followup, must still hold).

use xvision_engine::agents::model::{
    provider_default_max_tokens, AgentSlot, InputsPolicy, CONSERVATIVE_DEFAULT_MAX_TOKENS,
    OPENAI_COMPAT_DEFAULT_MAX_TOKENS,
};

fn slot_with(provider: &str, model: &str, max_tokens: Option<u32>) -> AgentSlot {
    AgentSlot {
        name: "trader".into(),
        provider: provider.into(),
        model: model.into(),
        system_prompt: "You are a deterministic eval-baseline trader.".into(),
        skill_ids: Vec::new(),
        max_tokens,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        delta_briefing: None,
    }
}

#[test]
fn explicit_slot_value_wins_over_provider_default() {
    // Per the QA15-followup contract: operators who pick a big number
    // get a big number. The per-provider default never overrides an
    // explicit slot value.
    for provider in ["anthropic", "openai", "openrouter", "deepseek", "acme-co"] {
        let slot = slot_with(provider, "unannounced-7b", Some(200_000));
        assert_eq!(
            slot.resolve_max_tokens(),
            Some(200_000),
            "{provider} explicit value must pass through verbatim — no clamping",
        );
    }
}

#[test]
fn known_model_uses_canonical_metadata_auto() {
    // Models in the canonical metadata table — the per-provider
    // default falls back to the metadata `auto_max_tokens`.
    // gpt-4.1-mini sits in `xvision-core::providers::model_metadata`.
    let n = provider_default_max_tokens("openai", "gpt-4.1-mini");
    let meta = xvision_engine::agents::lookup_model("gpt-4.1-mini");
    assert_eq!(
        n,
        meta.auto_max_tokens(),
        "known models must use metadata auto_max_tokens — not the per-provider 16k fallback",
    );
}

#[test]
fn unknown_model_on_openai_compat_provider_returns_16k_default() {
    // The OpenAI-compatible family (openai, openrouter, deepseek, groq,
    // together, mistral, xai/x-ai, fireworks, perplexity) all share
    // the 16k default for unknown models — matches the gpt-4.1-mini
    // class budget and leaves headroom for structured-output schemas.
    for p in [
        "openai",
        "openai-compat",
        "openrouter",
        "deepseek",
        "groq",
        "together",
        "mistral",
        "xai",
        "x-ai",
        "fireworks",
        "perplexity",
    ] {
        let n = provider_default_max_tokens(p, "unannounced-7b");
        assert_eq!(
            n, OPENAI_COMPAT_DEFAULT_MAX_TOKENS,
            "{p} unknown model must resolve to 16k default",
        );
        assert_eq!(n, 16_384, "OPENAI_COMPAT_DEFAULT_MAX_TOKENS pinned at 16k");
    }
}

#[test]
fn unknown_provider_with_unknown_model_returns_conservative_8k_cap() {
    // Provider not in any known family — the conservative 8k cap kicks
    // in so a runaway provider can't burn the operator's budget on
    // the first miss.
    for p in ["acme-co", "totally-made-up", "internal-test-provider"] {
        let n = provider_default_max_tokens(p, "unannounced-7b");
        assert_eq!(
            n, CONSERVATIVE_DEFAULT_MAX_TOKENS,
            "{p} unknown provider must resolve to 8k conservative cap",
        );
        assert_eq!(n, 8_192, "CONSERVATIVE_DEFAULT_MAX_TOKENS pinned at 8k");
    }
}

#[test]
fn anthropic_provider_uses_metadata_even_on_unknown_model() {
    // Anthropic's API requires `max_tokens` at the wire boundary, and
    // `anthropic_request_body` already falls back to
    // `lookup_model(model).auto_max_tokens()` for `None`. The
    // per-provider helper mirrors that contract so anthropic-routed
    // unknown models get the 4096 `unknown_default` (not the
    // OpenAI-compat 16k, which would be wrong for the API).
    let n = provider_default_max_tokens("anthropic", "claude-unknown-future-model");
    assert_eq!(n, 4_096, "anthropic unknown_default auto_max_tokens is 4096");
}

#[test]
fn anthropic_known_model_returns_metadata_auto() {
    // Sanity check that the canonical metadata hit fires for real
    // claude-* models. claude-sonnet-4-6 carries
    // `recommended_visible_output: 4096` so the auto value is 4096 —
    // the ceiling (8192) is the canonical-hit signal, not the auto
    // itself. Pin both so a future metadata tweak surfaces here.
    let n = provider_default_max_tokens("anthropic", "claude-sonnet-4-6");
    let meta = xvision_engine::agents::lookup_model("claude-sonnet-4-6");
    assert_eq!(n, meta.auto_max_tokens());
    assert!(
        meta.output_token_ceiling > 4_096,
        "claude-sonnet-4-6 ceiling must exceed unknown_default — got {}",
        meta.output_token_ceiling,
    );
}

#[test]
fn provider_normalisation_is_case_insensitive() {
    // Operators sometimes spell providers differently across configs.
    // The normalisation lowercases + trims before matching the family
    // table.
    let n1 = provider_default_max_tokens("OpenAI", "unannounced-7b");
    let n2 = provider_default_max_tokens("openai", "unannounced-7b");
    let n3 = provider_default_max_tokens("  OPENAI ", "unannounced-7b");
    assert_eq!(n1, OPENAI_COMPAT_DEFAULT_MAX_TOKENS);
    assert_eq!(n1, n2);
    assert_eq!(n2, n3);
}

#[test]
fn unset_slot_resolves_to_none_so_caller_consults_provider_default() {
    // The contract is two-step: `resolve_max_tokens` returns `None`
    // for unset slots, then the caller (dispatcher / pipeline) reads
    // `provider_default_max_tokens` to pick a default.
    let slot = slot_with("openrouter", "unannounced-7b", None);
    assert_eq!(slot.resolve_max_tokens(), None);
    // Caller's responsibility: choose the per-provider default.
    let chosen = slot
        .resolve_max_tokens()
        .unwrap_or_else(|| provider_default_max_tokens(&slot.provider, &slot.model));
    assert_eq!(
        chosen, OPENAI_COMPAT_DEFAULT_MAX_TOKENS,
        "openrouter unknown model should bottom out at the 16k default",
    );
}

#[test]
fn sqlite_sentinel_zero_collapses_to_none_then_uses_provider_default() {
    // The store layer round-trips `None` as `Some(0)` so the column
    // can stay `INTEGER NOT NULL DEFAULT 0`. The resolver treats
    // `Some(0)` the same as `None`.
    let slot = slot_with("acme-co", "unannounced-7b", Some(0));
    assert_eq!(slot.resolve_max_tokens(), None);
    let chosen = slot
        .resolve_max_tokens()
        .unwrap_or_else(|| provider_default_max_tokens(&slot.provider, &slot.model));
    assert_eq!(chosen, CONSERVATIVE_DEFAULT_MAX_TOKENS);
}

//! Shared pre-cycle validation for optimizer run-cycle launches.
//!
//! F26 (QA 2026-06-04): the CLI and the dashboard route both launch optimizer
//! cycles. Previously each surface built its own cycle setup, so a guard added to
//! one (e.g. the F22 cross-provider preflight) silently did not protect the
//! other. This module is the single home for that guard — both `xvn optimizer
//! run-cycle` and `POST /api/autooptimizer/run-cycle` call
//! [`preflight_trader_provider`], so neither can drift.

use sqlx::SqlitePool;

use crate::strategies::{DecisionMode, Strategy};

/// A confirmed pre-cycle rejection. Carries an operator-facing message; each
/// surface maps it onto its own error type (`CliError::usage`,
/// `DashboardError::Validation`).
#[derive(Debug, Clone)]
pub struct PreflightReject {
    pub message: String,
}

impl std::fmt::Display for PreflightReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PreflightReject {}

/// F22 (QA 2026-06-04): block a cycle whose strategy would route its paper-test
/// trader through a provider that differs from the cycle's dispatch provider.
///
/// The optimizer paper-test uses a single dispatch (the cycle's
/// `--provider`/`mutator.provider`, e.g. `openrouter`) for every backtest
/// decision, sending each slot's model id as-is. An agentless strategy with a
/// legacy `trader_slot` pinned to a different provider (e.g. the seeded examples'
/// `anthropic.claude-sonnet-4.6`) therefore sends an anthropic-format model id to
/// openrouter and dies with a confusing `... is not a valid model ID` 400.
///
/// Rather than that cross-provider 400, fail fast with guidance. Mechanistic
/// (non-LLM) strategies and agent-backed strategies whose slots resolve to the
/// cycle provider are unaffected.
pub async fn preflight_trader_provider(
    pool: &SqlitePool,
    strategy: &Strategy,
    strategy_id: &str,
    cycle_provider: &str,
    mock: bool,
    // F33: when true, skip the provider-consistency check entirely. The operator
    // has explicitly requested a different mutator provider (--mutator-provider)
    // from the paper-test provider (--provider) and accepts the risk.
    skip_provider_check: bool,
) -> Result<(), PreflightReject> {
    if mock || strategy.decision_mode == DecisionMode::Mechanistic || skip_provider_check {
        return Ok(());
    }
    // The optimizer paper-test routes EVERY trader decision through the cycle's
    // single dispatch (`cycle_provider`, e.g. openrouter), sending each slot's
    // model id as-is. Collect the provider each trader will actually route to,
    // from agent-backed slots AND the legacy `trader_slot`, then block if any
    // differs from the cycle provider — that's the cross-provider 400.
    let mut routes: Vec<(String, String)> = Vec::new();
    if !strategy.agents.is_empty() {
        // Resolution failure here isn't ours to surface — the normal run path
        // reports it. We only block on a *confirmed* cross-provider mismatch.
        if let Ok(slots) = crate::agent::pipeline::resolve_agent_slots_for_strategy(pool, strategy).await {
            for rs in &slots {
                let model = rs.slot.effective_model();
                if let Some(p) = infer_trader_provider(rs.slot.provider.as_deref().unwrap_or(""), &model) {
                    routes.push((p, model));
                }
            }
        }
    } else if let Some(slot) = &strategy.trader_slot {
        // F31: a legacy `trader_slot` with no explicit model binding is *unbound*
        // — it no longer silently derives a model from `attested_with` provenance.
        // Reject with a clear message instead of letting the cycle dispatch an
        // empty model id (or, pre-F31, a surprise anthropic one).
        if !slot.has_model_binding() {
            return Err(PreflightReject {
                message: format!(
                    "strategy {strategy_id}'s trader has no model configured: its legacy \
                     trader_slot sets no explicit `model` (the `attested_with` field is \
                     provenance only and is no longer used as a binding — see F31). \
                     Re-author the trader with an explicit provider + model on the registered \
                     provider '{cycle_provider}', attach an agent, or convert the strategy to a \
                     mechanistic (non-LLM) decision mode."
                ),
            });
        }
        let model = slot.effective_model();
        if let Some(p) = infer_trader_provider(slot.provider.as_deref().unwrap_or(""), &model) {
            routes.push((p, model));
        }
    }

    for (slot_provider, model) in routes {
        if !slot_provider.eq_ignore_ascii_case(cycle_provider) {
            return Err(PreflightReject {
                message: format!(
                    "strategy {strategy_id}'s trader is pinned to provider '{slot_provider}' \
                     (model '{model}'), which differs from this cycle's provider '{cycle_provider}'. \
                     The paper-test reuses the strategy's own trader model (so optimized winners stay \
                     interchangeable), so it would send a '{slot_provider}'-format model id to \
                     '{cycle_provider}' and fail with a cross-provider error. Fix: re-author the \
                     strategy's trader onto a registered provider — '{cycle_provider}' is already \
                     registered here, so point the trader at it (the seeded examples were previously \
                     hardcoded to 'anthropic', which an openrouter-only node can't dispatch — see F30). \
                     If '{slot_provider}' really is a provider you have registered, run the cycle with \
                     that provider instead; or convert the strategy to a mechanistic (non-LLM) \
                     decision mode."
                ),
            });
        }
    }
    Ok(())
}

/// Infer the provider a trader slot will actually route to, from its explicit
/// `provider` and resolved model id.
///
/// F22 (QA 2026-06-04): a slot may carry no explicit `provider`, only a model
/// id, so keying off `slot.provider` alone misses the real route. Infer from the
/// model id format instead. (F31: `effective_model()` no longer falls back to
/// `attested_with`, so a truly model-less slot now yields an empty model here →
/// `None`; the unbound case is rejected up-front in `preflight_trader_provider`.)
/// Inference rules:
///   - explicit `provider` always wins;
///   - an OpenRouter `vendor/model` id (slash) is served by the openrouter route;
///   - an attestation id with a known `<provider>.<model>` dotted prefix
///     (e.g. `anthropic.claude-sonnet-4.6`) implies that provider;
///   - otherwise unknown (a bare id like `claude-haiku-4-5`, or a version dot
///     like `claude-3.5`, carries no reliable provider hint) → `None`, so we
///     don't false-positive-block it.
pub fn infer_trader_provider(explicit_provider: &str, model: &str) -> Option<String> {
    const KNOWN_PROVIDER_PREFIXES: &[&str] = &[
        "anthropic",
        "openai",
        "google",
        "meta",
        "mistral",
        "mistralai",
        "deepseek",
        "qwen",
        "xai",
        "cohere",
        "nous",
        "nousresearch",
    ];
    let explicit = explicit_provider.trim();
    if !explicit.is_empty() {
        return Some(explicit.to_string());
    }
    let model = model.trim();
    if model.is_empty() {
        return None;
    }
    if model.contains('/') {
        // OpenRouter `vendor/model` id — served by the openrouter route.
        return Some("openrouter".to_string());
    }
    if let Some((vendor, rest)) = model.split_once('.') {
        let vendor_lc = vendor.to_ascii_lowercase();
        if !rest.is_empty() && KNOWN_PROVIDER_PREFIXES.contains(&vendor_lc.as_str()) {
            return Some(vendor_lc);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_trader_provider_handles_attestation_and_openrouter_ids() {
        // explicit provider always wins
        assert_eq!(
            infer_trader_provider("openrouter", "anthropic.claude-sonnet-4.6"),
            Some("openrouter".to_string())
        );
        // openrouter vendor/model id
        assert_eq!(
            infer_trader_provider("", "google/gemini-3.1-flash-lite"),
            Some("openrouter".to_string())
        );
        // dotted attestation prefix → that provider
        assert_eq!(
            infer_trader_provider("", "anthropic.claude-sonnet-4.6"),
            Some("anthropic".to_string())
        );
        // bare id with no reliable hint → None (don't false-positive-block)
        assert_eq!(infer_trader_provider("", "claude-haiku-4-5"), None);
        // version dot is not a provider prefix → None
        assert_eq!(infer_trader_provider("", "claude-3.5"), None);
        // empty → None
        assert_eq!(infer_trader_provider("", ""), None);
    }
}

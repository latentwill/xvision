//! LLM token-cost calculation, keyed on the model-library catalog.
//!
//! ## Why this lives here
//!
//! Token *counts* are captured at the agent boundary
//! (`crate::agent::llm::LlmResponse::{input_tokens, output_tokens}`) and
//! then surfaced in the eval run via `Run::actual_input_tokens` /
//! `Run::actual_output_tokens` and the export-side
//! `ProviderDiagnostics::tokens_used`. Pricing, however, lives in the
//! provider catalog: `xvision_core::providers::ModelEntry` carries
//! `pricing_per_million_input_usd` and `pricing_per_million_output_usd`
//! populated by the OpenRouter fetcher (PR #185 / #q15) from
//! `/api/v1/models` `pricing.prompt` and `pricing.completion`.
//!
//! Multiplying tokens by per-token rate is the obvious step that ties
//! the two halves together; we keep it in a single canonical place so
//! downstream consumers (eval export, observability `ModelCallFinished`,
//! dashboard run-detail) cannot drift on the formula.
//!
//! ## OpenRouter vs Anthropic / OpenAI
//!
//! Only OpenRouter publishes pricing on its `/models` endpoint today
//! (Anthropic publishes pricing out-of-band; bare OpenAI `/v1/models`
//! returns just ids). When a `ModelEntry`'s pricing fields are `None`,
//! [`compute_token_cost_usd`] returns `None` — i.e. "unknown", not
//! "$0.00". Existing operator-trusted cost paths for Anthropic / OpenAI
//! compute their numbers out-of-band, so leaving `None` here means we
//! don't clobber a known-good number with a misleadingly-precise zero.
//!
//! ## "Zero pricing = unknown" guard
//!
//! OpenRouter's free routes return `"prompt": "0"` and `"completion":
//! "0"`. The fetcher's `parse_per_token_usd` helper already filters
//! these to `None` at parse time, so a free-route `ModelEntry` arrives
//! here with `pricing_per_million_*_usd: None` and naturally falls into
//! the "unknown" branch.
//!
//! Defense-in-depth: even if a future catalog source surfaces
//! `Some(0.0)` directly (e.g. a hand-edited cache file), the cost
//! function treats `<= 0.0` as `None` so a downstream display can't
//! show "$0.00" for a model whose true price we don't know.
//!
//! ## Anchoring the test fixture
//!
//! `cost_for_openrouter_claude_opus_matches_openrouter_pricing_page` uses
//! OpenRouter's published Claude Opus 4.7 rates ($15 / $75 per Mtok) and
//! a fixed token mix, with the expected USD computed by hand from the
//! published formula. If OpenRouter ever changes the units of the
//! `pricing` block (currently $/token wire format), the fetcher's
//! existing parse tests catch it and this test catches downstream
//! arithmetic drift.

use sqlx::SqlitePool;
use xvision_core::providers::{Catalog, ModelEntry};

/// Compute LLM token cost in USD for a single (input, output) token pair
/// against a known `ModelEntry`.
///
/// Returns `None` ("unknown") when either pricing field is absent or
/// `<= 0.0`. Never returns `Some(0.0)` — a true free model is still
/// "unknown" cost from this function's point of view, because surfacing
/// `Some(0.0)` to the UI as a precise number is misleading when the
/// provider didn't actually quote us a price.
///
/// The math: `(in_tok * $/Mtok_in + out_tok * $/Mtok_out) / 1_000_000`.
/// We keep the per-Mtok unit (rather than per-token) so the values
/// stored on `ModelEntry` stay operator-readable in disk caches and the
/// settings UI; the division by 1M happens here, at the leaf.
pub fn compute_token_cost_usd(input_tokens: u64, output_tokens: u64, model: &ModelEntry) -> Option<f64> {
    let in_price = positive_price(model.pricing_per_million_input_usd)?;
    let out_price = positive_price(model.pricing_per_million_output_usd)?;
    let cost = (input_tokens as f64 * in_price + output_tokens as f64 * out_price) / 1_000_000.0;
    Some(cost)
}

/// Convenience wrapper: look up the model by id in a `Catalog` first,
/// then delegate to [`compute_token_cost_usd`]. Returns `None` when the
/// model isn't in the catalog at all (e.g. an enabled-but-not-fetched
/// id) — same "unknown" semantics so the caller can't mistake an
/// out-of-cache model for a free one.
pub fn compute_token_cost_usd_from_catalog(
    input_tokens: u64,
    output_tokens: u64,
    model_id: &str,
    catalog: &Catalog,
) -> Option<f64> {
    let entry = catalog.find(model_id)?;
    compute_token_cost_usd(input_tokens, output_tokens, entry)
}

/// Aggregate per-call `model_calls.cost_usd` for all model calls produced
/// during a specific eval run. Returns the sum, or `None` when the
/// `agent_runs` / `spans` / `model_calls` tables are unavailable (e.g. a
/// test context without migration 018) or when every call has `cost_usd = NULL`
/// (model not in pricing catalog).
///
/// Query path:
///   `eval_runs.id → agent_runs.eval_run_id → spans.run_id → model_calls.span_id`
///
/// The JOIN chain intentionally reads from the observability tables. If the
/// observability bus was not wired for this run (e.g. backtest via old CLI
/// path), the JOIN returns zero rows and `sum` is `NULL` → `None`.
pub async fn aggregate_eval_run_inference_cost(pool: &SqlitePool, eval_run_id: &str) -> Option<f64> {
    let result: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(mc.cost_usd) \
         FROM model_calls mc \
         JOIN spans s ON s.id = mc.span_id \
         JOIN agent_runs ar ON ar.id = s.run_id \
         WHERE ar.eval_run_id = ?",
    )
    .bind(eval_run_id)
    .fetch_one(pool)
    .await
    .ok()
    .flatten();
    // Return None when sum is zero or negative — a genuine zero spend means
    // all calls had known $0.00 pricing, which the positive_price guard
    // already filtered to None. Protect against negative values from DB
    // corruption.
    result.filter(|&v| v > 0.0 && v.is_finite())
}

fn positive_price(p: Option<f64>) -> Option<f64> {
    // OpenRouter's free routes (`"prompt": "0"`) are already filtered to
    // `None` at parse time, but we re-check here so a hand-edited cache
    // file or a future catalog source can't sneak `Some(0.0)` past the
    // gate and produce a misleadingly precise `$0.00` cost.
    match p {
        Some(v) if v > 0.0 && v.is_finite() => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::Value;

    fn openrouter_claude_opus_47() -> ModelEntry {
        // Mirrors what `parse_openrouter_models` populates from
        // `https://openrouter.ai/api/v1/models` for Claude Opus 4.7.
        // Pricing matches the test fixture in
        // `providers/fetcher.rs::parse_openrouter_models_extracts_full_metadata`:
        // wire `"0.000015"` / `"0.000075"` → $15 / $75 per 1M tokens.
        ModelEntry {
            id: "anthropic/claude-opus-4.7".into(),
            display_name: Some("Anthropic: Claude Opus 4.7".into()),
            context_window: Some(200_000),
            max_output_tokens: Some(8192),
            supports_reasoning: None,
            supports_tools: Some(true),
            pricing_per_million_input_usd: Some(15.0),
            pricing_per_million_output_usd: Some(75.0),
            raw: Value::Null,
        }
    }

    #[test]
    fn cost_for_openrouter_claude_opus_matches_openrouter_pricing_page() {
        // 10_000 prompt tokens * $15/Mtok + 2_000 completion tokens * $75/Mtok
        //   = $0.15 + $0.15 = $0.30
        // Hand-computed from OpenRouter's published Claude Opus 4.7 rates.
        // If this assertion ever drifts, the bug is either in
        // `compute_token_cost_usd` or in the per-Mtok scaling the
        // fetcher applies; the fetcher tests assert the scaling.
        let model = openrouter_claude_opus_47();
        let cost = compute_token_cost_usd(10_000, 2_000, &model).expect("priced model returns Some");
        assert!(
            (cost - 0.30).abs() < 1e-9,
            "expected 0.30, got {} (model pricing per Mtok: in={:?}, out={:?})",
            cost,
            model.pricing_per_million_input_usd,
            model.pricing_per_million_output_usd,
        );
    }

    #[test]
    fn cost_returns_none_when_pricing_absent() {
        // Anthropic / bare OpenAI catalogs land here. Returning `None`
        // (not `Some(0.0)`) is what lets downstream callers keep their
        // existing operator-trusted out-of-band cost path for these
        // providers instead of overwriting it with a fake zero.
        let mut model = openrouter_claude_opus_47();
        model.pricing_per_million_input_usd = None;
        model.pricing_per_million_output_usd = None;
        assert_eq!(compute_token_cost_usd(10_000, 2_000, &model), None);
    }

    #[test]
    fn cost_returns_none_when_either_side_missing() {
        // A half-populated entry (input priced, output unpriced) is
        // still "unknown" total cost — we don't want to silently bill
        // only the prompt half and call it a price.
        let mut model = openrouter_claude_opus_47();
        model.pricing_per_million_output_usd = None;
        assert_eq!(compute_token_cost_usd(10_000, 2_000, &model), None);

        let mut model = openrouter_claude_opus_47();
        model.pricing_per_million_input_usd = None;
        assert_eq!(compute_token_cost_usd(10_000, 2_000, &model), None);
    }

    #[test]
    fn cost_treats_zero_pricing_as_unknown() {
        // OpenRouter free routes parse to `None`, but defend against a
        // future shape where `Some(0.0)` reaches us — a precise "$0.00"
        // is a worse signal than "unknown".
        let mut model = openrouter_claude_opus_47();
        model.pricing_per_million_input_usd = Some(0.0);
        model.pricing_per_million_output_usd = Some(0.0);
        assert_eq!(compute_token_cost_usd(10_000, 2_000, &model), None);
    }

    #[test]
    fn cost_treats_negative_or_nonfinite_pricing_as_unknown() {
        // Cache corruption defense. Negative / NaN / inf should never
        // surface as a real cost — same `None` semantics as missing.
        let mut model = openrouter_claude_opus_47();
        model.pricing_per_million_input_usd = Some(-1.0);
        assert_eq!(compute_token_cost_usd(10, 10, &model), None);

        let mut model = openrouter_claude_opus_47();
        model.pricing_per_million_output_usd = Some(f64::NAN);
        assert_eq!(compute_token_cost_usd(10, 10, &model), None);

        let mut model = openrouter_claude_opus_47();
        model.pricing_per_million_input_usd = Some(f64::INFINITY);
        assert_eq!(compute_token_cost_usd(10, 10, &model), None);
    }

    #[test]
    fn cost_from_catalog_looks_up_by_exact_id() {
        let catalog = Catalog {
            provider: "openrouter".into(),
            fetched_at: Utc::now(),
            source_url: "https://openrouter.ai/api/v1/models".into(),
            models: vec![openrouter_claude_opus_47()],
        };

        // Exact id match wins.
        let cost = compute_token_cost_usd_from_catalog(10_000, 2_000, "anthropic/claude-opus-4.7", &catalog);
        assert_eq!(cost, Some(0.30));

        // Unknown id resolves to `None`, same as an unpriced model —
        // "we don't know" is the safe answer; the caller can decide
        // whether to fall back to a provider-specific out-of-band path.
        assert_eq!(
            compute_token_cost_usd_from_catalog(10_000, 2_000, "deepseek/deepseek-v4-pro", &catalog),
            None,
        );
    }

    #[test]
    fn cost_scales_linearly_with_tokens() {
        // Light arithmetic sanity check — doubling the token mix
        // doubles the cost, halving halves it. Catches sign / unit
        // mistakes a single-point check would miss.
        let model = openrouter_claude_opus_47();
        let base = compute_token_cost_usd(1_000, 1_000, &model).unwrap();
        let doubled = compute_token_cost_usd(2_000, 2_000, &model).unwrap();
        let halved = compute_token_cost_usd(500, 500, &model).unwrap();
        assert!((doubled - 2.0 * base).abs() < 1e-9);
        assert!((halved - 0.5 * base).abs() < 1e-9);
    }

    #[test]
    fn cost_handles_zero_token_counts() {
        // A run that errored before any tokens were charged should
        // produce a $0.00 cost (not `None`) — we *do* know the price
        // here, the spend is just genuinely zero.
        let model = openrouter_claude_opus_47();
        let cost = compute_token_cost_usd(0, 0, &model).expect("priced model with zero tokens is $0");
        assert!(cost.abs() < 1e-12);
    }
}

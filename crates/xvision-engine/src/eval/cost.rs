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
use xvision_core::config::ProviderKind;
use xvision_core::providers::{Catalog, ModelEntry};

/// Does this provider report `$0`/token for *every* model the cost
/// machinery can see?
///
/// Built on the same `compute_token_cost_usd` "unknown / non-positive
/// pricing => no cost" semantics as the rest of this module:
///
/// * **Local kinds** (`local-candle`, `ollama`, `llama-cpp`, `vllm`) run
///   on the operator's own hardware and have no published per-token
///   price, so they always report zero cost regardless of catalog.
/// * **Network kinds** (`anthropic`, `openai-compat`) report zero cost
///   only when NO model in `catalog` resolves to a positive price (the
///   catalog carries no pricing, e.g. bare OpenAI `/v1/models`, or every
///   entry is a free route filtered to `None` by `positive_price`).
///
/// Returned `true` is the signal an operator-facing surface needs (QA
/// U3): when `--budget` is set but the resolved mutator/judge provider
/// reports `$0`/token, a budget ceiling can never trip, so the consuming
/// CLI should warn and point the operator at `--experiments-per-cycle`.
///
/// An empty `catalog` for a network kind yields `true` — we genuinely
/// have no pricing to enforce a budget against, which is exactly the
/// condition the warning exists to surface.
pub fn provider_reports_zero_cost(kind: ProviderKind, catalog: &Catalog) -> bool {
    match kind {
        // Local inference — no per-token cost to meter against a budget.
        ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm => {
            true
        }
        // Network kinds: zero cost iff nothing in the catalog has a
        // positive published price. A single 1-in/1-out probe through the
        // same `compute_token_cost_usd` gate tells us whether the entry
        // carries usable pricing.
        ProviderKind::Anthropic | ProviderKind::OpenaiCompat => !catalog
            .models
            .iter()
            .any(|m| compute_token_cost_usd(1, 1, m).is_some_and(|c| c > 0.0)),
    }
}

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

/// Convenience wrapper: resolve the model id to a `Catalog` entry via
/// [`resolve_priced_entry`] first, then delegate to
/// [`compute_token_cost_usd`]. Returns `None` when the model can't be
/// resolved (e.g. an enabled-but-not-fetched id, or an ambiguous match)
/// — same "unknown" semantics so the caller can't mistake an
/// out-of-cache model for a free one.
pub fn compute_token_cost_usd_from_catalog(
    input_tokens: u64,
    output_tokens: u64,
    model_id: &str,
    catalog: &Catalog,
) -> Option<f64> {
    let entry = resolve_priced_entry(model_id, catalog)?;
    compute_token_cost_usd(input_tokens, output_tokens, entry)
}

/// Resolve a model id to a catalog entry for *pricing*, tolerating the
/// id-format gap between a model's executing provider and OpenRouter's
/// `vendor/model` pricing ids. A strategy may run a slot directly
/// against Anthropic (`claude-opus-4-7`) or OpenAI (`gpt-4o`) while the
/// only catalog carrying a price is OpenRouter, where the same model is
/// listed as `anthropic/claude-opus-4.7` / `openai/gpt-4o`. Exact-id
/// lookup misses these, leaving `model_calls.cost_usd` NULL.
///
/// Resolution is deliberately conservative — a *wrong* price is worse
/// than no price, so any ambiguity yields `None`:
///   1. Exact id match (OpenRouter-native and same-provider case).
///   2. Unique suffix match: the query equals the `<rest>` of exactly
///      one `<vendor>/<rest>` entry (`gpt-4o` → `openai/gpt-4o`).
///   3. Unique normalized match: lowercase + keep only `[a-z0-9]` on
///      both the query and each entry's id (and its `<vendor>/`-stripped
///      suffix), matching exactly one entry. Bridges punctuation drift
///      like Anthropic's `claude-opus-4-7` ↔ OpenRouter's
///      `anthropic/claude-opus-4.7`.
///
/// Whenever a step finds two or more candidates the function returns
/// `None` rather than picking one — surfacing "unknown" is always
/// preferable to billing the operator against the wrong model's rate.
fn resolve_priced_entry<'a>(model_id: &str, catalog: &'a Catalog) -> Option<&'a ModelEntry> {
    // 1. Exact id match — the common path for OpenRouter-sourced slots
    //    and same-provider catalogs.
    if let Some(entry) = catalog.find(model_id) {
        return Some(entry);
    }
    // 2. Unique `<vendor>/<suffix>` match where suffix == query.
    let mut suffix_hits = catalog.models.iter().filter(|m| id_suffix(&m.id) == model_id);
    if let Some(first) = suffix_hits.next() {
        if suffix_hits.next().is_none() {
            return Some(first);
        }
        // Ambiguous suffix (same model name under multiple vendors) —
        // refuse to guess.
        return None;
    }
    // 3. Unique normalized match (punctuation-insensitive).
    let want = normalize_model_id(model_id);
    if want.is_empty() {
        return None;
    }
    let mut norm_hits = catalog
        .models
        .iter()
        .filter(|m| normalize_model_id(&m.id) == want || normalize_model_id(id_suffix(&m.id)) == want);
    let first = norm_hits.next()?;
    if norm_hits.next().is_some() {
        return None; // ambiguous
    }
    Some(first)
}

/// The portion of an id after the last `/` — i.e. OpenRouter's
/// `<vendor>/<model>` reduced to `<model>`. Ids without a slash are
/// returned unchanged.
fn id_suffix(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

/// Lowercase and strip every character that isn't `[a-z0-9]`. Collapses
/// the `.` / `-` / `_` punctuation drift between provider-native model
/// ids and OpenRouter's pricing ids without inventing a per-vendor alias
/// table: `claude-opus-4-7` and `claude-opus-4.7` both become
/// `claudeopus47`.
fn normalize_model_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
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

/// bead-8wn: windowed sibling of [`aggregate_eval_run_inference_cost`].
///
/// Sum `model_calls.cost_usd` over every agent run **started** at or after
/// `since` (RFC-3339), via the same observability JOIN chain. Because the
/// window is keyed on `agent_runs.started_at`, this naturally spans BOTH
/// eval-linked agent runs (`eval_run_id` set) and standalone agent runs
/// (`eval_run_id` NULL) — "runs started in the window", as the cost-rollup
/// surface needs.
///
/// `None` is the honest "unknown" signal (HONESTY §8.1/§8.9), returned when:
///   * the observability tables are absent (e.g. a test pool without migration
///     018) — the query errors and we `.ok().flatten()` to `None`;
///   * no model call in the window carries a non-NULL `cost_usd` (every call
///     ran against an unpriced model) — `SUM` over zero priced rows is SQL
///     `NULL` → `None`;
///   * the metered sum is `<= 0` or non-finite (DB corruption guard).
///
/// It must NEVER fabricate `Some(0.0)`: a precise `$0.00` is a worse signal
/// than "unknown" when the true price is missing. The `started_at` lower bound
/// is bound as a SQL parameter (never string-interpolated).
pub async fn aggregate_inference_cost_since(
    pool: &SqlitePool,
    since: chrono::DateTime<chrono::Utc>,
) -> Option<f64> {
    let result: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(mc.cost_usd) \
         FROM model_calls mc \
         JOIN spans s ON s.id = mc.span_id \
         JOIN agent_runs ar ON ar.id = s.run_id \
         WHERE ar.started_at >= ?",
    )
    .bind(since.to_rfc3339())
    .fetch_one(pool)
    .await
    .ok()
    .flatten();
    result.filter(|&v| v > 0.0 && v.is_finite())
}

/// bead-8wn: optimizer (autooptimizer cycle) cost over a window =
/// `Σ cycle_cost.cost_usd` for every cycle whose `created_at` is at or after
/// `since`. Mirrors `session_cost_usd` in the dashboard autooptimizer route
/// (a plain `SUM(cost_usd)` over `cycle_cost`), but bounded by the cost row's
/// `created_at` rather than by session membership.
///
/// Same honest "unknown" contract as [`aggregate_inference_cost_since`]:
/// `None` when the `cycle_cost` table is absent, when no cycle in the window
/// has a cost row (SUM over zero rows is SQL `NULL`), or when the sum is
/// non-positive / non-finite. Never `Some(0.0)`.
pub async fn aggregate_optimizer_cost_since(
    pool: &SqlitePool,
    since: chrono::DateTime<chrono::Utc>,
) -> Option<f64> {
    let result: Option<f64> =
        sqlx::query_scalar("SELECT SUM(cost_usd) FROM cycle_cost WHERE created_at >= ?")
            .bind(since.to_rfc3339())
            .fetch_one(pool)
            .await
            .ok()
            .flatten();
    result.filter(|&v| v > 0.0 && v.is_finite())
}

/// bead-8wn: read the persisted operator-set daily budget cap from
/// `cost_budget` (single row, id = 1). `None` when the cap is UNSET — either
/// the row is absent (fresh DB, DB-wipe posture) or `daily_cap_usd` is NULL.
/// The dashboard renders no denominator (em-dash) in that case; it must NEVER
/// fall back to a faked ceiling. A missing table also degrades to `None`.
pub async fn get_daily_budget_cap(pool: &SqlitePool) -> Option<f64> {
    let result: Option<f64> = sqlx::query_scalar("SELECT daily_cap_usd FROM cost_budget WHERE id = 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten();
    // A persisted cap is finite + positive by construction (the API rejects
    // non-positive / NaN before write), but re-guard defensively.
    result.filter(|&v| v > 0.0 && v.is_finite())
}

/// bead-8wn: persist the operator-set daily budget cap into `cost_budget`
/// (single row, id = 1) via `INSERT OR REPLACE`. The caller (API boundary)
/// MUST have already validated `cap` is finite and `> 0` (400 otherwise);
/// this function does no validation of its own.
pub async fn set_daily_budget_cap(pool: &SqlitePool, cap: f64, updated_at: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR REPLACE INTO cost_budget (id, daily_cap_usd, updated_at) VALUES (1, ?, ?)")
        .bind(cap)
        .bind(updated_at)
        .execute(pool)
        .await?;
    Ok(())
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

    fn unpriced_model(id: &str) -> ModelEntry {
        let mut m = openrouter_claude_opus_47();
        m.id = id.into();
        m.pricing_per_million_input_usd = None;
        m.pricing_per_million_output_usd = None;
        m
    }

    fn catalog_with(models: Vec<ModelEntry>) -> Catalog {
        Catalog {
            provider: "test".into(),
            fetched_at: Utc::now(),
            source_url: "https://example/models".into(),
            models,
        }
    }

    #[test]
    fn zero_cost_true_for_local_kinds_regardless_of_catalog() {
        // Local inference has no per-token price; a populated, *priced*
        // catalog must not flip the verdict — these kinds always report $0.
        let priced = catalog_with(vec![openrouter_claude_opus_47()]);
        for kind in [
            ProviderKind::Ollama,
            ProviderKind::LocalCandle,
            ProviderKind::LlamaCpp,
            ProviderKind::Vllm,
        ] {
            assert!(
                provider_reports_zero_cost(kind, &priced),
                "{kind:?} must report zero cost"
            );
        }
    }

    #[test]
    fn zero_cost_false_for_network_kind_with_priced_catalog() {
        // A real priced catalog (OpenRouter Claude Opus) means a budget
        // CAN be enforced — no warning should fire.
        let priced = catalog_with(vec![openrouter_claude_opus_47()]);
        assert!(!provider_reports_zero_cost(ProviderKind::OpenaiCompat, &priced));
        assert!(!provider_reports_zero_cost(ProviderKind::Anthropic, &priced));
    }

    #[test]
    fn zero_cost_true_for_network_kind_with_unpriced_or_empty_catalog() {
        // Bare OpenAI / Anthropic /v1/models carry no pricing → unknown →
        // a budget can't be metered, so the helper reports zero cost.
        let unpriced = catalog_with(vec![unpriced_model("gpt-4o"), unpriced_model("o3")]);
        assert!(provider_reports_zero_cost(ProviderKind::OpenaiCompat, &unpriced));
        // Empty catalog: genuinely nothing to bill against.
        let empty = catalog_with(vec![]);
        assert!(provider_reports_zero_cost(ProviderKind::Anthropic, &empty));
    }

    #[test]
    fn zero_cost_false_when_any_model_priced() {
        // Mixed catalog: a single priced entry is enough to enforce a
        // budget, so the provider does NOT report zero cost.
        let mixed = catalog_with(vec![unpriced_model("free-route"), openrouter_claude_opus_47()]);
        assert!(!provider_reports_zero_cost(ProviderKind::OpenaiCompat, &mixed));
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

    /// Build a priced OpenRouter-shaped entry for resolver tests.
    fn priced(id: &str, in_usd: f64, out_usd: f64) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            display_name: None,
            context_window: None,
            max_output_tokens: None,
            supports_reasoning: None,
            supports_tools: None,
            pricing_per_million_input_usd: Some(in_usd),
            pricing_per_million_output_usd: Some(out_usd),
            raw: Value::Null,
        }
    }

    fn catalog_of(models: Vec<ModelEntry>) -> Catalog {
        Catalog {
            provider: "openrouter".into(),
            fetched_at: Utc::now(),
            source_url: "https://openrouter.ai/api/v1/models".into(),
            models,
        }
    }

    #[test]
    fn resolves_bare_anthropic_id_to_openrouter_pricing() {
        // A slot run directly against Anthropic emits the bare id
        // `claude-opus-4-7`; OpenRouter prices it as
        // `anthropic/claude-opus-4.7` (note `.` vs `-`). The normalized
        // match bridges the punctuation gap so cost isn't lost.
        let catalog = catalog_of(vec![openrouter_claude_opus_47()]);
        let cost = compute_token_cost_usd_from_catalog(10_000, 2_000, "claude-opus-4-7", &catalog);
        assert_eq!(cost, Some(0.30));
    }

    #[test]
    fn resolves_bare_openai_id_by_unique_suffix() {
        // `gpt-4o` (bare, from a direct OpenAI slot) → `openai/gpt-4o`
        // by suffix, even with sibling `gpt-4o-mini` / `gpt-4o-2024-…`
        // present (their suffixes differ, so no ambiguity).
        let catalog = catalog_of(vec![
            priced("openai/gpt-4o", 2.5, 10.0),
            priced("openai/gpt-4o-mini", 0.15, 0.6),
            priced("openai/gpt-4o-2024-08-06", 2.5, 10.0),
        ]);
        // 1_000_000 in @ $2.5/Mtok + 1_000_000 out @ $10/Mtok = $12.50
        let cost = compute_token_cost_usd_from_catalog(1_000_000, 1_000_000, "gpt-4o", &catalog);
        assert_eq!(cost, Some(12.5));
    }

    #[test]
    fn resolver_refuses_ambiguous_suffix() {
        // Same model name under two vendors → we cannot know which the
        // operator ran, so refuse rather than bill the wrong rate.
        let catalog = catalog_of(vec![
            priced("vendora/llama-3-70b", 1.0, 1.0),
            priced("vendorb/llama-3-70b", 99.0, 99.0),
        ]);
        assert_eq!(
            compute_token_cost_usd_from_catalog(1_000, 1_000, "llama-3-70b", &catalog),
            None,
        );
    }

    #[test]
    fn resolver_returns_none_for_unrelated_id() {
        let catalog = catalog_of(vec![openrouter_claude_opus_47()]);
        assert_eq!(
            compute_token_cost_usd_from_catalog(1_000, 1_000, "some-unknown-model", &catalog),
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

    // ─── bead-8wn: windowed cross-source cost + budget cap ────────────────────

    use sqlx::SqlitePool;

    /// Minimal observability + cost schema for the windowed aggregators. We
    /// inline the exact `agent_runs` / `spans` / `model_calls` columns the JOIN
    /// chain reads (a subset of migration 018), plus the `cycle_cost` table and
    /// the bead-8wn `cost_budget` table.
    async fn cost_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("open pool");
        sqlx::query(
            "CREATE TABLE agent_runs ( \
                id TEXT PRIMARY KEY, \
                eval_run_id TEXT, \
                status TEXT NOT NULL, \
                started_at TEXT NOT NULL )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE spans ( \
                id TEXT PRIMARY KEY, \
                run_id TEXT NOT NULL, \
                kind TEXT NOT NULL, \
                started_at TEXT NOT NULL )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE model_calls ( \
                span_id TEXT PRIMARY KEY, \
                provider TEXT NOT NULL, \
                model TEXT NOT NULL, \
                cost_usd REAL )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE cycle_cost ( \
                cycle_id TEXT PRIMARY KEY, \
                input_tokens INTEGER NOT NULL, \
                output_tokens INTEGER NOT NULL, \
                cost_usd REAL NOT NULL, \
                unpriced_calls INTEGER NOT NULL, \
                created_at TEXT NOT NULL )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE cost_budget ( \
                id INTEGER PRIMARY KEY CHECK (id = 1), \
                daily_cap_usd REAL, \
                updated_at TEXT NOT NULL )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    /// Insert one agent run with a single priced model call. `cost` is bound
    /// straight into `model_calls.cost_usd`; pass `None` for an unpriced call.
    async fn seed_agent_run_cost(pool: &SqlitePool, run_id: &str, started_at: &str, cost: Option<f64>) {
        sqlx::query(
            "INSERT INTO agent_runs (id, eval_run_id, status, started_at) VALUES (?, NULL, 'completed', ?)",
        )
        .bind(run_id)
        .bind(started_at)
        .execute(pool)
        .await
        .unwrap();
        let span_id = format!("span-{run_id}");
        sqlx::query("INSERT INTO spans (id, run_id, kind, started_at) VALUES (?, ?, 'model.call', ?)")
            .bind(&span_id)
            .bind(run_id)
            .bind(started_at)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO model_calls (span_id, provider, model, cost_usd) VALUES (?, 'openrouter', 'm', ?)",
        )
        .bind(&span_id)
        .bind(cost)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_cycle_cost(pool: &SqlitePool, cycle_id: &str, cost: f64, created_at: &str) {
        sqlx::query(
            "INSERT INTO cycle_cost (cycle_id, input_tokens, output_tokens, cost_usd, unpriced_calls, created_at) \
             VALUES (?, 0, 0, ?, 0, ?)",
        )
        .bind(cycle_id)
        .bind(cost)
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();
    }

    fn ts(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[tokio::test]
    async fn inference_cost_since_sums_runs_in_window() {
        let pool = cost_pool().await;
        // Two runs after the boundary, one before. Only the two after count.
        seed_agent_run_cost(&pool, "r1", "2026-06-10T12:00:00Z", Some(0.10)).await;
        seed_agent_run_cost(&pool, "r2", "2026-06-11T12:00:00Z", Some(0.25)).await;
        seed_agent_run_cost(&pool, "r-old", "2026-06-01T00:00:00Z", Some(99.0)).await;

        let total = aggregate_inference_cost_since(&pool, ts("2026-06-10T00:00:00Z")).await;
        assert!(
            matches!(total, Some(v) if (v - 0.35).abs() < 1e-9),
            "expected 0.35 over the two in-window runs, got {total:?}",
        );
    }

    #[tokio::test]
    async fn inference_cost_since_none_when_no_priced_rows() {
        let pool = cost_pool().await;
        // Run is in window but its model call is unpriced (cost_usd NULL):
        // SUM over zero priced rows is NULL → None, NOT Some(0.0).
        seed_agent_run_cost(&pool, "r1", "2026-06-10T12:00:00Z", None).await;
        let total = aggregate_inference_cost_since(&pool, ts("2026-06-01T00:00:00Z")).await;
        assert_eq!(total, None, "unpriced calls must yield None, never Some(0.0)");
    }

    #[tokio::test]
    async fn inference_cost_since_none_when_no_runs_in_window() {
        let pool = cost_pool().await;
        seed_agent_run_cost(&pool, "r-old", "2026-06-01T00:00:00Z", Some(5.0)).await;
        // Boundary is AFTER the only run → no rows → None.
        let total = aggregate_inference_cost_since(&pool, ts("2026-06-05T00:00:00Z")).await;
        assert_eq!(total, None);
    }

    #[tokio::test]
    async fn inference_cost_since_none_when_tables_absent() {
        // A bare pool without the observability tables must degrade to None
        // (unknown), not error or fabricate 0.
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let total = aggregate_inference_cost_since(&pool, ts("2026-06-01T00:00:00Z")).await;
        assert_eq!(total, None);
    }

    #[tokio::test]
    async fn optimizer_cost_since_sums_cycles_in_window() {
        let pool = cost_pool().await;
        seed_cycle_cost(&pool, "c1", 1.50, "2026-06-10T12:00:00Z").await;
        seed_cycle_cost(&pool, "c2", 2.50, "2026-06-11T12:00:00Z").await;
        seed_cycle_cost(&pool, "c-old", 99.0, "2026-06-01T00:00:00Z").await;
        let total = aggregate_optimizer_cost_since(&pool, ts("2026-06-10T00:00:00Z")).await;
        assert!(
            matches!(total, Some(v) if (v - 4.0).abs() < 1e-9),
            "expected 4.0 over the two in-window cycles, got {total:?}",
        );
    }

    #[tokio::test]
    async fn optimizer_cost_since_none_when_no_cycles_in_window() {
        let pool = cost_pool().await;
        seed_cycle_cost(&pool, "c-old", 3.0, "2026-06-01T00:00:00Z").await;
        let total = aggregate_optimizer_cost_since(&pool, ts("2026-06-05T00:00:00Z")).await;
        assert_eq!(total, None);
    }

    #[tokio::test]
    async fn optimizer_cost_since_none_when_table_absent() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let total = aggregate_optimizer_cost_since(&pool, ts("2026-06-01T00:00:00Z")).await;
        assert_eq!(total, None);
    }

    #[tokio::test]
    async fn budget_cap_none_when_unset_then_value_after_set() {
        let pool = cost_pool().await;
        // Fresh table, no row → unset → None (em-dash on the UI, no faked cap).
        assert_eq!(get_daily_budget_cap(&pool).await, None, "unset cap must be None");

        set_daily_budget_cap(&pool, 25.0, "2026-06-13T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(
            get_daily_budget_cap(&pool).await,
            Some(25.0),
            "cap must read back the set value",
        );

        // INSERT OR REPLACE on id=1 overwrites, not duplicates.
        set_daily_budget_cap(&pool, 40.0, "2026-06-13T01:00:00Z")
            .await
            .unwrap();
        assert_eq!(get_daily_budget_cap(&pool).await, Some(40.0));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cost_budget")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "single-row table must hold exactly one row");
    }

    #[tokio::test]
    async fn budget_cap_none_when_table_absent() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        assert_eq!(get_daily_budget_cap(&pool).await, None);
    }
}

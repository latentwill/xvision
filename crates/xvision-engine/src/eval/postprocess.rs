//! Post-finalize hooks for an eval run.
//!
//! v1 has one hook: findings extraction. The executor finalizes the run
//! (metrics + equity + decisions persisted), and the orchestration layer
//! (`api::eval::run_inner`) calls `extract_and_record` to drive the
//! findings extractor against the finalized state, persist each finding
//! via `RunStore::record_finding`, and refresh the search index.
//!
//! **Best-effort by design.** Extractor failures (LLM timeout, parse
//! error, empty response) MUST NOT fail the run — metrics + equity are
//! the load-bearing artifacts. Failures log at `warn!` and the function
//! returns `Ok(0)`. The single audit row records the actual outcome
//! (ok or error) so the eod report still surfaces the failure.
//!
//! Why this lives here, not in the executor: keeping `Executor::run`
//! free of `&ApiContext` lets the trait stay focused on "drive a strategy
//! through a scenario." Composition (extract → record → index) is the
//! orchestration layer's job.

use std::sync::Arc;
use std::time::Instant;

use serde_json::json;
use xvision_core::config::{ProviderEntry, ProviderKind};

use crate::agent::llm::LlmDispatch;
use crate::api::audit::{self, Outcome};
use crate::api::{search as api_search, ApiContext};
use crate::eval::findings::extractor::extract_findings;
use crate::eval::store::{DecisionRow, RunStore};

/// Default model for the v1 findings extractor when the eval's provider
/// is Anthropic-native. Cheap + fast — the extractor is summarization-
/// shaped, not deep reasoning. Other provider kinds map via
/// [`findings_model_for_provider`] (e.g. OpenRouter needs the
/// `anthropic/claude-haiku-4.5` slug; sending the Anthropic-native id
/// returns 400).
pub const DEFAULT_FINDINGS_MODEL: &str = "claude-haiku-4-5-20251001";

/// OpenRouter's slug for the same Claude Haiku 4.5 model. The
/// findings-postprocess dispatch is shared with the eval's trader
/// runtime; routing the Anthropic-native id through OpenRouter's
/// `/v1/chat/completions` returns 400 ("not a valid model ID").
const OPENROUTER_FINDINGS_MODEL: &str = "anthropic/claude-haiku-4.5";

/// Pick a findings-extractor model id for the eval's resolved provider.
/// Best-effort — when the provider's shape is unknown we fall back to
/// the operator's first enabled model (already vetted for this
/// provider), then to [`DEFAULT_FINDINGS_MODEL`] as a last resort.
pub fn findings_model_for_provider(entry: &ProviderEntry) -> String {
    match entry.kind {
        ProviderKind::Anthropic => DEFAULT_FINDINGS_MODEL.to_string(),
        ProviderKind::OpenaiCompat | ProviderKind::Vllm => {
            if entry.base_url.contains("openrouter.ai") {
                OPENROUTER_FINDINGS_MODEL.to_string()
            } else {
                entry
                    .enabled_models
                    .first()
                    .cloned()
                    .unwrap_or_else(|| DEFAULT_FINDINGS_MODEL.to_string())
            }
        }
        ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::LocalCandle => entry
            .enabled_models
            .first()
            .cloned()
            .unwrap_or_else(|| DEFAULT_FINDINGS_MODEL.to_string()),
    }
}

/// Drive findings extraction against a finalized run. Audited as
/// `eval/extract_findings`. Returns the count of persisted findings on
/// success; returns `Ok(0)` (and logs `warn!`) on every failure mode so
/// the calling executor / API path is never blocked.
pub async fn extract_and_record(
    ctx: &ApiContext,
    run_id: &str,
    dispatch: Arc<dyn LlmDispatch>,
    model: &str,
) -> usize {
    let started = Instant::now();
    let result = extract_and_record_inner(ctx, run_id, dispatch, model).await;

    let (outcome, count) = match &result {
        Ok(n) => (Outcome::Ok, *n),
        Err(e) => {
            tracing::warn!(error = %e, run_id, "findings postprocess failed (run still ok)");
            (Outcome::Error(e.to_string()), 0)
        }
    };
    let _ = audit::record(
        ctx,
        "eval",
        "extract_findings",
        Some(run_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    count
}

async fn extract_and_record_inner(
    ctx: &ApiContext,
    run_id: &str,
    dispatch: Arc<dyn LlmDispatch>,
    model: &str,
) -> anyhow::Result<usize> {
    let store = RunStore::new(ctx.db.clone());

    let run = store.get(run_id).await?;
    let decisions = store.read_decisions(run_id).await.unwrap_or_default();
    let equity = store.read_equity_curve(run_id).await.unwrap_or_default();

    let decisions_summary = summarise_decisions(&decisions);
    let equity_summary = summarise_equity(&equity);

    let findings = extract_findings(&run, decisions_summary, equity_summary, dispatch, model).await?;

    let mut persisted = 0usize;
    for f in findings {
        match store.record_finding(&f).await {
            Ok(()) => {
                if let Err(e) = api_search::upsert_finding(ctx, &f).await {
                    tracing::warn!(error = %e, run_id, finding_id = %f.id, "search index upsert (finding) failed");
                }
                persisted += 1;
            }
            Err(e) => {
                tracing::warn!(error = %e, run_id, finding_id = %f.id, "record_finding failed");
            }
        }
    }
    Ok(persisted)
}

/// Compact decision summary for the extractor prompt. Bounded size so a
/// long run doesn't push the LLM context window. We surface counts +
/// per-asset action histograms + a few headline numbers — enough for the
/// extractor to spot regime/overtrading/win-rate patterns.
fn summarise_decisions(rows: &[DecisionRow]) -> serde_json::Value {
    if rows.is_empty() {
        return json!({"n_decisions": 0});
    }

    let mut by_action: std::collections::BTreeMap<&str, u32> = Default::default();
    let mut realized_sum = 0.0_f64;
    let mut realized_count = 0u32;
    let mut wins = 0u32;
    let mut losses = 0u32;

    for r in rows {
        *by_action.entry(r.action.as_str()).or_default() += 1;
        if let Some(p) = r.pnl_realized {
            realized_sum += p;
            realized_count += 1;
            if p > 0.0 {
                wins += 1;
            } else if p < 0.0 {
                losses += 1;
            }
        }
    }

    let assets: std::collections::BTreeSet<&str> = rows.iter().map(|r| r.asset.as_str()).collect();
    let win_rate = if realized_count > 0 {
        wins as f64 / realized_count as f64
    } else {
        0.0
    };

    json!({
        "n_decisions": rows.len(),
        "by_action": by_action,
        "assets": assets.into_iter().collect::<Vec<_>>(),
        "realized_pnl_sum": realized_sum,
        "wins": wins,
        "losses": losses,
        "win_rate": win_rate,
    })
}

/// Compact equity-curve summary: start/end/min/max/peak-to-trough drop.
/// Only first + last + extrema, not the full series — the extractor only
/// needs shape, not every sample.
fn summarise_equity(curve: &[(chrono::DateTime<chrono::Utc>, f64)]) -> serde_json::Value {
    if curve.is_empty() {
        return json!({"n_samples": 0});
    }
    let start = curve.first().map(|(_, v)| *v).unwrap_or(0.0);
    let end = curve.last().map(|(_, v)| *v).unwrap_or(0.0);
    let min = curve.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
    let max = curve.iter().map(|(_, v)| *v).fold(f64::NEG_INFINITY, f64::max);

    // Peak-to-trough max drawdown — running peak then largest drop below it.
    let mut peak = f64::NEG_INFINITY;
    let mut max_dd = 0.0_f64;
    for (_, v) in curve {
        if *v > peak {
            peak = *v;
        }
        let dd = (peak - *v) / peak.max(1.0);
        if dd > max_dd {
            max_dd = dd;
        }
    }

    json!({
        "n_samples": curve.len(),
        "start": start,
        "end": end,
        "min": min,
        "max": max,
        "max_drawdown_pct": max_dd * 100.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::MockDispatch;
    use crate::api::Actor;
    use crate::eval::run::{MetricsSummary, Run, RunMode};
    use chrono::Utc;

    async fn fresh_ctx() -> (ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
            .await
            .unwrap();
        (ctx, dir)
    }

    // Returns a freshly-queued Run alongside the metrics the caller will pass
    // into `RunStore::finalize`. The Run itself has `metrics: None` so the DB
    // row created by `store.create(&run)` matches production — the queued row
    // never has `metrics_json` populated until `finalize` writes it. Tests
    // pass the returned metrics into `store.finalize(...)` to drive the
    // queued → completed transition, which is the actual code path under
    // test.
    fn queued_run() -> (Run, MetricsSummary) {
        let run = Run::new_queued(
            "strategy-h".into(),
            "crypto-bull-q1-2025".into(),
            RunMode::Backtest,
        );
        let metrics = MetricsSummary {
            total_return_pct: -3.2,
            sharpe: -0.4,
            max_drawdown_pct: 18.0,
            win_rate: 0.41,
            n_trades: 12,
            n_decisions: 30,
            baselines: None,
            ..Default::default()
        };
        (run, metrics)
    }

    #[tokio::test]
    async fn extract_and_record_persists_findings_and_indexes_them() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = RunStore::new(ctx.db.clone());
        let (run, metrics) = queued_run();
        store.create(&run).await.unwrap();
        store.finalize(&run.id, &metrics).await.unwrap();

        let canned = r#"[
            {"kind":"underperformance","severity":"warning","summary":"Total return below baseline","evidence":{"value":-3.2}},
            {"kind":"drawdown_concentration","severity":"critical","summary":"18% drawdown in calm regime","evidence":{"value":18.0}}
        ]"#;
        let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));

        let n = extract_and_record(&ctx, &run.id, dispatch, DEFAULT_FINDINGS_MODEL).await;
        assert_eq!(n, 2);

        // Persisted in the DB
        let read = store.read_findings(&run.id).await.unwrap();
        assert_eq!(read.len(), 2);
        assert!(read.iter().any(|f| f.kind == "underperformance"));
        assert!(read.iter().any(|f| f.kind == "drawdown_concentration"));

        // Indexed for ⌘K
        let hits = api_search::search(
            &ctx,
            "drawdown",
            &crate::search::SearchQuery {
                kind: Some(crate::search::SearchKind::Finding),
                limit: None,
            },
        )
        .await
        .unwrap();
        assert!(!hits.is_empty(), "finding should be indexed for ⌘K search");
    }

    #[tokio::test]
    async fn extract_and_record_returns_zero_on_extractor_error() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = RunStore::new(ctx.db.clone());
        let (run, metrics) = queued_run();
        store.create(&run).await.unwrap();
        store.finalize(&run.id, &metrics).await.unwrap();

        // Mock returns garbage that the extractor's JSON-array slicer can't parse.
        let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo("definitely not a json array"));

        let n = extract_and_record(&ctx, &run.id, dispatch, DEFAULT_FINDINGS_MODEL).await;
        assert_eq!(n, 0, "extractor failure must surface as 0, not a panic");

        // Run still readable, no findings rows.
        let read = store.read_findings(&run.id).await.unwrap();
        assert!(read.is_empty());

        // Audit row recorded the failure
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM api_audit WHERE domain = 'eval' AND operation = 'extract_findings' AND outcome = 'error'",
        )
        .fetch_one(&ctx.db)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn extract_and_record_returns_zero_when_extractor_returns_empty_array() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = RunStore::new(ctx.db.clone());
        let (run, metrics) = queued_run();
        store.create(&run).await.unwrap();
        store.finalize(&run.id, &metrics).await.unwrap();

        let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo("[]"));
        let n = extract_and_record(&ctx, &run.id, dispatch, DEFAULT_FINDINGS_MODEL).await;
        assert_eq!(n, 0);

        // Audit row recorded the success (empty result is still ok)
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM api_audit WHERE domain = 'eval' AND operation = 'extract_findings' AND outcome = 'ok'",
        )
        .fetch_one(&ctx.db)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn summarise_decisions_handles_empty() {
        let v = summarise_decisions(&[]);
        assert_eq!(v["n_decisions"], 0);
    }

    #[test]
    fn summarise_decisions_counts_actions_and_pnl() {
        let rows = vec![
            DecisionRow {
                run_id: "r".into(),
                decision_index: 0,
                timestamp: Utc::now(),
                asset: "BTC/USD".into(),
                action: "long_open".into(),
                conviction: Some(0.7),
                justification: None,
                reasoning: None,
                order_size: Some(1.0),
                fill_price: Some(60000.0),
                fill_size: Some(1.0),
                fee: Some(1.0),
                pnl_realized: Some(100.0),
                delayed: None,
            },
            DecisionRow {
                run_id: "r".into(),
                decision_index: 1,
                timestamp: Utc::now(),
                asset: "BTC/USD".into(),
                action: "flat".into(),
                conviction: None,
                justification: None,
                reasoning: None,
                order_size: None,
                fill_price: None,
                fill_size: None,
                fee: None,
                pnl_realized: Some(-25.0),
                delayed: None,
            },
        ];
        let v = summarise_decisions(&rows);
        assert_eq!(v["n_decisions"], 2);
        assert_eq!(v["wins"], 1);
        assert_eq!(v["losses"], 1);
        assert_eq!(v["realized_pnl_sum"], 75.0);
        assert_eq!(v["assets"], serde_json::json!(["BTC/USD"]));
    }

    #[test]
    fn findings_model_routes_per_provider_kind() {
        let anthropic = ProviderEntry {
            name: "anthropic".into(),
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            enabled_models: vec![],
        };
        assert_eq!(findings_model_for_provider(&anthropic), DEFAULT_FINDINGS_MODEL);

        // OpenRouter (OpenAI-compat shape) must use the slug form —
        // the Anthropic-native id is what blew up in production
        // (2026-05-19, eval 01KRZ18JTMZ1S7W1MBKC1PNNSJ).
        let openrouter = ProviderEntry {
            name: "openrouter".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://openrouter.ai/api/v1".into(),
            api_key_env: "OPENROUTER_API_KEY".into(),
            enabled_models: vec![],
        };
        assert_eq!(
            findings_model_for_provider(&openrouter),
            OPENROUTER_FINDINGS_MODEL
        );

        // Generic OpenAI-compat: fall back to the operator's enabled
        // models (already vetted for this provider).
        let generic = ProviderEntry {
            name: "custom".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://api.example.com/v1".into(),
            api_key_env: "CUSTOM_KEY".into(),
            enabled_models: vec!["gpt-4o-mini".into()],
        };
        assert_eq!(findings_model_for_provider(&generic), "gpt-4o-mini");
    }

    #[test]
    fn summarise_equity_computes_drawdown() {
        let now = Utc::now();
        let curve = vec![
            (now, 10000.0),
            (now, 11000.0),
            (now, 9000.0), // 18.18% drawdown from peak 11000
            (now, 9500.0),
        ];
        let v = summarise_equity(&curve);
        assert_eq!(v["n_samples"], 4);
        assert_eq!(v["start"], 10000.0);
        assert_eq!(v["end"], 9500.0);
        assert_eq!(v["min"], 9000.0);
        assert_eq!(v["max"], 11000.0);
        let dd = v["max_drawdown_pct"].as_f64().unwrap();
        assert!((dd - 18.181_818).abs() < 0.01, "drawdown ≈ 18.18%, got {dd}");
    }
}

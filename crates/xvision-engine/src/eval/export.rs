//! `EvalRunExport` — single-object JSON snapshot of a completed eval run.
//!
//! Built read-only over the existing persistence layer (no executor or
//! schema changes). Backs:
//!
//! - `GET /api/eval/runs/:id/export` in the dashboard.
//! - `xvn eval export <run_id>` in the CLI.
//! - The "Download JSON" button on the run-detail page.
//!
//! See:
//! - `docs/superpowers/specs/2026-05-16-q15-eval-resilience-and-contracts.md` §3
//! - `team/contracts/q15-eval-json-export.md`
//!
//! ## Schema versioning
//!
//! `schema_version` is pinned at `"1"`. Any breaking change to the shape
//! must bump it and ship a migration helper. Additive fields (new
//! `Option<…>` keys, new sub-arrays) do not bump the version.
//!
//! ## Aspirational vs. persisted fields
//!
//! The spec lists `events`, `errors`, and a richer per-decision shape
//! (`bar`, `trader_input`, `trader_output_raw`, `trader_output_parsed`,
//! `risk_decision`, `fill`, `errors[]`). Today the engine persists:
//!
//! - `eval_runs` (one row per run, including `error: Option<String>`),
//! - `eval_decisions` (one row per decision; carries the parsed
//!   `TraderDecision` + the broker-side fill + `reasoning`),
//! - `eval_equity_samples`, `eval_attestations`, `eval_reviews`,
//!   `eval_findings`.
//!
//! There is no `eval_events` table, and per-decision raw provider output
//! is not persisted (the executor stores only the parsed action +
//! reasoning + fill). The export reflects what's actually stored:
//!
//! - `events` is always `[]` (forward-compat placeholder; emit when an
//!   event log lands).
//! - `errors` wraps `run.error` as a single-element array when set, else
//!   `[]`.
//! - `decisions[]` exports the persisted DecisionRow fields. Adding raw
//!   provider trader output to this surface is a future task and would
//!   pair with a new column on `eval_decisions`.
//!
//! Round-trip semantics: the export is `Serialize + Deserialize`, so the
//! same bytes parse back into the same struct.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agents::{Agent, AgentStore};
use crate::api::{self, ApiContext, ApiError, ApiResult};
use crate::eval::attestation::{EvalAttestation, TokensUsed};
use crate::eval::findings::Finding;
use crate::eval::review::EvalReview;
use crate::eval::run::{MetricsSummary, Run, RunStatus};
use crate::eval::scenario::Scenario;
use crate::eval::scenario_store;
use crate::eval::store::{DecisionRow, RunStore};
use crate::strategies::Strategy;

/// Pinned export schema version. Bump only on breaking shape changes;
/// additive fields stay on `"1"`.
pub const SCHEMA_VERSION: &str = "1";

/// Full eval-run JSON snapshot.
///
/// `schema_version` is always present. Every other field is best-effort:
/// `scenario` / `strategy` may be `None` if the referenced record was
/// deleted after the run completed (cleanup paths exist; the export
/// stays usable). Arrays are always present, possibly empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRunExport {
    pub schema_version: String,
    pub run: Run,
    pub scenario: Option<Scenario>,
    pub strategy: Option<Strategy>,
    pub agents: Vec<Agent>,
    pub metrics: Option<MetricsSummary>,
    pub decisions: Vec<DecisionExportRow>,
    pub equity_samples: Vec<EquitySample>,
    /// Reserved for the future eval-events log. Currently always `[]`.
    pub events: Vec<serde_json::Value>,
    /// Run-level error log. Today wraps `run.error` as a single entry
    /// (or empty when the run completed cleanly). Per-decision errors
    /// live alongside their decision row in `decisions[]`.
    pub errors: Vec<RunErrorEntry>,
    pub reviews: Vec<ReviewExportRow>,
    pub provider_diagnostics: Option<ProviderDiagnostics>,
}

/// Per-decision export row. Mirrors the persisted `DecisionRow` plus an
/// `ix` field that matches `decision_index` — exposed by the explicit
/// name from the spec so external tools don't have to know the internal
/// column name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionExportRow {
    pub ix: u32,
    pub ts: DateTime<Utc>,
    pub asset: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conviction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_realized: Option<f64>,
}

impl From<&DecisionRow> for DecisionExportRow {
    fn from(row: &DecisionRow) -> Self {
        Self {
            ix: row.decision_index,
            ts: row.timestamp,
            asset: row.asset.clone(),
            action: row.action.clone(),
            conviction: row.conviction,
            justification: row.justification.clone(),
            reasoning: row.reasoning.clone(),
            order_size: row.order_size,
            fill_price: row.fill_price,
            fill_size: row.fill_size,
            fee: row.fee,
            pnl_realized: row.pnl_realized,
        }
    }
}

/// Single (timestamp, equity) point on the equity curve. Mirrors the
/// `eval_equity_samples` rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquitySample {
    pub ts: DateTime<Utc>,
    pub equity_usd: f64,
}

/// Single run-level error entry. Today there is at most one (the
/// terminal `eval_runs.error` string); the array shape leaves room for a
/// future per-decision or per-stage error log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunErrorEntry {
    pub message: String,
}

/// One review row + its findings, denormalized for round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewExportRow {
    pub review: EvalReview,
    pub findings: Vec<Finding>,
}

/// Provider-side diagnostics persisted alongside the run.
///
/// `attestation` is only present when the run was attested (a separate
/// `xvn eval attest` step). When an attestation is attached, its
/// `tokens_used` is authoritative — the same bytes are part of the
/// signed payload, so duplicating a divergent number elsewhere in the
/// export would let a tampered export pass a casual eyeball check. For
/// non-attested runs the counters fall back to `run.actual_*_tokens`
/// so the QA-round-trip footprint (e.g. QA15 item 5's `output_tokens=1000`
/// truncation) is still surfaced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDiagnostics {
    /// Aggregate input/output/total tokens for the run. Sourced from the
    /// attestation when present (authoritative), else from the run's
    /// observed counters.
    pub tokens_used: TokensUsed,
    /// Original run timestamp (attestation `ran_at` when present, else
    /// `run.started_at`).
    pub ran_at: DateTime<Utc>,
    /// Signed attestation. `None` when the run hasn't been attested yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<EvalAttestation>,
    /// Truncation/empty-output footprint mirrored from `run.error`. Only
    /// populated when the persisted error string matches one of the
    /// stable `trader_output[<tag>]:` failure classes; otherwise `None`
    /// so consumers can distinguish "no provider failure" from "unknown
    /// classification".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trader_output_failure: Option<String>,
}

/// Build the export for a *terminal* run id (`completed` / `failed` /
/// `cancelled`). Read-only — uses the same `RunStore` and helper
/// functions the dashboard/CLI already lean on.
///
/// Rejects non-terminal runs (`queued` / `running`) with
/// `ApiError::Validation` so the snapshot can't capture a moving
/// in-flight state. The contract scope explicitly excludes streaming
/// export for in-flight runs; the UI gates the "Download JSON" button
/// the same way, and this guard makes the rule load-bearing rather
/// than convention-only.
///
/// Returns `ApiError::NotFound` when the run id is unknown. Other
/// failures (deleted scenario, deleted agents, etc.) degrade the
/// matching field to `None` / `[]` rather than failing the whole export
/// — the goal is a usable QA artifact even when downstream records have
/// been cleaned up.
pub async fn build_export(ctx: &ApiContext, run_id: &str) -> ApiResult<EvalRunExport> {
    let store = RunStore::new(ctx.db.clone());
    let run = api::eval::get(ctx, run_id).await?;
    if !is_terminal(run.status) {
        return Err(ApiError::Validation(format!(
            "run {} is in status `{}`; export is only defined for terminal runs (completed/failed/cancelled)",
            run.id,
            run.status.as_str(),
        )));
    }

    // Scenario / strategy / agents are best-effort; cleanup paths may
    // have removed them after the run completed.
    let scenario = scenario_store::get_scenario(ctx, &run.scenario_id)
        .await
        .ok()
        .flatten();
    let strategy = api::strategy::get(ctx, &run.agent_id).await.ok();
    let agents = match &strategy {
        Some(s) => load_strategy_agents(ctx, s).await,
        None => Vec::new(),
    };

    let decisions: Vec<DecisionExportRow> = store
        .read_decisions(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("read_decisions: {e:#}")))?
        .iter()
        .map(DecisionExportRow::from)
        .collect();

    let equity_samples: Vec<EquitySample> = store
        .read_equity_curve(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("read_equity_curve: {e:#}")))?
        .into_iter()
        .map(|(ts, equity_usd)| EquitySample { ts, equity_usd })
        .collect();

    let reviews: Vec<ReviewExportRow> = match store.list_reviews_for_run(&run.id).await {
        Ok(rs) => {
            let mut out = Vec::with_capacity(rs.len());
            for review in rs {
                let findings = store
                    .read_findings_for_review(&review.id)
                    .await
                    .unwrap_or_default();
                out.push(ReviewExportRow { review, findings });
            }
            out
        }
        Err(_) => Vec::new(),
    };

    let attestation = store.get_attestation(&run.id).await.ok().flatten();
    let provider_diagnostics = build_provider_diagnostics(&run, attestation);

    let errors = match run.error.as_deref() {
        Some(msg) if !msg.trim().is_empty() => vec![RunErrorEntry {
            message: msg.to_string(),
        }],
        _ => Vec::new(),
    };

    let metrics = run.metrics.clone();
    Ok(EvalRunExport {
        schema_version: SCHEMA_VERSION.to_string(),
        run,
        scenario,
        strategy,
        agents,
        metrics,
        decisions,
        equity_samples,
        events: Vec::new(),
        errors,
        reviews,
        provider_diagnostics,
    })
}

async fn load_strategy_agents(ctx: &ApiContext, strategy: &Strategy) -> Vec<Agent> {
    if strategy.agents.is_empty() {
        return Vec::new();
    }
    let store = AgentStore::new(ctx.db.clone());
    let mut out = Vec::with_capacity(strategy.agents.len());
    for r in &strategy.agents {
        if let Ok(Some(agent)) = store.get(&r.agent_id).await {
            out.push(agent);
        }
    }
    out
}

fn build_provider_diagnostics(
    run: &Run,
    attestation: Option<EvalAttestation>,
) -> Option<ProviderDiagnostics> {
    // Attestation wins when present — its `tokens_used` is part of the
    // signed payload, so the export must mirror it exactly. Falling
    // back to `run.actual_*_tokens` only when the run hasn't been
    // attested keeps the export usable as a QA artifact for un-attested
    // runs without ever shipping a tokens_used envelope that disagrees
    // with the attached signed attestation.
    let tokens_used = match attestation.as_ref() {
        Some(att) => att.tokens_used.clone(),
        None => match (run.actual_input_tokens, run.actual_output_tokens) {
            (Some(input), Some(output)) => TokensUsed {
                input,
                output,
                total: input.saturating_add(output),
            },
            // Run has no observed usage and no attestation. Surface a
            // zeroed envelope so consumers can still rely on the field
            // shape — the `trader_output_failure` slot below stays
            // useful even when token counts weren't captured.
            _ => TokensUsed {
                input: 0,
                output: 0,
                total: 0,
            },
        },
    };
    let ran_at = attestation
        .as_ref()
        .map(|a| a.ran_at)
        .unwrap_or(run.started_at);

    let trader_output_failure = run
        .error
        .as_deref()
        .and_then(extract_trader_output_class)
        .map(str::to_string);

    // If nothing meaningful is set, return None so consumers can
    // distinguish "no diagnostics captured" from "captured zeros".
    if attestation.is_none()
        && tokens_used.total == 0
        && run.actual_input_tokens.is_none()
        && run.actual_output_tokens.is_none()
        && trader_output_failure.is_none()
    {
        return None;
    }

    Some(ProviderDiagnostics {
        tokens_used,
        ran_at,
        attestation,
        trader_output_failure,
    })
}

fn is_terminal(status: RunStatus) -> bool {
    matches!(
        status,
        RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled,
    )
}

/// Pull the stable `trader_output[<class>]:` tag out of a persisted
/// `eval_runs.error` string. Matches the wire format set by
/// `TraderOutputError::Display` in `eval::executor::trader_output`.
fn extract_trader_output_class(message: &str) -> Option<&str> {
    let needle = "trader_output[";
    let start = message.find(needle)?;
    let after = &message[start + needle.len()..];
    let end = after.find(']')?;
    Some(&after[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_trader_output_class_handles_known_wire_format() {
        let msg = "run 01TEST decision 3: trader_output[truncated]: trader output truncated at MaxTokens before any text was emitted (stop_reason=MaxTokens, input_tokens=422, output_tokens=1000, raw_excerpt=\"<empty>\")";
        assert_eq!(extract_trader_output_class(msg), Some("truncated"));
    }

    #[test]
    fn extract_trader_output_class_returns_none_for_unrelated_errors() {
        let msg = "broker timeout: connection reset";
        assert_eq!(extract_trader_output_class(msg), None);
    }

    #[test]
    fn schema_version_pinned_at_one() {
        // The contract pins schema_version to "1" from day one. Any
        // breaking change must bump it; this test fails loudly on a
        // silent change.
        assert_eq!(SCHEMA_VERSION, "1");
    }
}

#[cfg(test)]
mod roundtrip {
    //! End-to-end canary: build_export → serialize → deserialize, then
    //! assert top-level shape + `decisions[].ix` contiguous, as required
    //! by the q15-eval-json-export contract acceptance.

    use chrono::{DateTime, Utc};
    use sqlx::SqlitePool;

    use super::*;
    use crate::api::{Actor, ApiContext};
    use crate::eval::run::{Run, RunMode, RunStatus};
    use crate::eval::store::{DecisionRow, RunStore};

    async fn ctx_with_eval_tables() -> (ApiContext, tempfile::TempDir) {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        // Migrations the export touches transitively: 001 (api_audit),
        // 002 (eval_*), 014 (agent_id rename), 015 (reasoning column).
        // The rest (scenarios, reviews, etc.) aren't needed for the
        // top-level shape canary — get_scenario / list_reviews_for_run
        // degrade to None / [] when their tables don't exist.
        for migration in [
            include_str!("../../migrations/001_api_audit.sql"),
            include_str!("../../migrations/002_eval.sql"),
            include_str!("../../migrations/014_eval_agent_id.sql"),
            include_str!("../../migrations/015_eval_decisions_reasoning.sql"),
        ] {
            sqlx::query(migration).execute(&pool).await.unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::new(
            pool,
            Actor::Cli {
                user: "operator".into(),
            },
            dir.path().to_path_buf(),
        );
        (ctx, dir)
    }

    fn decision(run_id: &str, ix: u32, ts_offset: i64, action: &str) -> DecisionRow {
        DecisionRow {
            run_id: run_id.to_string(),
            decision_index: ix,
            timestamp: DateTime::<Utc>::from_timestamp(1_700_000_000 + ts_offset, 0).unwrap(),
            asset: "BTC/USD".into(),
            action: action.into(),
            conviction: Some(0.5),
            justification: Some(format!("decision {ix} justification")),
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
        }
    }

    async fn seed_completed_run(ctx: &ApiContext) -> String {
        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued("agent-A".into(), "scen-A".into(), RunMode::Backtest);
        run.status = RunStatus::Completed;
        run.actual_input_tokens = Some(422);
        run.actual_output_tokens = Some(1000);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        // Three contiguous decisions (ix=0,1,2) plus a couple of equity
        // samples so the round-trip arrays aren't empty.
        for ix in 0..3u32 {
            let d = decision(&run.id, ix, ix as i64 * 60, "hold");
            store.record_decision(&d).await.unwrap();
        }
        for (off, eq) in [(0, 100_000.0), (60, 100_500.0), (120, 99_800.0)] {
            store
                .record_equity(
                    &run.id,
                    DateTime::<Utc>::from_timestamp(1_700_000_000 + off, 0).unwrap(),
                    eq,
                )
                .await
                .unwrap();
        }
        run.id
    }

    #[tokio::test]
    async fn export_roundtrips_through_json_with_all_top_level_keys() {
        let (ctx, _d) = ctx_with_eval_tables().await;
        let run_id = seed_completed_run(&ctx).await;

        let export = build_export(&ctx, &run_id).await.expect("build_export");
        assert_eq!(export.schema_version, "1");
        assert_eq!(export.decisions.len(), 3);
        assert_eq!(export.equity_samples.len(), 3);

        // Round-trip: serialize → parse → match the spec's top-level keys.
        let json = serde_json::to_string(&export).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
        for key in [
            "schema_version",
            "run",
            "scenario",
            "strategy",
            "agents",
            "metrics",
            "decisions",
            "equity_samples",
            "events",
            "errors",
            "reviews",
            "provider_diagnostics",
        ] {
            assert!(
                parsed.get(key).is_some(),
                "expected top-level key `{key}` in export JSON; got: {json}",
            );
        }

        // Spec: decisions[].ix is contiguous (0,1,2,…). The executor
        // writes them in order; the export preserves that ordering.
        let ix_values: Vec<u64> = parsed["decisions"]
            .as_array()
            .expect("decisions array")
            .iter()
            .map(|d| d["ix"].as_u64().expect("ix is integer"))
            .collect();
        assert_eq!(ix_values, vec![0, 1, 2], "decisions[].ix must be contiguous");

        // Round-trip back into the typed struct — this asserts the
        // Serialize/Deserialize pair is symmetric. Bumps to the shape
        // that break round-trip surface here as a Deserialize error.
        let decoded: EvalRunExport =
            serde_json::from_str(&json).expect("deserialize EvalRunExport");
        assert_eq!(decoded.schema_version, export.schema_version);
        assert_eq!(decoded.decisions.len(), export.decisions.len());
    }

    #[tokio::test]
    async fn unknown_run_id_returns_not_found() {
        let (ctx, _d) = ctx_with_eval_tables().await;
        let err = build_export(&ctx, "01NOSUCHRUN0000000000000")
            .await
            .expect_err("unknown id must error");
        assert!(matches!(err, ApiError::NotFound(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn errors_array_captures_run_error_string() {
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let run = Run::new_queued("a".into(), "s".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        // QA15 footprint: trader_output[truncated] with stop_reason=MaxTokens.
        let trader_err = "run 01TEST decision 3: trader_output[truncated]: trader output truncated at MaxTokens before any text was emitted (stop_reason=MaxTokens, input_tokens=422, output_tokens=1000, raw_excerpt=\"<empty>\")";
        store
            .update_status(&run.id, RunStatus::Failed, Some(trader_err))
            .await
            .unwrap();

        let export = build_export(&ctx, &run.id).await.expect("build_export");
        assert_eq!(export.errors.len(), 1);
        assert!(export.errors[0].message.contains("trader_output[truncated]"));

        // Spec acceptance: the QA15 reproducer's truncation diagnostics
        // are surfaced under provider_diagnostics, not just buried in
        // the error string.
        let diag = export
            .provider_diagnostics
            .expect("provider_diagnostics on a failed run");
        assert_eq!(diag.trader_output_failure.as_deref(), Some("truncated"));
    }

    #[tokio::test]
    async fn queued_run_export_is_validation_error() {
        // Snapshotting an in-flight run would produce a moving,
        // partial envelope. The contract scope is terminal-only;
        // build_export enforces it instead of relying on the UI to
        // hide the button.
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        // No update_status call — the run stays in Queued.

        let err = build_export(&ctx, &run.id)
            .await
            .expect_err("queued run must be rejected");
        match err {
            ApiError::Validation(msg) => {
                assert!(msg.contains("queued"), "expected status in message, got: {msg}");
                assert!(msg.contains("terminal"), "expected terminal-only hint, got: {msg}");
            }
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn running_run_export_is_validation_error() {
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Running, None)
            .await
            .unwrap();

        let err = build_export(&ctx, &run.id)
            .await
            .expect_err("running run must be rejected");
        assert!(matches!(err, ApiError::Validation(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn cancelled_run_exports_successfully() {
        // `cancelled` is a terminal status; export must succeed even
        // when the run never produced metrics.
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Cancelled, Some("operator stop"))
            .await
            .unwrap();

        let export = build_export(&ctx, &run.id).await.expect("cancelled exports");
        assert_eq!(export.run.status.as_str(), "cancelled");
    }

    #[tokio::test]
    async fn provider_diagnostics_mirrors_attestation_when_present() {
        // When a signed attestation is attached, `tokens_used` must
        // equal `attestation.tokens_used` — the same bytes are part of
        // the signed payload, and the export must not ship a divergent
        // number in a sibling field.
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());

        // Run reports 422/1000 (the QA15 footprint) but the attached
        // attestation reports a different total (e.g. a later re-run
        // or an aggregator update). The attestation wins.
        let mut run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        run.actual_input_tokens = Some(422);
        run.actual_output_tokens = Some(1000);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let attested_tokens = TokensUsed {
            input: 7_000,
            output: 3_000,
            total: 10_000,
        };
        let attestation = EvalAttestation {
            agent_id: run.agent_id.clone(),
            scenario_id: run.scenario_id.clone(),
            metrics: MetricsSummary {
                total_return_pct: 0.0,
                sharpe: 0.0,
                max_drawdown_pct: 0.0,
                win_rate: 0.0,
                n_trades: 0,
                n_decisions: 0,
            },
            tokens_used: attested_tokens.clone(),
            ran_at: run.started_at,
            signing_pubkey_hex: "ed25519-test".into(),
            signature_hex: "sig-test".into(),
        };
        store.record_attestation(&run.id, &attestation).await.unwrap();

        let export = build_export(&ctx, &run.id).await.expect("build_export");
        let diag = export
            .provider_diagnostics
            .expect("provider_diagnostics when attestation is present");
        assert_eq!(
            diag.tokens_used, attested_tokens,
            "tokens_used must mirror the signed attestation payload, not run.actual_*"
        );
        let att = diag.attestation.expect("attestation attached");
        assert_eq!(att.tokens_used, attested_tokens);
    }

    #[tokio::test]
    async fn provider_diagnostics_falls_back_to_run_counters_without_attestation() {
        // Un-attested runs still surface the per-call truncation
        // footprint via the run's observed counters — QA15 reproducer
        // case.
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        run.actual_input_tokens = Some(422);
        run.actual_output_tokens = Some(1000);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let export = build_export(&ctx, &run.id).await.expect("build_export");
        let diag = export
            .provider_diagnostics
            .expect("provider_diagnostics with observed counters");
        assert_eq!(diag.tokens_used.input, 422);
        assert_eq!(diag.tokens_used.output, 1000);
        assert_eq!(diag.tokens_used.total, 1422);
        assert!(diag.attestation.is_none(), "no attestation attached");
    }
}

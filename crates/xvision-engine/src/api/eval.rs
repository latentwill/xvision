//! Eval-domain api dispatch.
//!
//! Public surface:
//! - `list` / `get` / `scenarios` — read-only browse (PR #23)
//! - `list_summaries` — slim wire shape for the dashboard's `/api/eval/runs`
//!   list and (future) MCP browse tools (PR #21)
//! - `get_run` — `RunDetail` (summary + decisions + equity curve) for the
//!   dashboard's `/eval-runs/:id` page (PR #24)
//! - `run` — paper-mode dispatch that constructs `PaperExecutor` +
//!   `AlpacaPaperSurface::from_env` + `AnthropicDispatch` +
//!   `ToolRegistry::default_with_builtins` from env vars (PR #26)
//! - `run_with_deps` — testable variant that takes the broker / dispatch /
//!   tools as parameters; useful for tests and any caller that wants to
//!   inject a `MockBrokerSurface` (e.g., a future "dry-run" mode)
//! - `compare` — wraps `eval::compare_runs` with audit + typed-error mapping
//!   for the dashboard's run-comparison view + `xvn eval compare` CLI
//! - `attest` — sign + persist an `EvalAttestation` for a completed run,
//!   sourcing the Ed25519 signing key from `$XVN_HOME/identity/signing.key`
//!   (auto-generated on first use). Wraps `eval::attestation::sign` +
//!   `RunStore::record_attestation`. Powers `xvn eval attest <run_id>` and
//!   the (future) `publish_attestation` MCP verb.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use crate::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch, OpenaiCompatDispatch};
use crate::agent::pipeline::{agent_slot_to_llm_slot, ResolvedAgentSlot};
use crate::agents::AgentStore;
use crate::api::audit::{self, Outcome};
use crate::api::scenario as api_scenario;
use crate::api::settings::brokers as api_brokers;
use crate::api::{search as api_search, strategy as api_strategy, ApiContext, ApiError, ApiResult};
use crate::eval::attestation::{self, EvalAttestation};
use crate::eval::compare::{compare_runs, CompareOptions, ComparisonReport, ManifestMismatch};
use crate::eval::cost::aggregate_eval_run_inference_cost;
use crate::eval::executor::{BacktestExecutor, Executor, PaperExecutor};
use crate::eval::findings::{Finding, InferenceCostDominatesReturnPayload, Severity};
use crate::eval::metrics::{
    compute_net_return_pct, inference_cost_dominates, INFERENCE_COST_DOMINANCE_THRESHOLD,
};
use crate::eval::run::{Run, RunMode, RunStatus};
#[allow(deprecated)]
use crate::eval::scenario::canonical_scenarios;
use crate::eval::scenario::Scenario;
use crate::eval::store::{ListFilter, RunStore};
use crate::tools::ToolRegistry;
use xvision_core::config::{self, ProviderEntry, ProviderKind};
use xvision_core::market::Ohlcv;
use xvision_data::fixtures::load_ohlcv_fixture;
use xvision_execution::broker_surface::{AlpacaPaperSurface, BrokerSurface};
use xvision_filters::{FilterEventV1, FilterSummary};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRunsRequest {
    pub agent_id: Option<String>,
    pub scenario_id: Option<String>,
    pub status: Option<RunStatus>,
    /// Optional pagination — when both fields are absent, every matching
    /// row is returned. The dashboard's list endpoint passes both;
    /// internal callers (retry idempotency, chart preview) pass neither
    /// because they need the full match set.
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Paged-list envelope used by the dashboard's `/api/eval/runs` route.
/// Carries the total row count so the SPA can render "page X of N"
/// without a second round-trip per page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedRunSummaries {
    pub items: Vec<RunSummary>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioSummary {
    pub id: String,
    pub display_name: String,
    pub asset_universe: Vec<String>,
    pub regime_tags: Vec<String>,
    pub time_window_days: i64,
}

/// Slim wire shape of a run. Used by the dashboard's `/api/eval/runs` and
/// (future) MCP browse tools so the payload stays bounded as the engine adds
/// internal telemetry fields to `Run`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: String,
    pub agent_id: String,
    pub scenario_id: String,
    pub mode: String,
    pub status: String,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub started_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts-export", ts(type = "string | null"))]
    pub completed_at: Option<DateTime<Utc>>,
    pub sharpe: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub total_return_pct: Option<f64>,
    pub error: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub actual_input_tokens: Option<u64>,
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub actual_output_tokens: Option<u64>,
    /// LLM inference cost aggregated over all model calls for this run (in USD / quote currency).
    /// `None` for old runs without pricing data or when the model isn't in the pricing catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_cost_quote_total: Option<f64>,
    /// Net return after deducting LLM inference cost from gross trading return.
    /// `None` for old runs without pricing data or when the model isn't in the pricing catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_return_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter_summaries: Vec<FilterSummary>,
}

/// Full run detail — `RunSummary` plus the decision rows and equity samples.
/// Used by `/api/eval/runs/:id`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDetail {
    pub summary: RunSummary,
    pub decisions: Vec<DecisionRowDto>,
    pub equity_curve: Vec<EquityPoint>,
    #[serde(default)]
    pub filter_events: Vec<FilterEventV1>,
    #[serde(default)]
    pub filter_summaries: Vec<FilterSummary>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRowDto {
    pub decision_index: u32,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub timestamp: DateTime<Utc>,
    pub asset: String,
    pub action: String,
    pub conviction: Option<f64>,
    pub justification: Option<String>,
    pub reasoning: Option<String>,
    pub order_size: Option<f64>,
    pub fill_price: Option<f64>,
    pub fill_size: Option<f64>,
    pub fee: Option<f64>,
    pub pnl_realized: Option<f64>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub timestamp: DateTime<Utc>,
    pub equity_usd: f64,
}

pub async fn list(ctx: &ApiContext, req: ListRunsRequest) -> ApiResult<Vec<Run>> {
    let started = Instant::now();
    let result = list_inner(ctx, &req).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let args_json = serde_json::to_string(&req).ok();
    let _ = audit::record(
        ctx,
        "eval",
        "list",
        None,
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_inner(ctx: &ApiContext, req: &ListRunsRequest) -> ApiResult<Vec<Run>> {
    let store = RunStore::new(ctx.db.clone());
    let filter = ListFilter {
        agent_id: req.agent_id.clone(),
        scenario_id: req.scenario_id.clone(),
        status: req.status,
        limit: req.limit,
        offset: req.offset,
    };
    store
        .list(filter)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

/// Paged variant of `list_summaries` — returns one page of `RunSummary`
/// rows plus the total count. The dashboard's `/api/eval/runs` route
/// drives this so the SPA's pager has both halves of the contract in a
/// single round-trip.
pub async fn list_summaries_paged(ctx: &ApiContext, req: ListRunsRequest) -> ApiResult<PagedRunSummaries> {
    let started = Instant::now();
    let result = list_summaries_paged_inner(ctx, &req).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let args_json = serde_json::to_string(&req).ok();
    let _ = audit::record(
        ctx,
        "eval",
        "list_summaries_paged",
        None,
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_summaries_paged_inner(ctx: &ApiContext, req: &ListRunsRequest) -> ApiResult<PagedRunSummaries> {
    let store = RunStore::new(ctx.db.clone());
    let filter = ListFilter {
        agent_id: req.agent_id.clone(),
        scenario_id: req.scenario_id.clone(),
        status: req.status,
        limit: req.limit,
        offset: req.offset,
    };
    // Compute total BEFORE slicing so the pager renders an honest
    // "of N" even when the active page is the last and partial.
    let total = store
        .count(&filter)
        .await
        .map_err(|e| ApiError::Internal(format!("count eval_runs: {e}")))?;
    let runs = store
        .list(filter)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(PagedRunSummaries {
        items: runs.into_iter().map(summarise).collect(),
        total,
    })
}

/// Same as `list` but returns the slim `RunSummary` shape.
pub async fn list_summaries(ctx: &ApiContext, req: ListRunsRequest) -> ApiResult<Vec<RunSummary>> {
    let started = Instant::now();
    let result = list_summaries_inner(ctx, &req).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let args_json = serde_json::to_string(&req).ok();
    let _ = audit::record(
        ctx,
        "eval",
        "list_summaries",
        None,
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_summaries_inner(ctx: &ApiContext, req: &ListRunsRequest) -> ApiResult<Vec<RunSummary>> {
    let runs = list_inner(ctx, req).await?;
    Ok(runs.into_iter().map(summarise).collect())
}

pub async fn get(ctx: &ApiContext, run_id: &str) -> ApiResult<Run> {
    let started = Instant::now();
    let result = get_inner(ctx, run_id).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "eval",
        "get",
        Some(run_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

pub async fn delete(ctx: &ApiContext, run_id: &str) -> ApiResult<()> {
    let started = Instant::now();
    let store = RunStore::new(ctx.db.clone());
    let result = store.delete(run_id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("run not found") {
            ApiError::NotFound(format!("eval run '{run_id}'"))
        } else {
            ApiError::Internal(msg)
        }
    });
    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "eval",
        "delete",
        Some(run_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

/// F-11: navigate from an eval run back to the workspace agent record
/// that drove it. Reads `eval_runs.agents_agent_id` (added in migration
/// 021) and, when populated, looks up the live agent in the agent
/// library.
///
/// Returns:
/// - `Ok(Some(agent))` when the run carries a long-lived agent id AND
///   that row still exists in `agents`.
/// - `Ok(None)` when either the run is missing, the column is NULL
///   (pre-migration-022 row, intentionally not backfilled), or the
///   referenced agent has been deleted.
///
/// No regex / bundle-hash fallback — by design. The whole point of the
/// new column is to retire that heuristic.
pub async fn lookup_agent_for_eval_run(
    ctx: &ApiContext,
    run_id: &str,
) -> ApiResult<Option<crate::agents::model::Agent>> {
    let store = RunStore::new(ctx.db.clone());
    let aid = store
        .get_agents_agent_id(run_id)
        .await
        .map_err(|e| ApiError::Internal(format!("read agents_agent_id: {e}")))?;
    let Some(aid) = aid else { return Ok(None) };
    let agent_store = AgentStore::new(ctx.db.clone());
    let agent = agent_store
        .get(&aid)
        .await
        .map_err(|e| ApiError::Internal(format!("load agent {aid}: {e}")))?;
    Ok(agent)
}

pub async fn cancel(ctx: &ApiContext, run_id: &str) -> ApiResult<Run> {
    let started = Instant::now();
    let store = RunStore::new(ctx.db.clone());
    let result = async {
        let cancelled = store
            .cancel_active(run_id, "cancelled by user")
            .await
            .map_err(|e| ApiError::Internal(format!("cancel run: {e}")))?;
        if cancelled {
            return get_inner(ctx, run_id).await;
        }

        let run = get_inner(ctx, run_id).await?;
        if run.status == RunStatus::Cancelled {
            return Ok(run);
        }
        if run.status.is_terminal() {
            return Err(ApiError::Validation(format!(
                "run '{run_id}' is already {}",
                run.status.as_str()
            )));
        }
        Err(ApiError::Validation(format!(
            "run '{run_id}' cannot be cancelled from status {}",
            run.status.as_str()
        )))
    }
    .await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "eval",
        "cancel",
        Some(run_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn get_inner(ctx: &ApiContext, run_id: &str) -> ApiResult<Run> {
    let store = RunStore::new(ctx.db.clone());
    store.get(run_id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("run not found") {
            ApiError::NotFound(format!("run '{run_id}'"))
        } else {
            ApiError::Internal(msg)
        }
    })
}

/// Classifies *why* a retry was issued, so downstream surfaces (review
/// queue, lineage ribbon, audit log readers) can distinguish a deliberate
/// rerun of a `Completed` run from a recovery retry of a `Failed` or
/// `Cancelled` run. Derived deterministically from source status —
/// callers do not supply it.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryReason {
    /// Source run was `Failed` or `Cancelled`. Operator wants to retry
    /// the same workload now that the underlying problem is fixed.
    FailureRecovery,
    /// Source run was `Completed`. Operator wants a fresh trace against
    /// the same agent/scenario for result-stability or re-test.
    ManualRerun,
}

impl RetryReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            RetryReason::FailureRecovery => "failure_recovery",
            RetryReason::ManualRerun => "manual_rerun",
        }
    }
}

/// Rich return shape from `retry_with_outcome`: the freshly-enqueued (or
/// coalesced-in-flight) `RunDetail`, plus the lineage breadcrumbs the
/// shorter `retry(...) -> RunDetail` form discards. Lineage is also
/// written to the audit log so downstream readers can pick it up
/// without a schema change to `eval_runs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryOutcome {
    pub detail: RunDetail,
    pub reason: RetryReason,
    pub source_run_id: String,
}

/// Retry an eval run by enqueueing a new run with the same
/// `(agent_id, scenario_id, mode, params_override)` inputs.
///
/// Accepted source statuses: `Failed`, `Cancelled`, `Completed`.
///
/// - `Failed` / `Cancelled` → `RetryReason::FailureRecovery`. The
///   operator typically wants to re-run after fixing a transient error
///   or after deliberately stopping a run.
/// - `Completed` → `RetryReason::ManualRerun`. The operator wants a
///   fresh trace against the same agent/scenario inputs (re-test a fix,
///   verify result stability). This is NOT A/B compare and NOT a
///   fingerprint-dedup case — the operator explicitly wants a new run.
///
/// Rejected with `ApiError::Validation` if the source is `Queued` or
/// `Running` — there's nothing to retry, and the existing run is what
/// the operator should be watching.
///
/// Idempotent on the source-run fingerprint: if any run with the same
/// `(agent_id, scenario_id, mode, params_override)` is already queued or
/// running, returns that run's detail instead of starting another to
/// avoid retry storms when the operator double-clicks the Retry/Rerun
/// button. A queued or running sibling that shares
/// `(agent_id, scenario_id, mode)` but differs on `params_override` is a
/// distinct workload and does NOT coalesce — retry starts a new run.
///
/// Lineage (`source_run_id` + classified `RetryReason`) is recorded in
/// the audit log's `args_json` column and returned in
/// `retry_with_outcome`'s `RetryOutcome`.
pub async fn retry(ctx: &ApiContext, source_id: &str) -> ApiResult<RunDetail> {
    retry_with_outcome(ctx, source_id).await.map(|o| o.detail)
}

/// Same gate, idempotency, and side effects as [`retry`] — additionally
/// surfaces `RetryReason` and the source run id so callers that want
/// lineage in their typed response (frontend, CLI, MCP) don't have to
/// re-read the audit log.
pub async fn retry_with_outcome(ctx: &ApiContext, source_id: &str) -> ApiResult<RetryOutcome> {
    let started = Instant::now();
    let result = retry_inner(ctx, source_id).await;
    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    // Capture lineage in audit args_json so downstream readers can
    // distinguish a deliberate Rerun from a FailureRecovery retry
    // without a migration to `eval_runs`.
    let args_json = result.as_ref().ok().and_then(|o| {
        serde_json::to_string(&serde_json::json!({
            "reason": o.reason.as_str(),
            "source_run_id": o.source_run_id,
        }))
        .ok()
    });
    let _ = audit::record(
        ctx,
        "eval",
        "retry",
        Some(source_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn retry_inner(ctx: &ApiContext, source_id: &str) -> ApiResult<RetryOutcome> {
    let source = get_inner(ctx, source_id).await?;
    let reason = match source.status {
        RunStatus::Failed | RunStatus::Cancelled => RetryReason::FailureRecovery,
        RunStatus::Completed => RetryReason::ManualRerun,
        RunStatus::Queued | RunStatus::Running => {
            return Err(ApiError::Validation(format!(
                "run '{source_id}' cannot be retried from status {}; retry requires a 'failed', 'cancelled', or 'completed' run",
                source.status.as_str()
            )));
        }
    };

    // Idempotency: if any run with the same fingerprint is still in
    // flight, return it instead of starting another. Prevents retry
    // storms when the operator double-clicks the Retry/Rerun button.
    // This guarantee holds equally for FailureRecovery and ManualRerun
    // — a deliberate rerun of a Completed source still coalesces onto a
    // queued/running sibling rather than fanning out.
    let store = RunStore::new(ctx.db.clone());
    let siblings = store
        .list(ListFilter {
            agent_id: Some(source.agent_id.clone()),
            scenario_id: Some(source.scenario_id.clone()),
            status: None,
            ..Default::default()
        })
        .await
        .map_err(|e| ApiError::Internal(format!("list runs for retry idempotency: {e}")))?;
    if let Some(existing) = siblings.into_iter().find(|r| {
        r.id != source.id
            && r.mode == source.mode
            && r.params_override == source.params_override
            && matches!(r.status, RunStatus::Queued | RunStatus::Running)
    }) {
        let detail = get_run(ctx, &existing.id).await?;
        return Ok(RetryOutcome {
            detail,
            reason,
            source_run_id: source.id,
        });
    }

    let req = EvalRunRequest {
        agent_id: source.agent_id.clone(),
        scenario_id: source.scenario_id.clone(),
        mode: source.mode,
        params_override: source.params_override.clone(),
        limits: None,
        skip_preflight: false,
    };
    let detail = start_run(ctx, req).await?;
    Ok(RetryOutcome {
        detail,
        reason,
        source_run_id: source.id,
    })
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompareRunsRequest {
    /// Two-or-more run ids to fold into a single `ComparisonReport`.
    pub run_ids: Vec<String>,
    /// When `true`, skip the manifest-canonical consistency check and render
    /// the comparison even when runs have different data manifests. Default
    /// `false`. Pass `true` only when you explicitly want to compare runs
    /// that used different feeds, adjustment modes, or session filters.
    #[serde(default)]
    pub allow_manifest_mismatch: bool,
}

/// Run-set comparison. Loads each run + equity curve + findings from the
/// store and packages them into a `ComparisonReport`.
///
/// Validation:
/// - rejects zero or one run id with `ApiError::Validation` (compare needs
///   ≥2 to do its job — the dashboard's existing `/eval-runs/:id` view
///   already covers single-run inspection)
/// - maps a missing run to `ApiError::NotFound` naming the offending id so
///   operators can fix typos without grepping logs
pub async fn compare(ctx: &ApiContext, req: CompareRunsRequest) -> ApiResult<ComparisonReport> {
    let started = Instant::now();
    let target = if req.run_ids.is_empty() {
        None
    } else {
        Some(req.run_ids.join(","))
    };
    let args_json = serde_json::to_string(&req).ok();

    let result = compare_inner(ctx, &req).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "eval",
        "compare",
        target.as_deref(),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn compare_inner(ctx: &ApiContext, req: &CompareRunsRequest) -> ApiResult<ComparisonReport> {
    if req.run_ids.is_empty() {
        return Err(ApiError::Validation(
            "compare requires at least one run id".into(),
        ));
    }
    if req.run_ids.len() < 2 {
        return Err(ApiError::Validation(
            "compare requires at least two run ids — single-run views go through `eval get`".into(),
        ));
    }
    let store = RunStore::new(ctx.db.clone());
    let options = CompareOptions {
        allow_manifest_mismatch: req.allow_manifest_mismatch,
    };
    compare_runs(&req.run_ids, &store, &options).await.map_err(|e| {
        // anyhow's alternate formatter walks the entire context chain so
        // the underlying "run not found: <id>" surfaces even though
        // `compare_runs` wraps it with `with_context`.
        let chain = format!("{e:#}");
        if chain.contains("run not found") {
            let missing = chain
                .rsplit_once("run not found:")
                .map(|(_, tail)| tail.trim().trim_end_matches(['\'', '"']).to_string())
                .unwrap_or_else(|| "<unknown>".into());
            ApiError::NotFound(format!("eval run '{missing}'"))
        } else if e.downcast_ref::<ManifestMismatch>().is_some() {
            ApiError::Validation(chain)
        } else {
            ApiError::Internal(chain)
        }
    })
}

/// Full run detail (summary + decisions + equity curve). Maps the engine's
/// `run not found` error to typed `NotFound` so the dashboard renders 404
/// rather than 500.
pub async fn get_run(ctx: &ApiContext, id: &str) -> ApiResult<RunDetail> {
    let started = Instant::now();
    let result = get_run_inner(ctx, id).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "eval",
        "get_run",
        Some(id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn get_run_inner(ctx: &ApiContext, id: &str) -> ApiResult<RunDetail> {
    let store = RunStore::new(ctx.db.clone());

    let run = store.get(id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("run not found") {
            ApiError::NotFound(format!("eval run '{id}'"))
        } else {
            ApiError::Internal(msg)
        }
    })?;

    let decisions = store
        .read_decisions(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .map(|d| DecisionRowDto {
            decision_index: d.decision_index,
            timestamp: d.timestamp,
            asset: d.asset,
            action: d.action,
            conviction: d.conviction,
            justification: d.justification,
            reasoning: d.reasoning,
            order_size: d.order_size,
            fill_price: d.fill_price,
            fill_size: d.fill_size,
            fee: d.fee,
            pnl_realized: d.pnl_realized,
        })
        .collect();

    let equity_curve = store
        .read_equity_curve(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .map(|(timestamp, equity_usd)| EquityPoint {
            timestamp,
            equity_usd,
        })
        .collect();

    let filter_events = store
        .read_filter_events(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let filter_summaries = store
        .read_filter_summaries(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut summary = summarise(run);
    summary.filter_summaries = filter_summaries.clone();

    Ok(RunDetail {
        summary,
        decisions,
        equity_curve,
        filter_events,
        filter_summaries,
    })
}

/// Return the behavior summary for a run by loading its decisions on demand
/// and running the pure derivation function. No DB writes; safe to call
/// repeatedly.
pub async fn get_run_behavior(
    ctx: &ApiContext,
    run_id: &str,
) -> ApiResult<crate::eval::behavior::BehaviorSummary> {
    let store = RunStore::new(ctx.db.clone());
    // Verify the run exists so callers get a proper NotFound rather than
    // an empty summary for a non-existent id.
    store.get(run_id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("run not found") {
            ApiError::NotFound(format!("eval run '{run_id}'"))
        } else {
            ApiError::Internal(msg)
        }
    })?;
    let decisions = store
        .read_decisions(run_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(crate::eval::behavior::derive_behavior_summary(&decisions))
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalRunRequest {
    /// Strategy agent id returned by `api::strategy::list`.
    pub agent_id: String,
    /// Scenario id from `canonical_scenarios()` (e.g. `crypto-bull-q1-2025`).
    pub scenario_id: String,
    /// Run mode. `Paper` drives an `AlpacaPaperSurface` against real Alpaca
    /// paper credentials; `Backtest` replays the scenario's parquet fixture
    /// in-process without any broker.
    pub mode: RunMode,
    /// Optional per-run override of `Strategy.mechanical_params`. Persisted as
    /// `eval_runs.params_override_json`.
    #[cfg_attr(feature = "ts-export", ts(type = "Record<string, unknown> | null"))]
    pub params_override: Option<serde_json::Value>,
    /// Optional per-run hard caps (decisions / token totals / wall-clock).
    /// Breach lands the run as `Cancelled` with a stable reason string in
    /// `error`. See `crate::eval::limits::EvalLimits` for shape + semantics.
    /// `None` (or every field None) is the pre-limits behavior.
    #[serde(default)]
    pub limits: Option<crate::eval::limits::EvalLimits>,
    /// When `true`, skip the provider reachability preflight and launch the
    /// run regardless of provider state. For offline-development scenarios
    /// and CI replay only — the default (`false`) is "preflight on" and
    /// is the safe production default.
    ///
    /// CLI: `--skip-preflight`. Dashboard: `skip_preflight: Option<bool>`.
    /// When skipped, a `warn`-severity `supervisor_notes` row is written
    /// immediately after run creation so the audit trail is honest.
    #[serde(default)]
    pub skip_preflight: bool,
}

/// Public env-bound entry point: constructs broker (paper mode only) /
/// dispatch / tools from environment variables and dispatches to
/// `run_with_deps`.
///
/// Required env:
/// - paper mode: `APCA_API_KEY_ID`, `APCA_API_SECRET_KEY`,
///   `[APCA_API_BASE_URL]`, `ANTHROPIC_API_KEY`
/// - backtest mode: `ANTHROPIC_API_KEY` only (no broker constructed)
///
/// Validation that doesn't depend on env (missing strategy, missing
/// scenario) runs FIRST so the operator sees a clean "strategy not found"
/// error rather than buried-behind an `APCA_API_KEY_ID not found` from the
/// broker constructor.
pub async fn run(ctx: &ApiContext, req: EvalRunRequest) -> ApiResult<Run> {
    // Early NotFound surfaces without env-var noise. Resolve the scenario
    // via the DB-backed registry (with a legacy `canonical_scenarios()`
    // fallback for test contexts that haven't applied migration 006).
    let strategy = api_strategy::get(ctx, &req.agent_id).await?;
    let _scenario = resolve_scenario(ctx, &req.scenario_id).await?;

    let broker: Option<Arc<dyn BrokerSurface>> = match req.mode {
        RunMode::Paper => Some(build_alpaca_paper_broker(ctx).await?),
        RunMode::Backtest => None,
    };
    let agent_slots = resolve_agent_slots(ctx, &strategy).await?;
    validate_eval_trader_source(&strategy, &agent_slots)?;

    let provider_names = validate_provider_preflight(ctx, &req, &strategy, &agent_slots).await?;
    let skip_preflight = req.skip_preflight;
    let (dispatch_arc, findings_model) = build_eval_dispatch(ctx, &strategy, &agent_slots).await?;
    let tools_arc = Arc::new(ToolRegistry::default_with_builtins());
    let run = run_with_deps(ctx, req, broker, dispatch_arc, findings_model, tools_arc).await?;
    let store = RunStore::new(ctx.db.clone());
    write_preflight_supervisor_notes(&store, &run.id, &provider_names, skip_preflight).await;
    Ok(run)
}

/// Build an Alpaca paper broker, preferring credentials stored via the
/// settings UI (`$XVN_HOME/secrets/brokers.toml`) over `APCA_*` env
/// vars. Env-var fallback keeps CI scripts working without migration.
/// Returns `ApiError::Validation` with a user-actionable message if
/// neither source has credentials — the dashboard wires this into
/// "Configure Alpaca → Settings" copy.
async fn build_alpaca_paper_broker(ctx: &ApiContext) -> ApiResult<Arc<dyn BrokerSurface>> {
    const DEFAULT_PAPER_URL: &str = "https://paper-api.alpaca.markets";
    if let Some(creds) = api_brokers::load_alpaca_credentials(&ctx.xvn_home).await? {
        let base = creds.base_url.as_deref().unwrap_or(DEFAULT_PAPER_URL);
        return AlpacaPaperSurface::from_credentials(&creds.api_key_id, &creds.api_secret_key, base)
            .map(|s| Arc::new(s) as Arc<dyn BrokerSurface>)
            .map_err(|e| ApiError::Internal(format!("alpaca paper from stored creds: {e}")));
    }
    // Env-var fallback.
    match AlpacaPaperSurface::from_env() {
        Ok(s) => Ok(Arc::new(s)),
        Err(e) => {
            let msg = e.to_string();
            // Missing env vars is operator-actionable; bubble the
            // "where to set" hint into the validation message.
            if msg.contains("APCA_API_KEY_ID") || msg.contains("APCA_API_SECRET_KEY") {
                Err(ApiError::Validation(
                    "Alpaca paper credentials not configured. Set them in Settings → Brokers, or export APCA_API_KEY_ID + APCA_API_SECRET_KEY before running.".into()
                ))
            } else {
                Err(ApiError::Internal(format!("alpaca paper from env: {e}")))
            }
        }
    }
}

/// Build the LLM dispatch the eval will use plus the findings-extractor
/// model id appropriate for that provider. The second tuple element
/// exists because the postprocess path reuses this same dispatch, and
/// the right Haiku id varies by provider (Anthropic-native vs OpenRouter
/// slug); see [`crate::eval::postprocess::findings_model_for_provider`].
async fn build_eval_dispatch(
    ctx: &ApiContext,
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
) -> ApiResult<(Arc<dyn LlmDispatch>, String)> {
    let provider_name = select_eval_provider(ctx, strategy, agent_slots).await?;
    let cfg_path = runtime_config_path(ctx);
    let cfg = tokio::task::spawn_blocking(move || config::load_runtime(&cfg_path))
        .await
        .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))?
        .map_err(|e| ApiError::Validation(format!("load config: {e}")))?;
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == provider_name)
        .ok_or_else(|| {
            ApiError::Validation(format!(
                "provider `{provider_name}` is not configured. Pick a configured provider/model for the strategy agent before running eval."
            ))
        })?;
    let runtime_slots = runtime_slots(strategy, agent_slots);
    validate_eval_provider_models(entry, &runtime_slots)?;
    let findings_model = crate::eval::postprocess::findings_model_for_provider(entry);
    let dispatch = dispatch_from_provider(entry).await?;
    Ok((dispatch, findings_model))
}

async fn select_eval_provider(
    ctx: &ApiContext,
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
) -> ApiResult<String> {
    if let Some(provider) = runtime_slots(strategy, agent_slots)
        .into_iter()
        .filter_map(|slot| slot.provider.as_deref())
        .map(str::trim)
        .find(|provider| !provider.is_empty())
    {
        return Ok(provider.to_string());
    }

    let agent_store = AgentStore::new(ctx.db.clone());
    for agent_ref in &strategy.agents {
        if let Some(agent) = agent_store
            .get(&agent_ref.agent_id)
            .await
            .map_err(|e| ApiError::Internal(format!("load agent {}: {e}", agent_ref.agent_id)))?
        {
            if let Some(provider) = agent
                .slots
                .iter()
                .map(|slot| slot.provider.trim())
                .find(|provider| !provider.is_empty())
            {
                return Ok(provider.to_string());
            }
        }
    }

    Err(ApiError::Validation(format!(
        "eval requires an explicit provider/model on a strategy slot or attached agent; \
         no workspace default is assumed. Strategy `{}` has no slot or attached agent with a non-empty provider. \
         Re-create with `xvn strategy new --provider <name> --model <id>`, set the provider/model on the AgentSlot, or attach an agent that has them configured.",
        strategy.manifest.id,
    )))
}

/// Collect the distinct set of provider names referenced by every slot (legacy
/// and attached-agent) in the strategy. This is the preflight candidate set:
/// every name returned here will be probed by `preflight_providers` before
/// the run is queued.
///
/// Returns an empty `Vec` when the strategy has no slots (misconfigured;
/// `validate_eval_trader_source` will reject it later) or when every slot
/// omits the provider field. Callers must not fail on an empty return — the
/// preflight gate simply skips the probe.
async fn collect_provider_names_for_strategy(
    ctx: &ApiContext,
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    // 1. Legacy inline slots on the strategy (trader / intern / regime).
    for slot in [
        strategy.trader_slot.as_ref(),
        strategy.intern_slot.as_ref(),
        strategy.regime_slot.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(p) = slot.provider.as_deref() {
            let p = p.trim();
            if !p.is_empty() && !names.contains(&p.to_string()) {
                names.push(p.to_string());
            }
        }
    }

    // 2. Resolved agent slots (post-refactor strategies — these include the
    //    slot configs loaded from the agent library rows at start_run time).
    for resolved in agent_slots {
        if let Some(p) = resolved.slot.provider.as_deref() {
            let p = p.trim();
            if !p.is_empty() && !names.contains(&p.to_string()) {
                names.push(p.to_string());
            }
        }
    }

    // 3. AgentRef rows that didn't resolve into `agent_slots` (e.g., because
    //    the agent-store lookup was skipped or failed). Load each referenced
    //    agent's slots directly as a belt-and-suspenders safety net.
    if !strategy.agents.is_empty() {
        let agent_store = AgentStore::new(ctx.db.clone());
        for agent_ref in &strategy.agents {
            // If we already covered this via resolved slots, skip the DB hit.
            if let Ok(Some(agent)) = agent_store.get(&agent_ref.agent_id).await {
                for slot in &agent.slots {
                    let p = slot.provider.trim();
                    if !p.is_empty() && !names.contains(&p.to_string()) {
                        names.push(p.to_string());
                    }
                }
            }
        }
    }

    names
}

async fn validate_provider_preflight(
    ctx: &ApiContext,
    req: &EvalRunRequest,
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
) -> ApiResult<Vec<String>> {
    let provider_names = collect_provider_names_for_strategy(ctx, strategy, agent_slots).await;
    if !req.skip_preflight && !provider_names.is_empty() {
        let preflight_results = crate::eval::preflight::preflight_providers(ctx, &provider_names).await;
        let failing: Vec<_> = preflight_results.iter().filter(|r| !r.reachable).collect();
        if !failing.is_empty() {
            let error_body = crate::eval::preflight::format_preflight_error(&preflight_results);
            tracing::warn!(
                strategy_id = %req.agent_id,
                scenario_id = %req.scenario_id,
                failing_providers = %failing.iter().map(|r| r.provider_name.as_str()).collect::<Vec<_>>().join(", "),
                "eval launch blocked by provider preflight: {error_body}",
            );
            return Err(ApiError::Validation(error_body));
        }
    } else if req.skip_preflight {
        tracing::warn!(
            strategy_id = %req.agent_id,
            scenario_id = %req.scenario_id,
            "provider preflight bypassed via skip_preflight — run will proceed regardless of provider state",
        );
    }

    Ok(provider_names)
}

/// Persist one `supervisor_notes` row per probed provider, and an additional
/// `warn`-severity row when `skip_preflight` is true. Best-effort — write
/// failures are logged but do not abort the run.
async fn write_preflight_supervisor_notes(
    store: &RunStore,
    run_id: &str,
    provider_names: &[String],
    skip_preflight: bool,
) {
    if skip_preflight {
        if let Err(e) = store
            .record_supervisor_note(
                run_id,
                "preflight",
                "warn",
                "provider preflight was bypassed via skip_preflight; provider reachability was NOT verified before this run",
            )
            .await
        {
            tracing::warn!(run_id, err = %e, "failed to write skip_preflight supervisor note");
        }
        return;
    }

    // When preflight ran and passed (we only reach here for non-failing
    // results — failures return early from start_run), write an `info`
    // note naming every provider that was verified reachable.
    if provider_names.is_empty() {
        return;
    }
    let summary = format!(
        "provider preflight passed: {} provider(s) verified reachable before launch ({})",
        provider_names.len(),
        provider_names.join(", "),
    );
    if let Err(e) = store
        .record_supervisor_note(run_id, "preflight", "info", &summary)
        .await
    {
        tracing::warn!(run_id, err = %e, "failed to write preflight-pass supervisor note");
    }
}

fn runtime_slots<'a>(
    strategy: &'a crate::strategies::Strategy,
    agent_slots: &'a [ResolvedAgentSlot],
) -> Vec<&'a crate::strategies::slot::LLMSlot> {
    if !agent_slots.is_empty() {
        return agent_slots.iter().map(|resolved| &resolved.slot).collect();
    }
    [
        strategy.trader_slot.as_ref(),
        strategy.intern_slot.as_ref(),
        strategy.regime_slot.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Pick the long-lived `agents.agent_id` of the agent acting as the
/// run's trader, for persistence in `eval_runs.agents_agent_id`
/// (migration 022). Prefers the AgentRef with canonical role `trader`;
/// falls back to the first AgentRef when no role match exists. Returns
/// `None` for legacy strategies that still use the deprecated slot
/// fields (no AgentRefs attached) — those rows leave the column NULL,
/// matching the no-backfill policy in the F-11 contract.
fn pick_agents_agent_id(strategy: &crate::strategies::Strategy) -> Option<String> {
    if let Some(r) = strategy
        .agents
        .iter()
        .find(|r| r.canonical_role().eq_ignore_ascii_case("trader"))
    {
        return Some(r.agent_id.clone());
    }
    strategy.agents.first().map(|r| r.agent_id.clone())
}

fn validate_eval_trader_source(
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
) -> ApiResult<()> {
    // QA22 / `strategy-require-at-least-one-agent`: the eval boundary
    // requires at least one attached agent. The legacy `trader_slot`
    // fallback that previously kept pre-refactor strategies runnable
    // was removed 2026-05-21 — the CLI `xvn strategy create` path has
    // been auto-migrating template slots to `AgentRef` at save time
    // since the strategies refactor, and the engine fixtures that
    // formerly relied on the fallback now seed real `Agent` rows
    // (see `strategy-require-at-least-one-agent-fixture-migration`).
    if strategy.agents.is_empty() {
        return Err(ApiError::Validation(format!(
            "strategy `{}` has no agent attached. At least one agent (with a `trader` role) is required to run an eval. Attach an agent in the Strategy Inspector or via `xvn agent attach`.",
            strategy.manifest.id
        )));
    }

    if agent_slots
        .iter()
        .any(|resolved| resolved.role.trim().eq_ignore_ascii_case("trader"))
    {
        return Ok(());
    }

    let roles = agent_slots
        .iter()
        .map(|resolved| resolved.role.trim())
        .filter(|role| !role.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    Err(ApiError::Validation(format!(
        "eval requires an attached agent with role `trader` for strategy `{}`. Attached roles: [{}]. Attach a trader agent in the Strategy Inspector or via `xvn agent attach`.",
        strategy.manifest.id, roles
    )))
}

fn validate_eval_provider_models(
    entry: &ProviderEntry,
    slots: &[&crate::strategies::slot::LLMSlot],
) -> ApiResult<()> {
    let mut saw_provider_slot = false;
    for slot in slots {
        let provider = slot
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|provider| !provider.is_empty())
            .ok_or_else(|| {
                ApiError::Validation(format!(
                    "eval requires an explicit provider/model on strategy role `{}`; no workspace default is assumed",
                    slot.role
                ))
            })?;
        if provider != entry.name {
            return Err(ApiError::Validation(format!(
                "eval currently requires all executable slots to use one provider; role `{}` uses `{provider}` but selected provider is `{}`",
                slot.role, entry.name
            )));
        }
        saw_provider_slot = true;
        let model = slot
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .ok_or_else(|| {
                let attested = slot.attested_with.trim();
                let attestation_hint = if attested.is_empty() {
                    String::new()
                } else {
                    format!(" Strategy was last attested with `{attested}` (informational only — does not gate binding).")
                };
                let enabled = if entry.enabled_models.is_empty() {
                    "No models are enabled for this provider.".to_string()
                } else {
                    format!("Enabled models: {}", entry.enabled_models.join(", "))
                };
                ApiError::Validation(format!(
                    "provider `{}` is selected for strategy role `{}`, but no explicit model is configured.{attestation_hint} {enabled}",
                    entry.name, slot.role
                ))
            })?;
        if entry.kind == ProviderKind::LocalCandle {
            continue;
        }
        if entry.enabled_models.is_empty() {
            return Err(ApiError::Validation(format!(
                "provider `{}` has no enabled models. Enable `{model}` or pick a configured provider/model before running eval.",
                entry.name
            )));
        }
        if !entry.enabled_models.iter().any(|enabled| enabled == model) {
            return Err(ApiError::Validation(format!(
                "provider `{}` is selected for strategy role `{}`, but model `{model}` is not enabled for that provider. Enabled models: {}",
                entry.name,
                slot.role,
                entry.enabled_models.join(", ")
            )));
        }
    }
    if saw_provider_slot {
        Ok(())
    } else {
        Err(ApiError::Validation(format!(
            "provider `{}` was selected for eval, but no executable strategy slot uses it.",
            entry.name
        )))
    }
}

async fn resolve_agent_slots(
    ctx: &ApiContext,
    strategy: &crate::strategies::Strategy,
) -> ApiResult<Vec<ResolvedAgentSlot>> {
    if strategy.agents.is_empty() {
        return Ok(Vec::new());
    }

    let agent_store = AgentStore::new(ctx.db.clone());
    let mut out = Vec::with_capacity(strategy.agents.len());
    for agent_ref in &strategy.agents {
        let agent = agent_store
            .get(&agent_ref.agent_id)
            .await
            .map_err(|e| ApiError::Internal(format!("load agent {}: {e}", agent_ref.agent_id)))?
            .ok_or_else(|| ApiError::NotFound(format!("agent {}", agent_ref.agent_id)))?;
        let slot = agent.slots.first().ok_or_else(|| {
            ApiError::Validation(format!("agent {} has no executable slots", agent.agent_id))
        })?;
        out.push(ResolvedAgentSlot {
            role: agent_ref.role.clone(),
            slot: agent_slot_to_llm_slot(&agent_ref.role, slot),
            system_prompt: slot.system_prompt.clone(),
            max_tokens: slot.resolve_max_tokens(),
            temperature: slot.temperature,
            inputs_policy: slot.inputs_policy,
            bar_history_limit: slot.bar_history_limit,
            memory_mode: slot.memory_mode,
            agent_id: agent.agent_id.clone(),
            noop_skip: slot.noop_skip.unwrap_or(true),
        });
    }
    Ok(out)
}

async fn dispatch_from_provider(entry: &ProviderEntry) -> ApiResult<Arc<dyn LlmDispatch>> {
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(&entry.api_key_env).map_err(|_| {
            ApiError::Validation(format!(
                "no API key for provider `{}` (env var {} is unset). Paste a key in Settings → Providers or export {} before running eval.",
                entry.name, entry.api_key_env, entry.api_key_env
            ))
        })?
    };
    if api_key.is_empty() && entry.kind != ProviderKind::LocalCandle {
        return Err(ApiError::Validation(format!(
            "provider `{}` has no API key set. Paste one in Settings → Providers.",
            entry.name
        )));
    }
    match entry.kind {
        ProviderKind::Anthropic => Ok(Arc::new(AnthropicDispatch::new(api_key))),
        ProviderKind::OpenaiCompat => Ok(Arc::new(OpenaiCompatDispatch::new(
            entry.base_url.clone(),
            api_key,
        ))),
        ProviderKind::LocalCandle => Ok(Arc::new(MockDispatch::echo(
            r#"{"action":"hold","conviction":0.0,"justification":"local-candle deterministic hold"}"#,
        ))),
    }
}

fn runtime_config_path(ctx: &ApiContext) -> std::path::PathBuf {
    if let Ok(p) = std::env::var("XVN_CONFIG_PATH") {
        if !p.is_empty() {
            return p.into();
        }
    }
    ctx.xvn_home.join("config").join("default.toml")
}

/// Load every configured provider's cached catalog once per eval run.
/// The observability emitter uses these for `model_calls.cost_usd`, and
/// context-overflow recovery uses them to choose a cheap summarizer
/// model. Missing / never-fetched catalogs are skipped silently. We
/// deliberately do NOT trigger a network refresh here: eval runs must
/// not hang on catalog fetches.
async fn load_provider_catalogs(
    ctx: &ApiContext,
) -> std::collections::HashMap<String, std::sync::Arc<xvision_core::providers::Catalog>> {
    use std::collections::HashMap;
    let cfg_path = runtime_config_path(ctx);
    let cfg = match tokio::task::spawn_blocking(move || config::load_runtime(&cfg_path)).await {
        Ok(Ok(c)) => c,
        // Config load failures are not the cost path's problem —
        // upstream handlers surface their own validation errors. Just
        // skip catalog wiring so emit-time cost is None.
        _ => return HashMap::new(),
    };
    let svc = match crate::providers::CatalogService::new(ctx.xvn_home.clone()) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    let mut out = HashMap::new();
    for p in &cfg.providers {
        if matches!(p.kind, ProviderKind::LocalCandle) {
            // local-candle has no remote catalog and no pricing.
            continue;
        }
        if let Ok(Some(cat)) = svc.get_or_load(&p.name).await {
            out.insert(p.name.clone(), cat);
        }
    }
    out
}

/// Testable / deps-injecting variant of `run`. Tests pass a
/// `MockBrokerSurface` + `MockDispatch` so no network is required;
/// production callers go through `run` which constructs deps from env.
///
/// `broker` is `Some` for paper mode and ignored for backtest mode.
/// Paper mode without a broker returns `ApiError::Validation`.
pub async fn run_with_deps(
    ctx: &ApiContext,
    req: EvalRunRequest,
    broker: Option<Arc<dyn BrokerSurface>>,
    dispatch: Arc<dyn LlmDispatch>,
    findings_model: String,
    tools: Arc<ToolRegistry>,
) -> ApiResult<Run> {
    let started = Instant::now();
    let target_clone = format!("{}@{}", req.agent_id, req.scenario_id);
    let args_json = serde_json::to_string(&req).ok();

    let result = run_inner(ctx, req, broker, dispatch, findings_model, tools).await;

    let (outcome, target) = match &result {
        Ok(run) => (Outcome::Ok, Some(run.id.clone())),
        Err(e) => (Outcome::Error(e.to_string()), None),
    };
    let _ = audit::record(
        ctx,
        "eval",
        "run",
        target.as_deref().or(Some(target_clone.as_str())),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn run_inner(
    ctx: &ApiContext,
    req: EvalRunRequest,
    broker: Option<Arc<dyn BrokerSurface>>,
    dispatch: Arc<dyn LlmDispatch>,
    findings_model: String,
    tools: Arc<ToolRegistry>,
) -> ApiResult<Run> {
    // 1. Look up the strategy. Propagates ApiError::NotFound cleanly.
    let strategy = api_strategy::get(ctx, &req.agent_id).await?;
    let agent_slots = resolve_agent_slots(ctx, &strategy).await?;
    validate_eval_trader_source(&strategy, &agent_slots)?;

    // 2. Look up the scenario. Primary path is the DB-backed registry
    //    (`api::scenario::get`); legacy path falls back to the compiled-in
    //    `canonical_scenarios()` for test contexts that haven't applied
    //    migration 006 yet (and for un-migrated legacy ids).
    let (scenario, from_db) = resolve_scenario_with_source(ctx, &req.scenario_id).await?;

    // 2b. QA15 warmup preflight: warn if the scenario doesn't carry as
    //     many warmup bars as the strategy's indicator periods imply.
    //     Soft signal — the run continues; the executor will just see a
    //     shorter `bar_history` slice at bar 1.
    warn_on_warmup_mismatch(&scenario, &strategy);

    // 4. Build a fresh Run, persist, then drive the executor. The
    //    `run.id` must exist before we construct the observability
    //    emitter so SpanStarted events have a valid FK.
    let mut run = Run::new_queued(req.agent_id.clone(), scenario.id.clone(), req.mode);
    run.params_override = req.params_override.clone();
    // F-11: persist the long-lived workspace `agents.agent_id` next to
    // the existing bundle-hash `agent_id`. Migration 021 added the
    // column; `pick_agents_agent_id` returns `None` for legacy
    // slot-only strategies, leaving the column NULL (no backfill).
    run.agents_agent_id = pick_agents_agent_id(&strategy);

    // Observability emitter (`qa-eval-observability-wiring`). Built
    // only when the dashboard injected an obs bus on the ApiContext;
    // CLI and tests run without it and emission is a no-op. `RunStarted`
    // is published below — only after the `eval_runs` row exists and
    // executor preflight has succeeded — so the recorder's
    // `agent_runs.eval_run_id` FK is valid and a preflight failure can't
    // leave a phantom observability run behind.
    // Load provider catalogs ONCE so observability can compute token
    // cost and context-overflow recovery can choose a cheap summarizer
    // model. Best-effort: providers without a cached catalog are
    // skipped and both consumers fall back to None/no-recovery.
    let provider_catalogs = load_provider_catalogs(ctx).await;
    let obs_catalogs = if ctx.obs_event_bus.is_some() {
        provider_catalogs.clone()
    } else {
        std::collections::HashMap::new()
    };
    let obs_emitter = ctx.obs_event_bus.as_ref().map(|bus| {
        // `harness-payload-blob-write`: attach the BlobStore so
        // `emit_model_call_finished_with_payloads` can persist
        // prompt + response bodies under FullDebug / Redacted
        // retention. Blob root mirrors the dashboard's resolution
        // at `$xvn_home/agent_runs/blobs/` so the existing
        // `GET /api/agent-runs/:id/blobs/:ref` route serves the
        // exact files this writer produces.
        let blob_store = xvision_observability::BlobStore::new(ctx.xvn_home.join("agent_runs").join("blobs"));
        crate::agent::observability::ObsEmitter::new(bus.clone(), run.id.clone())
            .with_retention(crate::agent::observability::ObsRetentionPolicy::from_config(
                &ctx.obs_config,
            ))
            .with_blob_store(blob_store)
            .with_catalogs(obs_catalogs.clone())
    });

    // 3. Pick the executor for this run mode. For backtest mode, when the
    //    scenario came from the DB we try to source bars through the
    //    cache wrapper (`eval::bars::load_bars`); on miss / fetch error
    //    we fall back to the legacy `data/probes/<cache_key>.parquet`
    //    loader so existing test fixtures keep working.
    let executor: Box<dyn Executor> = match req.mode {
        RunMode::Paper => {
            let b = broker.ok_or_else(|| ApiError::Validation("paper mode requires a broker".into()))?;
            build_paper_executor(
                ctx,
                &scenario,
                from_db,
                b,
                obs_emitter.clone(),
                provider_catalogs.clone(),
                req.limits.as_ref(),
            )
            .await?
        }
        RunMode::Backtest => {
            build_backtest_executor(
                ctx,
                &scenario,
                from_db,
                obs_emitter.clone(),
                provider_catalogs.clone(),
                req.limits.as_ref(),
            )
            .await?
        }
    };

    let store = RunStore::new(ctx.db.clone());
    store
        .create(&run)
        .await
        .map_err(|e| ApiError::Internal(format!("create run: {e}")))?;
    let started = store
        .begin_running(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("begin run: {e}")))?;
    if !started {
        let stopped = store
            .get(&run.id)
            .await
            .map_err(|e| ApiError::Internal(format!("re-read stopped run: {e}")))?;
        return Ok(stopped);
    }
    run.status = RunStatus::Running;

    // With the `eval_runs` row persisted and the executor built, register
    // the observability run. From here, any executor failure emits
    // `RunFinished{Failed}` below; a successful run emits
    // `RunFinished{Completed}` after finalize.
    if let Some(em) = obs_emitter.as_ref() {
        let objective = format!(
            "eval:{mode:?}:{scenario}",
            mode = req.mode,
            scenario = scenario.id,
        );
        em.emit_run_started(objective, ctx.obs_config.retention.mode.as_db_str())
            .await;
    }

    // Clone the dispatch Arc so we can reuse it for the post-finalize
    // findings extraction below without re-paying client setup.
    let dispatch_for_postprocess = dispatch.clone();

    if let Err(e) = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &agent_slots,
            dispatch,
            tools,
            &store,
        )
        .await
    {
        // Persist the failure so downstream callers (CLI, dashboard) can
        // see why this run is not Completed. Route through the
        // `FinalizeWriter` so concurrent finalize storms collapse into
        // batched UPDATEs — fall back to the direct `RunStore` path if
        // the queue is full or the writer has shut down so we never
        // lose a finalize.
        let err_msg = e.to_string();
        route_mark_failed(ctx, &store, &run.id, &err_msg).await;
        // Index the failed run so it shows up in ⌘K with its current status
        // — operators frequently want to find a recently-failed run by id
        // prefix without leaving the palette.
        if let Ok(failed) = store.get(&run.id).await {
            api_search::upsert_run(ctx, &failed).await;
        }
        if let Some(em) = obs_emitter.as_ref() {
            em.emit_run_finished(xvision_observability::RunStatus::Failed, Some(err_msg.clone()))
                .await;
        }
        return Err(ApiError::Internal(format!("executor: {err_msg}")));
    }

    if let Some(em) = obs_emitter.as_ref() {
        em.emit_run_finished(xvision_observability::RunStatus::Completed, None)
            .await;
    }

    // Re-read from the store so the returned Run reflects the canonical
    // post-finalize state — completed_at + metrics_json are set inside
    // RunStore::finalize and we want callers to see them.
    let mut finalized = store
        .get(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("re-read finalized run: {e}")))?;

    // V2E item 25: enrich the finalized metrics with inference cost aggregate.
    // Best-effort — enrichment failures never surface to the caller (the run
    // already completed; we don't want a DB join failure to retroactively
    // fail it). Capital initial comes from the scenario; we read it here to
    // denominate the net_return_pct in the same "% of starting capital" units
    // as gross_return_pct.
    enrich_with_inference_cost(ctx, &store, &mut finalized, &scenario).await;

    api_search::upsert_run(ctx, &finalized).await;

    // Postprocess: drive the findings extractor against the finalized run,
    // persist + index any findings. Best-effort — extractor failures
    // (LLM timeout, parse error) log + audit but never fail the run.
    // Reuses the same dispatch instance so we don't re-pay client setup.
    crate::eval::postprocess::extract_and_record(
        ctx,
        &finalized.id,
        dispatch_for_postprocess,
        &findings_model,
    )
    .await;

    // Rule-based auto-review. Reads the just-persisted findings and
    // writes a single `eval_reviews` row with a verdict + score. No
    // LLM call, no dispatch dependency. Best-effort by design —
    // failures log warn! and the run stays successful.
    let store_for_auto = RunStore::new(ctx.db.clone());
    crate::eval::review::auto::fire_auto_review(&store_for_auto, &finalized.id).await;

    // Guardrail rewrite summary (eval-guardrail-log-collapse). Reads
    // guard-role supervisor_notes, emits one tracing::warn! and one
    // eval_findings row summarising the rewrite rate. Best-effort.
    let store_for_guard = RunStore::new(ctx.db.clone());
    crate::eval::guardrail_summary::fire_guardrail_summary(&store_for_guard, &finalized.id).await;

    Ok(finalized)
}

/// Enrich a completed run's `MetricsSummary` with inference cost aggregate and
/// `net_return_pct` (V2E item 25). Best-effort — any failure is logged and
/// swallowed; the run keeps its existing metrics unchanged.
///
/// Emits `inference_cost_dominates_return` finding when the cost-dominance
/// threshold is exceeded (annotate-only, does not block the run).
async fn enrich_with_inference_cost(
    ctx: &ApiContext,
    store: &RunStore,
    run: &mut Run,
    scenario: &crate::eval::scenario::Scenario,
) {
    let Some(mut metrics) = run.metrics.clone() else {
        return; // run failed before finalize
    };

    // Aggregate per-call cost_usd. Returns None when the observability tables
    // aren't available or all calls have NULL cost (model not in catalog).
    let inference_cost = aggregate_eval_run_inference_cost(&ctx.db, &run.id).await;

    // Capital initial from the scenario's capital spec.
    let capital_initial = scenario.capital.initial;

    // net_return_pct = gross_return_pct − (inference_cost / capital × 100)
    let net = compute_net_return_pct(metrics.total_return_pct, inference_cost, capital_initial);

    metrics.inference_cost_quote_total = inference_cost;
    metrics.net_return_pct = net;

    // Persist the enriched metrics to the DB.
    if let Err(e) = store.patch_metrics(&run.id, &metrics).await {
        tracing::warn!(
            run_id = %run.id,
            error = %e,
            "enrich_with_inference_cost: patch_metrics failed (best-effort; run keeps existing metrics)",
        );
        return;
    }
    run.metrics = Some(metrics.clone());

    // Emit inference_cost_dominates_return finding when threshold is crossed.
    if let Some(cost) = inference_cost {
        let gross_return_quote = capital_initial * metrics.total_return_pct / 100.0;
        if inference_cost_dominates(gross_return_quote, cost, INFERENCE_COST_DOMINANCE_THRESHOLD) {
            let ratio = if gross_return_quote.abs() > f64::EPSILON {
                cost.abs() / gross_return_quote.abs()
            } else {
                f64::INFINITY
            };
            let payload = InferenceCostDominatesReturnPayload {
                ratio,
                threshold: INFERENCE_COST_DOMINANCE_THRESHOLD,
                gross_return_quote,
                inference_cost_quote_total: cost,
            };
            let evidence = match serde_json::to_value(&payload) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(run_id = %run.id, error = %e, "failed to serialize inference_cost finding payload");
                    return;
                }
            };
            let finding = Finding {
                id: ulid::Ulid::new().to_string(),
                run_id: run.id.clone(),
                kind: "inference_cost_dominates_return".into(),
                severity: Severity::Warning,
                summary: format!(
                    "LLM inference cost (${cost:.4}) exceeds {:.0}% of gross trading return (${:.4}); net return may be negative.",
                    INFERENCE_COST_DOMINANCE_THRESHOLD * 100.0,
                    gross_return_quote.abs(),
                ),
                evidence,
                extracted_at: chrono::Utc::now(),
                schema_version: crate::eval::findings::FINDING_SCHEMA_VERSION.to_string(),
                evidence_cycle_ids: Some(vec![]),
                produced_by_check: Some("metrics:cost_dominance".to_string()),
                eval_review_id: None,
                review_type: None,
                confidence: None,
                title: Some("Inference cost dominates return".into()),
                description: Some(format!(
                    "produced_by_check=metrics:cost_dominance ratio={ratio:.3} threshold={t}",
                    t = INFERENCE_COST_DOMINANCE_THRESHOLD,
                )),
                recommendation: Some(
                    "Consider using a cheaper model for this strategy, or increase capital to dilute the per-decision cost.".into(),
                ),
                created_at: Some(chrono::Utc::now()),
            };
            if let Err(e) = store.record_finding(&finding).await {
                tracing::warn!(
                    run_id = %run.id,
                    error = %e,
                    "enrich_with_inference_cost: record finding failed (best-effort)",
                );
            }
        }
    }
}

/// Resolve a scenario id to a `Scenario`. Tries the DB-backed registry
/// first (`api::scenario::get`); on `NotFound` (or on store errors —
/// typically a test context without migration 006 applied), falls back
/// to the compiled-in legacy `canonical_scenarios()` set so existing
/// tests and pre-Task-6 caches keep working.
async fn resolve_scenario(ctx: &ApiContext, id: &str) -> ApiResult<Scenario> {
    let (s, _from_db) = resolve_scenario_with_source(ctx, id).await?;
    Ok(s)
}

/// Same as `resolve_scenario` but also reports whether the row came from
/// the DB (primary path) or from the compiled-in legacy fallback. The
/// caller uses this to decide between routing bars through
/// `eval::bars::load_bars` (DB path) or the legacy fixture loader.
async fn resolve_scenario_with_source(ctx: &ApiContext, id: &str) -> ApiResult<(Scenario, bool)> {
    match api_scenario::get(ctx, id).await {
        Ok(s) => Ok((s, true)),
        Err(_) => {
            #[allow(deprecated)]
            let legacy = canonical_scenarios()
                .into_iter()
                .find(|s| s.id == id)
                .ok_or_else(|| ApiError::NotFound(format!("scenario '{id}'")))?;
            Ok((legacy, false))
        }
    }
}

/// Source bars for a DB-resolved scenario via the cache wrapper. The
/// returned bars feed `BacktestExecutor::with_bars`. Errors surface
/// fetch / cache failures so the caller can decide whether to fall
/// back to the legacy fixture loader.
async fn load_bars_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
) -> ApiResult<Vec<xvision_data::alpaca::MarketBar>> {
    let asset = scenario
        .asset
        .first()
        .ok_or_else(|| ApiError::Validation(format!("scenario '{}' has empty asset list", scenario.id)))?
        .venue_symbol
        .clone();
    crate::eval::bars::load_bars(
        ctx,
        &crate::eval::bars::BarCacheArgs {
            cache_key: scenario.bar_cache_policy.cache_key.clone(),
            asset_pair: asset,
            granularity: scenario.granularity,
            start: scenario.time_window.start,
            end: scenario.time_window.end,
            data_source_tag: "alpaca-historical-v1".into(),
        },
    )
    .await
}

/// Pre-fetch the warmup window for a scenario. Returns an empty Vec when
/// `scenario.warmup_bars == 0`. Errors surface as
/// `ApiError::Validation(..)` with the actionable "run `xvn bars fetch`
/// first" hint so eval preflight can wrap them into the QA15 cache-miss
/// preflight error.
async fn load_warmup_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
) -> ApiResult<Vec<xvision_data::alpaca::MarketBar>> {
    let asset = scenario
        .asset
        .first()
        .ok_or_else(|| ApiError::Validation(format!("scenario '{}' has empty asset list", scenario.id)))?
        .venue_symbol
        .clone();
    crate::eval::bars::load_warmup_bars(
        ctx,
        &asset,
        scenario.granularity,
        scenario.time_window.start,
        scenario.warmup_bars,
    )
    .await
    .map_err(|e| match e {
        ApiError::Validation(msg) => ApiError::Validation(format!(
            "warmup-bars preflight failed for scenario '{}': {}. Pre-fetch the warmup window with `xvn bars fetch --asset {} --granularity {} --from <warmup_start> --to {}` before running.",
            scenario.id,
            msg,
            asset,
            scenario.granularity.as_alpaca_str(),
            scenario.time_window.start.to_rfc3339(),
        )),
        other => other,
    })
}

fn market_bars_to_ohlcv(bars: Vec<xvision_data::alpaca::MarketBar>) -> Vec<Ohlcv> {
    bars.into_iter()
        .map(|b| Ohlcv {
            timestamp: b.timestamp,
            open: b.open,
            high: b.high,
            low: b.low,
            close: b.close,
            volume: b.volume,
        })
        .collect()
}

async fn load_ohlcv_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
    from_db: bool,
) -> ApiResult<Vec<Ohlcv>> {
    if from_db {
        return load_bars_for_scenario(ctx, scenario)
            .await
            .map(market_bars_to_ohlcv);
    }

    let asset = scenario
        .asset
        .first()
        .ok_or_else(|| ApiError::Validation(format!("scenario '{}' has empty asset list", scenario.id)))?
        .venue_symbol
        .clone();
    let mut bars = load_ohlcv_fixture(&scenario.bar_cache_policy.cache_key, &asset, usize::MAX).map_err(|e| {
        ApiError::Validation(format!(
            "scenario '{}' is missing historical bars for paper eval. Fetch/cache bars before starting paper mode: {e}",
            scenario.id
        ))
    })?;
    let overlaps_window = bars
        .iter()
        .any(|b| b.timestamp >= scenario.time_window.start && b.timestamp < scenario.time_window.end);
    if !overlaps_window {
        let step = chrono::Duration::seconds(scenario.granularity.seconds() as i64);
        for (idx, bar) in bars.iter_mut().enumerate() {
            bar.timestamp = scenario.time_window.start + step * idx as i32;
        }
    }
    Ok(bars)
}

async fn build_paper_executor(
    ctx: &ApiContext,
    scenario: &Scenario,
    from_db: bool,
    broker: Arc<dyn BrokerSurface>,
    obs: Option<crate::agent::observability::ObsEmitter>,
    provider_catalogs: std::collections::HashMap<String, std::sync::Arc<xvision_core::providers::Catalog>>,
    limits: Option<&crate::eval::limits::EvalLimits>,
) -> ApiResult<Box<dyn Executor>> {
    let bars = load_ohlcv_for_scenario(ctx, scenario, from_db).await?;
    let warmup = if from_db {
        market_bars_to_ohlcv(load_warmup_for_scenario(ctx, scenario).await?)
    } else {
        // Legacy / fixture path: no separate warmup cache wrapper. The
        // fixture is already a wider window, and the trader sees only the
        // current bar in the seed today, so we don't synthesize warmup
        // here — that's only meaningful for DB-resolved scenarios that
        // can pull a real pre-window from the bars cache.
        Vec::new()
    };
    let min_notional = paper_min_notional_usd(ctx);
    let mut paper = PaperExecutor::with_bars(broker, bars)
        .with_warmup(warmup)
        .with_event_bus(ctx.event_bus.clone())
        .with_provider_catalogs(provider_catalogs)
        .with_min_notional_usd(min_notional);
    if let Some(emitter) = obs {
        paper = paper.with_observability(emitter);
    }
    // V2D: thread the server-built recorder onto the executor so per-slot
    // `memory_mode = AgentScoped` actually emits recall/write events. The
    // recorder treats `Off` as a no-op, so legacy strategies are unaffected.
    if let Some(recorder) = ctx.memory_recorder.clone() {
        paper = paper.with_memory_recorder(recorder);
    }
    if let Some(l) = limits {
        paper = paper.with_limits(l.clone());
    }
    Ok(Box::new(paper))
}

/// Resolve the `paper` venue's `min_notional_usd` from the active risk
/// config (`$XVN_HOME/config/risk.toml`, with `XVN_RISK_CONFIG_PATH`
/// override). Returns `0.0` (rule no-op) when the file is missing,
/// fails to parse, or has no `[venues.paper]` entry — matching the
/// "absent venue → pass-through" contract from PR #324.
///
/// Plumbing choice: a per-run, best-effort read at executor-build
/// time. Avoids threading a `RiskConfig` handle through `ApiContext`
/// (which today holds no risk-layer state), and the file is small
/// (~30 lines) so the read is negligible next to executor setup.
/// Failure paths log and fall back to 0.0 — never panic, never
/// bubble. The risk-layer crate already validates the file at the
/// top of every run, so production paths see a well-formed config.
fn paper_min_notional_usd(ctx: &ApiContext) -> f64 {
    let path = if let Ok(p) = std::env::var("XVN_RISK_CONFIG_PATH") {
        if !p.is_empty() {
            std::path::PathBuf::from(p)
        } else {
            ctx.xvn_home.join("config").join("risk.toml")
        }
    } else {
        ctx.xvn_home.join("config").join("risk.toml")
    };
    if !path.exists() {
        return 0.0;
    }
    match xvision_risk::config::RiskConfig::from_path(&path) {
        Ok(cfg) => cfg.venue_limits("paper").min_notional_usd,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to load risk config for MinNotional wiring; defaulting to 0.0 (rule no-op)"
            );
            0.0
        }
    }
}

async fn build_backtest_executor(
    ctx: &ApiContext,
    scenario: &Scenario,
    from_db: bool,
    obs: Option<crate::agent::observability::ObsEmitter>,
    provider_catalogs: std::collections::HashMap<String, std::sync::Arc<xvision_core::providers::Catalog>>,
    limits: Option<&crate::eval::limits::EvalLimits>,
) -> ApiResult<Box<dyn Executor>> {
    if from_db {
        match load_bars_for_scenario(ctx, scenario).await {
            Ok(bars) => {
                let ohlcv: Vec<xvision_core::market::Ohlcv> = bars
                    .into_iter()
                    .map(|b| xvision_core::market::Ohlcv {
                        timestamp: b.timestamp,
                        open: b.open,
                        high: b.high,
                        low: b.low,
                        close: b.close,
                        volume: b.volume,
                    })
                    .collect();
                // Warmup is a hard preflight error when DB-resolved: an
                // operator who set `warmup_bars > 0` expects real
                // pre-window context, not silent emptiness.
                let warmup = market_bars_to_ohlcv(load_warmup_for_scenario(ctx, scenario).await?);
                let mut bt = BacktestExecutor::with_bars(ohlcv)
                    .with_warmup(warmup)
                    .with_event_bus(ctx.event_bus.clone())
                    .with_provider_catalogs(provider_catalogs);
                if let Some(emitter) = obs {
                    bt = bt.with_observability(emitter);
                }
                // V2D: thread the server-built recorder onto the executor.
                if let Some(recorder) = ctx.memory_recorder.clone() {
                    bt = bt.with_memory_recorder(recorder);
                }
                if let Some(l) = limits {
                    bt = bt.with_limits(l.clone());
                }
                return Ok(Box::new(bt));
            }
            Err(e) => {
                if scenario.warmup_bars > 0 || !legacy_fixture_exists(scenario) {
                    return Err(missing_bars_validation(scenario, Some(e.to_string())));
                }
                tracing::warn!(
                    scenario_id = %scenario.id,
                    error = %e,
                    "load_bars failed; falling back to fixture loader without warmup context",
                );
            }
        }
    } else if !legacy_fixture_exists(scenario) {
        return Err(missing_bars_validation(scenario, None));
    }

    let mut bt = BacktestExecutor::new()
        .with_event_bus(ctx.event_bus.clone())
        .with_provider_catalogs(provider_catalogs);
    if let Some(emitter) = obs {
        bt = bt.with_observability(emitter);
    }
    // V2D: thread the server-built recorder onto the executor.
    if let Some(recorder) = ctx.memory_recorder.clone() {
        bt = bt.with_memory_recorder(recorder);
    }
    if let Some(l) = limits {
        bt = bt.with_limits(l.clone());
    }
    Ok(Box::new(bt))
}

/// Emit a warning (via `tracing::warn`) when the scenario's
/// `warmup_bars` is below the strategy's `min_warmup_bars`. The QA15
/// spec calls for this to surface in eval preflight; today the operator
/// sees it in logs / SSE while we wire a richer surface in a follow-up.
fn warn_on_warmup_mismatch(scenario: &Scenario, strategy: &crate::strategies::Strategy) {
    let strat_min = strategy.min_warmup_bars();
    if scenario.warmup_bars < strat_min {
        tracing::warn!(
            scenario_id = %scenario.id,
            strategy_id = %strategy.manifest.id,
            scenario_warmup = scenario.warmup_bars,
            strategy_min_warmup = strat_min,
            "scenario warmup_bars below strategy min_warmup_bars; indicators may lack history at bar 1",
        );
    }
}

fn legacy_fixture_exists(scenario: &Scenario) -> bool {
    xvision_data::fixtures::fixture_path(&scenario.bar_cache_policy.cache_key).exists()
}

fn missing_bars_validation(scenario: &Scenario, source_error: Option<String>) -> ApiError {
    let mut msg = format!(
        "scenario '{}' is missing bars cache and legacy fixture for cache key '{}'. Fetch bars for this scenario before starting the backtest.",
        scenario.id, scenario.bar_cache_policy.cache_key
    );
    if let Some(e) = source_error {
        msg.push_str(&format!(" Last cache fetch error: {e}"));
    }
    ApiError::Validation(msg)
}

/// Non-blocking dashboard entrypoint. Validates the request, persists a
/// `Queued` run row, spawns a background task that drives the executor,
/// and returns the freshly-persisted `RunDetail`. The HTTP handler
/// returns in ~milliseconds; the run finishes in 3–10+ minutes and the
/// frontend polls `GET /api/eval/runs/:id` to track progress.
///
/// Sync-up-front validation: env vars (`ANTHROPIC_API_KEY`, Alpaca
/// creds in paper mode) are read before the spawn so missing-config
/// errors return as `ApiError::Validation` rather than landing in the
/// row's `error` field. Strategy/scenario lookups also happen up-front
/// for the same reason.
pub async fn start_run(ctx: &ApiContext, req: EvalRunRequest) -> ApiResult<RunDetail> {
    let started = Instant::now();
    let strategy = api_strategy::get(ctx, &req.agent_id).await?;
    let (scenario, from_db) = resolve_scenario_with_source(ctx, &req.scenario_id).await?;

    // Build broker / dispatch / tools from env up-front so any
    // missing-config errors return synchronously rather than landing in
    // a background-task failure row the user has to dig out of the list.
    let broker: Option<Arc<dyn BrokerSurface>> = match req.mode {
        RunMode::Paper => Some(build_alpaca_paper_broker(ctx).await?),
        RunMode::Backtest => None,
    };
    let agent_slots = resolve_agent_slots(ctx, &strategy).await?;
    validate_eval_trader_source(&strategy, &agent_slots)?;

    let provider_names = validate_provider_preflight(ctx, &req, &strategy, &agent_slots).await?;

    let (dispatch, findings_model) = build_eval_dispatch(ctx, &strategy, &agent_slots).await?;
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    // Other entry point (`run_with_deps_in_progress`) — observability
    // wiring is opt-in via the same ApiContext bus. The emitter is
    // built after `run.id` is available below; `RunStarted` is
    // published only after the `eval_runs` row exists and executor
    // preflight has succeeded, so the recorder's FK is valid and
    // preflight failures can't leave a phantom observability run
    // behind. The matching `RunFinished` is emitted by
    // `execute_in_background`.
    let mut run = Run::new_queued(req.agent_id.clone(), scenario.id.clone(), req.mode);
    // F-11: see comment in `run_inner` above — same reasoning here.
    run.agents_agent_id = pick_agents_agent_id(&strategy);
    // Same catalog-wiring as the synchronous run path above; see the
    // comment there for the rationale.
    let provider_catalogs = load_provider_catalogs(ctx).await;
    let obs_catalogs = if ctx.obs_event_bus.is_some() {
        provider_catalogs.clone()
    } else {
        std::collections::HashMap::new()
    };
    let obs_emitter = ctx.obs_event_bus.as_ref().map(|bus| {
        // Mirror the FullDebug-aware emitter wiring above; same
        // blob root so the second eval entry point produces refs
        // the dashboard's blob-fetch route resolves to.
        let blob_store = xvision_observability::BlobStore::new(ctx.xvn_home.join("agent_runs").join("blobs"));
        crate::agent::observability::ObsEmitter::new(bus.clone(), run.id.clone())
            .with_retention(crate::agent::observability::ObsRetentionPolicy::from_config(
                &ctx.obs_config,
            ))
            .with_blob_store(blob_store)
            .with_catalogs(obs_catalogs.clone())
    });

    let executor: Box<dyn Executor> = match req.mode {
        RunMode::Paper => {
            let b = broker.expect("paper mode broker built above");
            build_paper_executor(
                ctx,
                &scenario,
                from_db,
                b,
                obs_emitter.clone(),
                provider_catalogs.clone(),
                req.limits.as_ref(),
            )
            .await?
        }
        RunMode::Backtest => {
            build_backtest_executor(
                ctx,
                &scenario,
                from_db,
                obs_emitter.clone(),
                provider_catalogs.clone(),
                req.limits.as_ref(),
            )
            .await?
        }
    };

    run.params_override = req.params_override.clone();
    let store = RunStore::new(ctx.db.clone());
    store
        .create(&run)
        .await
        .map_err(|e| ApiError::Internal(format!("create run: {e}")))?;

    // Persist preflight results as supervisor_notes immediately after the run
    // row exists. Uses `info` severity for reachable providers and `warn` for
    // skip_preflight. Best-effort: a failed note write does NOT abort the run.
    write_preflight_supervisor_notes(&store, &run.id, &provider_names, req.skip_preflight).await;

    if let Some(em) = obs_emitter.as_ref() {
        let objective = format!(
            "eval:{mode:?}:{scenario}",
            mode = req.mode,
            scenario = scenario.id,
        );
        em.emit_run_started(objective, ctx.obs_config.retention.mode.as_db_str())
            .await;
    }

    let args_json = serde_json::to_string(&req).ok();
    let _ = audit::record(
        ctx,
        "eval",
        "start",
        Some(&run.id),
        args_json.as_deref(),
        Outcome::Ok,
        started.elapsed().as_millis() as i64,
    )
    .await;

    // F-1 (eval-launch-concurrency-cap, 2026-05-19): cap how many runs
    // can be in flight against a single upstream `(provider, model)`
    // bucket. Resolved from the trader slot (the dominant token spender);
    // findings/intern slots ride along on the same permit because the
    // F-1 audit (`team/intake/2026-05-16-eval-review-and-v2a.md`) tracked
    // the burst as a single user-perceived "launch". The guard is moved
    // into the spawned background task so it lives for the full run
    // lifecycle and is dropped (releasing the permit) when the task
    // exits — including via panic.
    let (gate_provider, gate_model) = resolve_launch_gate_key(&strategy, &agent_slots, &findings_model);
    let launch_permit = ctx.launch_gate.acquire(&gate_provider, &gate_model).await;

    let ctx_bg = ctx.clone();
    let run_id = run.id.clone();
    tokio::spawn(async move {
        // Hold the permit for the entire background task lifetime.
        // Dropping it releases the slot back to the gate; this must
        // outlive `execute_in_background` and `extract_and_record`.
        let _launch_permit = launch_permit;
        execute_in_background(
            ctx_bg,
            run,
            strategy,
            scenario,
            agent_slots,
            executor,
            dispatch,
            findings_model,
            tools,
            obs_emitter,
        )
        .await;
    });

    get_run(ctx, &run_id).await
}

/// Resolve the `(provider, model)` pair the launch-concurrency gate
/// should key on. Prefers the trader role from `agent_slots` (post-refactor
/// strategies), falls back to the legacy `trader_slot` on `Strategy`, then
/// to any other agent slot, then to the resolved `findings_model` as a
/// last-ditch source. Empty strings still produce *some* key — we'd rather
/// over-serialize a misconfigured strategy than skip the cap entirely.
fn resolve_launch_gate_key(
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
    findings_model: &str,
) -> (String, String) {
    // 1. Attached agent with role == "trader".
    if let Some(trader) = agent_slots
        .iter()
        .find(|resolved| resolved.role.trim().eq_ignore_ascii_case("trader"))
    {
        let provider = trader.slot.provider.clone().unwrap_or_default();
        let model = trader.slot.effective_model();
        if !provider.is_empty() && !model.is_empty() {
            return (provider, model);
        }
    }

    // 2. Legacy `trader_slot` on the strategy.
    if let Some(slot) = strategy.trader_slot.as_ref() {
        let provider = slot.provider.clone().unwrap_or_default();
        let model = slot.effective_model();
        if !provider.is_empty() && !model.is_empty() {
            return (provider, model);
        }
    }

    // 3. First attached agent with any non-empty provider/model.
    for resolved in agent_slots {
        let provider = resolved.slot.provider.clone().unwrap_or_default();
        let model = resolved.slot.effective_model();
        if !provider.is_empty() && !model.is_empty() {
            return (provider, model);
        }
    }

    // 4. Last-ditch: pair with the resolved findings model and an empty
    // provider. Better than skipping the cap; this only fires on a
    // misconfigured strategy that already shouldn't have reached
    // `start_run`.
    (String::new(), findings_model.to_string())
}

/// Background-task body: transition Queued → Running, drive the
/// executor, and on completion/failure persist the canonical state.
/// Detached — failures here can't propagate to the spawning request, so
/// every error path writes to the run row's `error` field and logs at
/// the `xvision::eval` target.
#[allow(clippy::too_many_arguments)]
async fn execute_in_background(
    ctx: ApiContext,
    mut run: Run,
    strategy: crate::strategies::Strategy,
    scenario: Scenario,
    agent_slots: Vec<ResolvedAgentSlot>,
    executor: Box<dyn Executor>,
    dispatch: Arc<dyn LlmDispatch>,
    findings_model: String,
    tools: Arc<ToolRegistry>,
    obs_emitter: Option<crate::agent::observability::ObsEmitter>,
) {
    let store = RunStore::new(ctx.db.clone());

    match store.begin_running(&run.id).await {
        Ok(true) => {
            run.status = RunStatus::Running;
        }
        Ok(false) => {
            if let Ok(terminal) = store.get(&run.id).await {
                api_search::upsert_run(&ctx, &terminal).await;
            }
            // Caller already advanced past Queued (e.g., cancel before
            // executor start). Emit Cancelled so SSE consumers don't
            // wait forever on /api/agent-runs/<eval_run_id>.
            if let Some(em) = obs_emitter.as_ref() {
                em.emit_run_finished(xvision_observability::RunStatus::Cancelled, None)
                    .await;
            }
            return;
        }
        Err(e) => {
            tracing::error!(
                target: "xvision::eval",
                run_id = %run.id,
                error = %e,
                "failed to transition Queued → Running",
            );
            if let Some(em) = obs_emitter.as_ref() {
                em.emit_run_finished(
                    xvision_observability::RunStatus::Failed,
                    Some(format!("failed to transition Queued → Running: {e}")),
                )
                .await;
            }
            return;
        }
    }

    let dispatch_for_postprocess = dispatch.clone();

    if let Err(e) = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &agent_slots,
            dispatch,
            tools,
            &store,
        )
        .await
    {
        let err_msg = format!("{e:#}");
        if matches!(store.is_cancelled(&run.id).await, Ok(true)) {
            if let Ok(cancelled) = store.get(&run.id).await {
                api_search::upsert_run(&ctx, &cancelled).await;
            }
            if let Some(em) = obs_emitter.as_ref() {
                em.emit_run_finished(xvision_observability::RunStatus::Cancelled, None)
                    .await;
            }
            return;
        }
        tracing::error!(
            target: "xvision::eval",
            run_id = %run.id,
            error = %e,
            error_chain = %err_msg,
            "executor failed",
        );
        route_mark_failed(&ctx, &store, &run.id, &err_msg).await;
        if let Ok(failed) = store.get(&run.id).await {
            api_search::upsert_run(&ctx, &failed).await;
        }
        if let Some(em) = obs_emitter.as_ref() {
            em.emit_run_finished(xvision_observability::RunStatus::Failed, Some(err_msg))
                .await;
        }
        return;
    }

    // TODO(F-1 follow-up / #345): serialize finalize writes across concurrent
    // eval runs that share the same (provider, model) slot. When many runs
    // complete simultaneously, concurrent `store.finalize` + `upsert_run`
    // calls can contend on the SQLite write lock and leave some runs in a
    // "stuck running" state. PR #345 (eval-run-watchdog-and-stuck-running,
    // F-3) already touches this path — add write batching there to avoid a
    // merge conflict here.
    let mut finalized = match store.get(&run.id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(
                target: "xvision::eval",
                run_id = %run.id,
                error = %e,
                "failed to re-read finalized run",
            );
            if let Some(em) = obs_emitter.as_ref() {
                em.emit_run_finished(
                    xvision_observability::RunStatus::Failed,
                    Some(format!("failed to re-read finalized run: {e}")),
                )
                .await;
            }
            return;
        }
    };

    // V2E item 25: enrich with inference cost aggregate (best-effort).
    enrich_with_inference_cost(&ctx, &store, &mut finalized, &scenario).await;

    api_search::upsert_run(&ctx, &finalized).await;
    if let Some(em) = obs_emitter.as_ref() {
        em.emit_run_finished(xvision_observability::RunStatus::Completed, None)
            .await;
    }

    // Best-effort findings extraction — failures audit but don't reopen
    // the run.
    crate::eval::postprocess::extract_and_record(
        &ctx,
        &finalized.id,
        dispatch_for_postprocess,
        &findings_model,
    )
    .await;

    // Rule-based auto-review postprocess. Best-effort; reads the
    // findings we just persisted and writes a single eval_reviews row.
    let store_for_auto = RunStore::new(ctx.db.clone());
    crate::eval::review::auto::fire_auto_review(&store_for_auto, &finalized.id).await;

    // Guardrail rewrite summary (eval-guardrail-log-collapse). Best-effort.
    let store_for_guard = RunStore::new(ctx.db.clone());
    crate::eval::guardrail_summary::fire_guardrail_summary(&store_for_guard, &finalized.id).await;
}

/// Route a single `mark_failed` write through `ApiContext::finalize_writer`
/// so concurrent finalize storms (the 27-runs-in-15s pattern captured in
/// the 2026-05-19 audit) collapse into batched UPDATEs. If the writer's
/// bounded channel is full or the receiver has shut down, fall back to
/// the direct `RunStore::fail_active` path so we never lose a finalize.
async fn route_mark_failed(ctx: &ApiContext, store: &RunStore, run_id: &str, err_msg: &str) {
    let completed_at = Utc::now();
    match ctx
        .finalize_writer
        .send_mark_failed(run_id.to_string(), err_msg.to_string(), completed_at)
        .await
    {
        Ok(()) => {}
        Err(e) => {
            tracing::warn!(
                target: "xvision::eval",
                run_id = %run_id,
                error = %e,
                "finalize_writer failed; falling back to direct fail_active",
            );
            let _ = store.fail_active(run_id, err_msg).await;
        }
    }
}

/// Sweep any `Queued` or `Running` rows from a previous process and
/// transition them to `Failed`. Background tasks die with the dashboard
/// process so a clean restart should fail orphans out before serving
/// traffic — otherwise the runs list shows phantom "Running" rows.
///
/// Stays on the direct `RunStore` path (not the `FinalizeWriter`)
/// because it fires at most once per process start, so it never
/// produces a burst. Routing through the writer would just add
/// boot-time complexity for no batching benefit.
pub async fn fail_orphan_runs(ctx: &ApiContext) -> ApiResult<u64> {
    let store = RunStore::new(ctx.db.clone());
    store
        .fail_active_runs("daemon restarted before run completed")
        .await
        .map_err(|e| ApiError::Internal(format!("fail orphan runs: {e}")))
}

/// Default values for the retention janitor when no env override is set.
///
/// These bound the disk footprint of the agent-run observability blob
/// store. The audit on 2026-05-19 found 5,568 blobs in
/// `/data/agent_runs/blobs/` because the janitor was implemented but
/// never spawned — see `crates/xvision-observability/src/janitor.rs`.
///
/// - `payload_ttl_days = 14` matches the team's stated 2-week retention
///   target for full-debug trace payloads.
/// - `max_payload_bytes = 4 GB` is the per-host disk-budget cap. When
///   the blob store grows past this, the janitor evicts in
///   mtime-ascending order until the store is back under the cap.
/// - `tick = 1 hour` keeps the bookkeeping cost negligible while
///   ensuring nothing past TTL lingers for more than an hour.
pub const JANITOR_DEFAULT_TTL_DAYS: u64 = 14;
pub const JANITOR_DEFAULT_MAX_BYTES: u64 = 4_000_000_000;
pub const JANITOR_DEFAULT_TICK_SECS: u64 = 60 * 60;

/// Resolve the janitor configuration from environment variables, falling
/// back to the documented defaults above. Exposed for tests so they can
/// assert env-override behaviour without spawning the task.
pub fn resolve_janitor_config_from_env() -> (xvision_observability::JanitorConfig, std::time::Duration) {
    let ttl_days = std::env::var("XVN_PAYLOAD_TTL_DAYS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(JANITOR_DEFAULT_TTL_DAYS);
    let max_bytes = std::env::var("XVN_MAX_PAYLOAD_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(JANITOR_DEFAULT_MAX_BYTES);
    let tick_secs = std::env::var("XVN_JANITOR_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(JANITOR_DEFAULT_TICK_SECS);
    (
        xvision_observability::JanitorConfig {
            payload_ttl_days: ttl_days,
            max_payload_bytes: max_bytes,
        },
        std::time::Duration::from_secs(tick_secs.max(1)),
    )
}

/// Spawn the retention janitor as a periodic background task at engine
/// boot. The handle is returned so the caller can `abort()` it at
/// process shutdown; in practice the dashboard's `serve` lets it run
/// for the whole process lifetime.
///
/// Behaviour:
/// - Reads TTL + max-bytes from env (`XVN_PAYLOAD_TTL_DAYS`,
///   `XVN_MAX_PAYLOAD_BYTES`); defaults documented on
///   [`JANITOR_DEFAULT_TTL_DAYS`] / [`JANITOR_DEFAULT_MAX_BYTES`].
/// - Builds the blob store at `$xvn_home/agent_runs/blobs/` — same path
///   the eval emitter writes to.
/// - If the blob root is missing it logs and silently skips (no panic).
///   We try `create_dir_all` first so the common "fresh install"
///   case still gets a running janitor.
///
/// Returns `None` when no task was spawned (blob root missing AND
/// couldn't be created); otherwise the `JoinHandle` of the periodic
/// task.
pub fn spawn_retention_janitor(ctx: &ApiContext) -> Option<tokio::task::JoinHandle<()>> {
    let blob_root = ctx.xvn_home.join("agent_runs").join("blobs");
    // Best-effort: create the dir so the very first boot on a fresh
    // host still gets a running janitor. If creation fails (read-only
    // mount, permissions), log and skip — never panic.
    if !blob_root.exists() {
        if let Err(e) = std::fs::create_dir_all(&blob_root) {
            tracing::warn!(
                target: "xvision_engine::janitor",
                blob_root = %blob_root.display(),
                error = %e,
                "retention janitor skipped: blob root does not exist and could not be created"
            );
            return None;
        }
    }
    let blob_store = xvision_observability::BlobStore::new(blob_root.clone());
    let (config, interval) = resolve_janitor_config_from_env();
    tracing::info!(
        target: "xvision_engine::janitor",
        blob_root = %blob_root.display(),
        payload_ttl_days = config.payload_ttl_days,
        max_payload_bytes = config.max_payload_bytes,
        tick_secs = interval.as_secs(),
        "retention janitor spawned"
    );
    Some(xvision_observability::spawn_janitor(
        ctx.db.clone(),
        blob_store,
        config,
        interval,
    ))
}

pub async fn scenarios(ctx: &ApiContext) -> ApiResult<Vec<ScenarioSummary>> {
    let started = Instant::now();
    // Pull the live set from the DB (seeded canonical rows + any
    // user-created ones, non-archived). Fall back to the compiled-in
    // legacy set when the scenarios table is unavailable (test contexts
    // without migration 006).
    let rows: Vec<Scenario> =
        match api_scenario::list(ctx, api_scenario::ListScenariosFilter::default()).await {
            Ok(v) if !v.is_empty() => v,
            _ => {
                #[allow(deprecated)]
                {
                    canonical_scenarios()
                }
            }
        };
    let summaries: Vec<ScenarioSummary> = rows
        .into_iter()
        .map(|s| {
            let asset_universe: Vec<String> = s.asset.iter().map(|a| a.venue_symbol.clone()).collect();
            // Old `regime_tags` shape — extract the "regime:*" prefix off the
            // new combined `tags` field. Will go away with Task 6's seed
            // rewrite.
            let regime_tags: Vec<String> = s
                .tags
                .iter()
                .filter_map(|t| t.strip_prefix("regime:").map(|r| r.to_string()))
                .collect();
            ScenarioSummary {
                id: s.id,
                display_name: s.display_name,
                asset_universe,
                regime_tags,
                time_window_days: (s.time_window.end - s.time_window.start).num_days(),
            }
        })
        .collect();

    let _ = audit::record(
        ctx,
        "eval",
        "scenarios",
        None,
        None,
        Outcome::Ok,
        started.elapsed().as_millis() as i64,
    )
    .await;
    Ok(summaries)
}

/// Convert a `Run` to the slim `RunSummary` wire shape. Public so the
/// dashboard's `launch` handler can build the 201 response directly
/// without re-fetching from the store.
pub fn summarise_run(run: Run) -> RunSummary {
    summarise(run)
}

fn summarise(run: Run) -> RunSummary {
    let (sharpe, max_dd, total_return, inference_cost, net_return) = match &run.metrics {
        Some(m) => (
            Some(m.sharpe),
            Some(m.max_drawdown_pct),
            Some(m.total_return_pct),
            m.inference_cost_quote_total,
            m.net_return_pct,
        ),
        None => (None, None, None, None, None),
    };
    RunSummary {
        id: run.id,
        agent_id: run.agent_id,
        scenario_id: run.scenario_id,
        mode: match run.mode {
            RunMode::Backtest => "backtest".into(),
            RunMode::Paper => "paper".into(),
        },
        status: run.status.as_str().into(),
        started_at: run.started_at,
        completed_at: run.completed_at,
        sharpe,
        max_drawdown_pct: max_dd,
        total_return_pct: total_return,
        error: run.error,
        actual_input_tokens: run.actual_input_tokens,
        actual_output_tokens: run.actual_output_tokens,
        inference_cost_quote_total: inference_cost,
        net_return_pct: net_return,
        filter_summaries: Vec::new(),
    }
}

// --- attestation surface (Phase 3.D Task 11) -----------------------------

/// Sign + persist an `EvalAttestation` for a completed run. Loads the
/// Ed25519 signing key from `$XVN_HOME/identity/signing.key`,
/// auto-generating one on first use. Returns the signed attestation —
/// callers (CLI / future MCP verb) can serialize it for marketplace
/// publishing.
///
/// Errors:
/// - `NotFound` — the run id doesn't exist
/// - `Validation` — the run hasn't computed metrics yet (still queued /
///   running / failed) or its scenario id isn't in `canonical_scenarios()`
/// - `Internal` — key load/generate or signing failure
pub async fn attest(ctx: &ApiContext, run_id: &str) -> ApiResult<EvalAttestation> {
    let started = Instant::now();
    let result = attest_inner(ctx, run_id).await;
    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "eval",
        "attest",
        Some(run_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn attest_inner(ctx: &ApiContext, run_id: &str) -> ApiResult<EvalAttestation> {
    let store = RunStore::new(ctx.db.clone());
    let run = store.get(run_id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("run not found") {
            ApiError::NotFound(format!("eval run '{run_id}'"))
        } else {
            ApiError::Internal(msg)
        }
    })?;
    if run.metrics.is_none() {
        return Err(ApiError::Validation(format!(
            "run '{run_id}' has no metrics — finalize before attesting (status: {})",
            run.status.as_str()
        )));
    }
    let scenario = resolve_scenario(ctx, &run.scenario_id).await.map_err(|_| {
        ApiError::Validation(format!(
            "run '{run_id}' references unknown scenario '{}'; cannot attest",
            run.scenario_id
        ))
    })?;

    let signing_key = load_or_create_signing_key(&ctx.xvn_home)
        .map_err(|e| ApiError::Internal(format!("signing key: {e:#}")))?;
    let attestation = attestation::sign(&run, &scenario, &signing_key)
        .map_err(|e| ApiError::Internal(format!("sign: {e:#}")))?;
    store
        .record_attestation(&run.id, &attestation)
        .await
        .map_err(|e| ApiError::Internal(format!("persist attestation: {e:#}")))?;
    Ok(attestation)
}

/// Load `$xvn_home/identity/signing.key` (raw 32 bytes) or generate one
/// if missing. Returns the parsed `SigningKey`. New keys are written
/// 0o600 on Unix; on creation, the parent directory is created with
/// `create_dir_all`.
fn load_or_create_signing_key(xvn_home: &Path) -> anyhow::Result<SigningKey> {
    let dir = xvn_home.join("identity");
    let path = dir.join("signing.key");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            let arr: [u8; 32] = bytes.as_slice().try_into().expect("len 32 checked");
            return Ok(SigningKey::from_bytes(&arr));
        }
        anyhow::bail!(
            "signing key at {} has length {}; expected 32 raw bytes",
            path.display(),
            bytes.len()
        );
    }

    // Generate fresh.
    std::fs::create_dir_all(&dir).map_err(|e| anyhow::anyhow!("create {}: {e}", dir.display()))?;
    let mut rng = rand_core::OsRng;
    let key = SigningKey::generate(&mut rng);
    let bytes = key.to_bytes();
    std::fs::write(&path, bytes).map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(key)
}

// ── Batch persistence API (migration 020) ─────────────────────────────────────

use crate::eval::batch_store::{Batch, BatchStore};

/// Request shape for `create_batch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBatchRequest {
    pub strategy_id: String,
    /// Agent profile id for `--review-with` (optional).
    pub review_with: Option<String>,
}

/// Request shape for `list_batches`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ListBatchesRequest {
    /// Optional strategy filter (most-recent-first ordering preserved).
    pub strategy_id: Option<String>,
}

/// `Batch` + its associated run ids (joined via `eval_runs.batch_id`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDetail {
    #[serde(flatten)]
    pub batch: Batch,
    pub run_ids: Vec<String>,
}

/// Insert a new `eval_batches` row with `status = 'running'`. Returns the
/// persisted `Batch` so callers have the generated `batch_id` immediately.
pub async fn create_batch(ctx: &ApiContext, req: CreateBatchRequest) -> ApiResult<Batch> {
    let store = BatchStore::new(ctx.db.clone());
    store
        .create(&req.strategy_id, req.review_with.as_deref())
        .await
        .map_err(|e| ApiError::Internal(format!("create_batch: {e}")))
}

/// Load a batch plus its associated run ids (sorted by `started_at`).
pub async fn get_batch(ctx: &ApiContext, batch_id: &str) -> ApiResult<BatchDetail> {
    let store = BatchStore::new(ctx.db.clone());
    let batch = store
        .get(batch_id)
        .await
        .map_err(|e| ApiError::Internal(format!("get_batch: {e}")))?
        .ok_or_else(|| ApiError::NotFound(format!("batch '{batch_id}'")))?;
    let run_ids = store
        .run_ids_for_batch(batch_id)
        .await
        .map_err(|e| ApiError::Internal(format!("run_ids_for_batch: {e}")))?;
    Ok(BatchDetail { batch, run_ids })
}

/// List batches most-recent first; optionally filter by `strategy_id`.
pub async fn list_batches(ctx: &ApiContext, req: ListBatchesRequest) -> ApiResult<Vec<Batch>> {
    let store = BatchStore::new(ctx.db.clone());
    store
        .list(req.strategy_id.as_deref())
        .await
        .map_err(|e| ApiError::Internal(format!("list_batches: {e}")))
}

/// Compute rollup status from the batch's run statuses and set `completed_at`.
/// Idempotent: re-calling on a batch that already has a terminal status is
/// a no-op and returns the stored row unchanged.
pub async fn finalize_batch(ctx: &ApiContext, batch_id: &str) -> ApiResult<Batch> {
    let batch_store = BatchStore::new(ctx.db.clone());
    let run_store = RunStore::new(ctx.db.clone());

    // Load current batch first to check if already terminal.
    let batch = batch_store
        .get(batch_id)
        .await
        .map_err(|e| ApiError::Internal(format!("get batch for finalize: {e}")))?
        .ok_or_else(|| ApiError::NotFound(format!("batch '{batch_id}'")))?;

    if matches!(batch.status.as_str(), "completed" | "partial" | "failed") {
        return Ok(batch);
    }

    // Load run statuses for this batch.
    let run_ids = batch_store
        .run_ids_for_batch(batch_id)
        .await
        .map_err(|e| ApiError::Internal(format!("run_ids_for_batch: {e}")))?;

    let mut statuses: Vec<String> = Vec::with_capacity(run_ids.len());
    for run_id in &run_ids {
        let run = run_store
            .get(run_id)
            .await
            .map_err(|e| ApiError::Internal(format!("get run {run_id}: {e}")))?;
        statuses.push(run.status.as_str().to_string());
    }

    let status_refs: Vec<&str> = statuses.iter().map(String::as_str).collect();
    batch_store
        .finalize(batch_id, &status_refs)
        .await
        .map_err(|e| ApiError::Internal(format!("finalize batch: {e}")))
}

/// Attach a run to an existing batch. Called by `batch run` immediately after
/// each run completes. Idempotent if the run already carries the batch_id.
pub async fn attach_run_to_batch(ctx: &ApiContext, run_id: &str, batch_id: &str) -> ApiResult<()> {
    let store = BatchStore::new(ctx.db.clone());
    store
        .attach_run(run_id, batch_id)
        .await
        .map_err(|e| ApiError::Internal(format!("attach_run_to_batch: {e}")))
}

mod tests {
    use super::*;
    use crate::strategies::{
        manifest::PublicManifest, risk::RiskPreset, slot::LLMSlot, AgentRef, PipelineDef, Strategy,
    };

    #[allow(dead_code)]
    fn provider(enabled_models: Vec<&str>) -> ProviderEntry {
        ProviderEntry {
            name: "openrouter".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://openrouter.ai/api/v1".into(),
            api_key_env: "OPENROUTER_API_KEY".into(),
            enabled_models: enabled_models.into_iter().map(str::to_string).collect(),
        }
    }

    #[allow(dead_code)]
    fn slot(provider: Option<&str>, model: Option<&str>, attested_with: &str) -> LLMSlot {
        LLMSlot {
            role: "trader".into(),
            attested_with: attested_with.into(),
            allowed_tools: Vec::new(),
            provider: provider.map(str::to_string),
            model: model.map(str::to_string),
        }
    }

    #[allow(dead_code)]
    fn strategy_with_legacy_slot(legacy_slot: LLMSlot) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id: "01TESTEVALMODELRESOLUTION".into(),
                display_name: "Test".into(),
                plain_summary: "test".into(),
                creator: "@test".into(),
                template: "custom".into(),
                regime_fit: Vec::new(),
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 60,
                attested_with: Vec::new(),
                required_tools: Vec::new(),
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
            },
            hypothesis: None,
            agents: vec![AgentRef {
                agent_id: "01TESTAGENT".into(),
                role: "trader".into(),
            }],
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: Some(legacy_slot),
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
        }
    }

    #[test]
    fn eval_provider_model_validation_rejects_legacy_requirement_as_model() {
        let entry = provider(vec!["deepseek/deepseek-v4-flash"]);
        let bad_slot = slot(Some("openrouter"), None, "anthropic.claude-sonnet-4.6");

        let err = validate_eval_provider_models(&entry, &[&bad_slot]).unwrap_err();

        assert!(
            err.to_string().contains("anthropic.claude-sonnet-4.6"),
            "expected rejected model in error, got {err}",
        );
        assert!(
            err.to_string().contains("deepseek/deepseek-v4-flash"),
            "expected enabled model hint in error, got {err}",
        );
    }

    #[test]
    fn eval_provider_model_validation_accepts_enabled_agent_model() {
        let entry = provider(vec!["deepseek/deepseek-v4-flash"]);
        let agent_slot = slot(
            Some("openrouter"),
            Some("deepseek/deepseek-v4-flash"),
            "anthropic.claude-sonnet-4.6",
        );

        validate_eval_provider_models(&entry, &[&agent_slot]).unwrap();
    }

    #[test]
    fn eval_runtime_slots_prefer_attached_agents_over_legacy_slots() {
        let legacy_slot = slot(Some("openrouter"), None, "anthropic.claude-sonnet-4.6");
        let strategy = strategy_with_legacy_slot(legacy_slot);
        let agent_slots = vec![ResolvedAgentSlot {
            role: "trader".into(),
            slot: slot(
                Some("openrouter"),
                Some("deepseek/deepseek-v4-flash"),
                "anthropic.claude-sonnet-4.6",
            ),
            system_prompt: String::new(),
            max_tokens: Some(4096),
            temperature: None,
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
        }];

        let slots = runtime_slots(&strategy, &agent_slots);

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].effective_model(), "deepseek/deepseek-v4-flash");
    }

    #[test]
    fn eval_run_request_rejects_unknown_fields() {
        let err = serde_json::from_str::<EvalRunRequest>(
            r#"{"agent_id":"a","scenario_id":"s","mode":"backtest","params_override":null,"extra":true}"#,
        )
        .expect_err("unknown eval-run fields must be rejected");

        assert!(err.to_string().contains("unknown field"));
    }

    // `eval_trader_source_accepts_legacy_trader_slot_without_agents`
    // deleted 2026-05-21 alongside the legacy fallback removal — the
    // eval boundary no longer accepts an empty `Strategy.agents` even
    // when `trader_slot` is populated. See
    // `team/contracts/strategy-require-at-least-one-agent-fixture-migration.md`.

    #[test]
    fn eval_trader_source_rejects_attached_agents_without_trader_role() {
        let legacy_slot = slot(Some("openrouter"), None, "anthropic.claude-sonnet-4.6");
        let strategy = strategy_with_legacy_slot(legacy_slot);
        let agent_slots = vec![ResolvedAgentSlot {
            role: "seeker".into(),
            slot: slot(
                Some("openrouter"),
                Some("deepseek/deepseek-v4-flash"),
                "anthropic.claude-sonnet-4.6",
            ),
            system_prompt: String::new(),
            max_tokens: Some(4096),
            temperature: None,
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
        }];

        let err = validate_eval_trader_source(&strategy, &agent_slots).unwrap_err();
        let msg = err.to_string();

        assert!(
            msg.contains("role `trader`"),
            "expected trader-role guardrail, got {msg}"
        );
        assert!(
            msg.contains("seeker"),
            "expected attached role in error, got {msg}"
        );
    }

    #[test]
    fn eval_trader_source_accepts_attached_trader_role() {
        let legacy_slot = slot(Some("openrouter"), None, "anthropic.claude-sonnet-4.6");
        let strategy = strategy_with_legacy_slot(legacy_slot);
        let agent_slots = vec![ResolvedAgentSlot {
            role: "trader".into(),
            slot: slot(
                Some("openrouter"),
                Some("deepseek/deepseek-v4-flash"),
                "anthropic.claude-sonnet-4.6",
            ),
            system_prompt: String::new(),
            max_tokens: Some(4096),
            temperature: None,
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
        }];

        validate_eval_trader_source(&strategy, &agent_slots).unwrap();
    }

    #[test]
    fn eval_trader_source_rejects_empty_agents() {
        // QA22 / `strategy-require-at-least-one-agent`: when the
        // strategy has no attached agents the eval boundary names the
        // missing-agent condition explicitly so operators know which
        // fix to make. Post-2026-05-21 the legacy `trader_slot`
        // fallback is gone — an empty `agents` is fatal regardless of
        // whether `trader_slot` is set.
        let legacy_slot = slot(Some("openrouter"), None, "anthropic.claude-sonnet-4.6");
        let mut strategy = strategy_with_legacy_slot(legacy_slot);
        strategy.agents.clear();

        let err = validate_eval_trader_source(&strategy, &[]).unwrap_err();
        let msg = err.to_string();

        assert!(
            msg.contains("no agent attached"),
            "expected missing-agent message, got {msg}"
        );
        assert!(
            msg.contains("Attach an agent"),
            "expected attach-agent remediation, got {msg}"
        );
    }
}

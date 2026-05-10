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

use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::llm::{AnthropicDispatch, LlmDispatch};
use crate::api::audit::{self, Outcome};
use crate::api::{strategy as api_strategy, ApiContext, ApiError, ApiResult};
use crate::eval::executor::{Executor, PaperExecutor};
use crate::eval::run::{Run, RunMode, RunStatus};
use crate::eval::scenario::{canonical_scenarios, Scenario};
use crate::eval::store::{ListFilter, RunStore};
use crate::tools::ToolRegistry;
use xvision_execution::broker_surface::{AlpacaPaperSurface, BrokerSurface};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ListRunsRequest {
    pub strategy_bundle_hash: Option<String>,
    pub scenario_id: Option<String>,
    pub status: Option<RunStatus>,
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
    pub strategy_bundle_hash: String,
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
        strategy_bundle_hash: req.strategy_bundle_hash.clone(),
        scenario_id: req.scenario_id.clone(),
        status: req.status,
    };
    store
        .list(filter)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

/// Same as `list` but returns the slim `RunSummary` shape.
pub async fn list_summaries(
    ctx: &ApiContext,
    req: ListRunsRequest,
) -> ApiResult<Vec<RunSummary>> {
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

async fn list_summaries_inner(
    ctx: &ApiContext,
    req: &ListRunsRequest,
) -> ApiResult<Vec<RunSummary>> {
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

    Ok(RunDetail {
        summary: summarise(run),
        decisions,
        equity_curve,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRunRequest {
    /// Strategy bundle id (the `agent_id` returned by `api::strategy::list`).
    pub agent_id: String,
    /// Scenario id from `canonical_scenarios()` (e.g. `crypto-bull-q1-2025`).
    pub scenario_id: String,
    /// Run mode. `Backtest` is rejected with `ApiError::Validation` until
    /// `BacktestExecutor` lands (Phase 3.B-backtest, separate PR).
    pub mode: RunMode,
    /// Optional per-run override of bundle.mechanical_params. Persisted as
    /// `eval_runs.params_override_json`.
    pub params_override: Option<serde_json::Value>,
}

/// Public env-bound entry point: constructs broker / dispatch / tools
/// from environment variables and dispatches to `run_with_deps`.
///
/// Required env for paper mode:
///   APCA_API_KEY_ID, APCA_API_SECRET_KEY, [APCA_API_BASE_URL]
///   ANTHROPIC_API_KEY
///
/// Validation that doesn't depend on env (backtest mode, missing strategy,
/// missing scenario) runs FIRST so the operator sees a clean "backtest not
/// supported" / "strategy not found" error rather than buried-behind an
/// `APCA_API_KEY_ID not found` from the broker constructor.
pub async fn run(ctx: &ApiContext, req: EvalRunRequest) -> ApiResult<Run> {
    if req.mode == RunMode::Backtest {
        return Err(ApiError::Validation(
            "backtest mode not yet supported — Phase 3.B-backtest ships BacktestExecutor as a follow-up; use --mode paper".into(),
        ));
    }
    // Early NotFound surfaces without env-var noise.
    let _bundle = api_strategy::get(ctx, &req.agent_id).await?;
    if !canonical_scenarios().iter().any(|s| s.id == req.scenario_id) {
        return Err(ApiError::NotFound(format!(
            "scenario '{}'",
            req.scenario_id
        )));
    }

    let broker_arc: Arc<dyn BrokerSurface> = Arc::new(
        AlpacaPaperSurface::from_env()
            .map_err(|e| ApiError::Internal(format!("alpaca paper from_env: {e}")))?,
    );
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        ApiError::Validation("ANTHROPIC_API_KEY env var is required for paper mode".into())
    })?;
    let dispatch_arc: Arc<dyn LlmDispatch> = Arc::new(AnthropicDispatch::new(api_key));
    let tools_arc = Arc::new(ToolRegistry::default_with_builtins());
    run_with_deps(ctx, req, broker_arc, dispatch_arc, tools_arc).await
}

/// Testable / deps-injecting variant of `run`. Tests pass a
/// `MockBrokerSurface` + `MockDispatch` so no network is required;
/// production callers go through `run` which constructs deps from env.
pub async fn run_with_deps(
    ctx: &ApiContext,
    req: EvalRunRequest,
    broker: Arc<dyn BrokerSurface>,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
) -> ApiResult<Run> {
    let started = Instant::now();
    let target_clone = format!("{}@{}", req.agent_id, req.scenario_id);
    let args_json = serde_json::to_string(&req).ok();

    let result = run_inner(ctx, req, broker, dispatch, tools).await;

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
    broker: Arc<dyn BrokerSurface>,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
) -> ApiResult<Run> {
    if req.mode == RunMode::Backtest {
        return Err(ApiError::Validation(
            "backtest mode not yet supported — Phase 3.B-backtest ships BacktestExecutor as a follow-up; use --mode paper".into(),
        ));
    }

    // 1. Look up the strategy bundle. Propagates ApiError::NotFound cleanly.
    let bundle = api_strategy::get(ctx, &req.agent_id).await?;

    // 2. Look up the scenario from the canonical set.
    let scenario: Scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == req.scenario_id)
        .ok_or_else(|| ApiError::NotFound(format!("scenario '{}'", req.scenario_id)))?;

    // 3. Build a fresh Run, persist, then drive the executor.
    let mut run = Run::new_queued(
        req.agent_id.clone(),
        scenario.id.clone(),
        req.mode,
    );
    run.params_override = req.params_override.clone();

    let store = RunStore::new(ctx.db.clone());
    store
        .create(&run)
        .await
        .map_err(|e| ApiError::Internal(format!("create run: {e}")))?;

    let executor = PaperExecutor::new(broker);
    if let Err(e) = executor
        .run(&mut run, &bundle, &scenario, dispatch, tools, &store)
        .await
    {
        // Persist the failure so downstream callers (CLI, dashboard) can
        // see why this run is not Completed.
        let err_msg = e.to_string();
        let _ = store
            .update_status(&run.id, RunStatus::Failed, Some(&err_msg))
            .await;
        return Err(ApiError::Internal(format!("executor: {err_msg}")));
    }

    // Re-read from the store so the returned Run reflects the canonical
    // post-finalize state — completed_at + metrics_json are set inside
    // RunStore::finalize and we want callers to see them.
    store
        .get(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("re-read finalized run: {e}")))
}

pub async fn scenarios(ctx: &ApiContext) -> ApiResult<Vec<ScenarioSummary>> {
    let started = Instant::now();
    let summaries: Vec<ScenarioSummary> = canonical_scenarios()
        .into_iter()
        .map(|s| ScenarioSummary {
            id: s.id,
            display_name: s.display_name,
            asset_universe: s.asset_universe,
            regime_tags: s.regime_tags,
            time_window_days: (s.time_window.end - s.time_window.start).num_days(),
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

fn summarise(run: Run) -> RunSummary {
    let (sharpe, max_dd, total_return) = match &run.metrics {
        Some(m) => (
            Some(m.sharpe),
            Some(m.max_drawdown_pct),
            Some(m.total_return_pct),
        ),
        None => (None, None, None),
    };
    RunSummary {
        id: run.id,
        strategy_bundle_hash: run.strategy_bundle_hash,
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
    }
}

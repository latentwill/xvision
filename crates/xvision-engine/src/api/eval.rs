//! Eval-domain api dispatch.
//!
//! Phase 3.D shipped the read-only core (`list`, `get`, `scenarios`) returning
//! the engine's full `Run` shape. This module also exposes a slimmer
//! `RunSummary` wire shape via `list_summaries` for clients (today: the
//! dashboard, tomorrow: MCP browse tools) that don't want the full Run, and a
//! `get_run` fn that returns a `RunDetail` (summary + decisions + equity
//! curve) for the dashboard's `/eval-runs/:id` page.
//!
//! The `run` dispatch (which constructs PaperExecutor + LlmDispatch +
//! ToolRegistry from env) is still deferred to a follow-up PR.

use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::audit::{self, Outcome};
use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::run::{Run, RunMode, RunStatus};
use crate::eval::scenario::canonical_scenarios;
use crate::eval::store::{ListFilter, RunStore};

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

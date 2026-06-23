//! Eval-domain api dispatch.
//!
//! Public surface:
//! - `list` / `get` / `scenarios` — read-only browse (PR #23)
//! - `list_summaries` — slim wire shape for the dashboard's `/api/eval/runs`
//!   list and (future) MCP browse tools (PR #21)
//! - `get_run` — `RunDetail` (summary + decisions + equity curve) for the
//!   dashboard's `/eval-runs/:id` page (PR #24)
//! - `run` — Backtest-mode dispatch that constructs `Executor` +
//!   `AnthropicDispatch` + `ToolRegistry::default_with_builtins` from env
//!   vars (PR #26; Live mode lands with the `live-bar-source-alpaca` track)
//! - `run_with_deps` — testable variant that takes the broker / dispatch /
//!   tools as parameters; useful for tests and any caller that wants to
//!   inject a custom dispatch (e.g., a `MockDispatch` for fixture-only tests)
//! - `compare` — wraps `eval::compare_runs` with audit + typed-error mapping
//!   for the dashboard's run-comparison view + `xvn eval compare` CLI
//! - `attest` — sign + persist an `EvalAttestation` for a completed run,
//!   sourcing the Ed25519 signing key from `$XVN_HOME/identity/signing.key`
//!   (auto-generated on first use). Wraps `eval::attestation::sign` +
//!   `RunStore::record_attestation`. Powers `xvn eval attest <run_id>` and
//!   the (future) `publish_attestation` MCP verb.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use crate::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch, OpenaiCompatDispatch};
use crate::agent::pipeline::ResolvedAgentSlot;
use crate::agents::AgentStore;
use crate::api::audit::{self, Outcome};
use crate::api::scenario as api_scenario;
use crate::api::settings::brokers as broker_settings;
use crate::api::{search as api_search, strategy as api_strategy, ApiContext, ApiError, ApiResult};
use crate::eval::attestation::{self, EvalAttestation};
use crate::eval::compare::{compare_runs, CompareOptions, ComparisonReport, ManifestMismatch};
use crate::eval::cost::aggregate_eval_run_inference_cost;
use crate::eval::executor::{Executor, GatedBrokerSurface, RunExecutor};
use crate::eval::findings::{Finding, InferenceCostDominatesReturnPayload, Severity};
use crate::eval::live_config::LiveConfig;
use crate::eval::metrics::{
    compute_net_return_pct, inference_cost_dominates, INFERENCE_COST_DOMINANCE_THRESHOLD,
};
use crate::eval::run::{DeploymentSource, ReviewModel, Run, RunMode, RunStatus};
#[allow(deprecated)]
use crate::eval::scenario::canonical_scenarios;
use crate::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel, LatencyModel,
    LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario, ScenarioSource,
    SlippageModel, TimeWindow, Venue, VenueSettings,
};
use crate::eval::store::{ListFilter, RunStore};
use crate::safety::{AuthContext, VenueLabel};
use crate::tools::ToolRegistry;
use xvision_agent_client::{AgentClient, ToolDispatch, ToolDispatchError};
use xvision_core::config::{self, AgentRuntime, ProviderEntry, ProviderKind};
use xvision_core::market::Ohlcv;
use xvision_data::alpaca_live::{AlpacaLiveClient, AlpacaLiveCredentials};
use xvision_data::alpaca_live_poll::{production_fetcher, AlpacaLivePoll};
use xvision_execution::broker_surface::{AlpacaPaperSurface, BrokerSurface, OrderlyLiveSurface};
use xvision_execution::{ByrealLiveSurface, DegenArenaSurface, HyperliquidSurface};
use xvision_filters::{FilterEventV1, FilterSummary};

// ---------------------------------------------------------------------------
// U13: agentd process registry for eval cancel
// ---------------------------------------------------------------------------
//
// `eval cancel` marks the run cancelled in the DB but, before this, did nothing
// about the `xvision-agentd` sidecar that an in-flight Cline run spawned. The
// sidecar kept running (holding Ollama GPU memory / CPU), so the NEXT eval run
// started against a zombie and appeared hung.
//
// We track each Cline run's agentd handle at spawn time in a process-global
// registry keyed by `run_id`, and `cancel` signals it. The handle captures the
// OS pid (when the sidecar supervisor exposes it) and the socket path. Cancel
// DEGRADES GRACEFULLY: if the run isn't registered (older run, llm-dispatch
// path, or a sidecar whose pid we couldn't capture), cancel still succeeds and
// returns a [`CancelOutcome`] telling the caller whether the process was
// actually signaled, so the CLI can warn the operator to restart the container.

/// A registered agentd sidecar belonging to an in-flight eval run.
#[derive(Debug, Clone)]
pub struct AgentdHandle {
    /// OS process id of the spawned `xvision-agentd` sidecar, when the
    /// supervisor exposed it at spawn time. `None` when unknown — cancel then
    /// degrades to "not signaled" rather than killing an unrelated pid.
    pub pid: Option<u32>,
    /// The sidecar's main UDS socket path, for diagnostics / a future
    /// socket-based shutdown handshake.
    pub socket_path: std::path::PathBuf,
}

type AgentdRegistry = std::sync::Mutex<std::collections::HashMap<String, AgentdHandle>>;

fn agentd_registry() -> &'static AgentdRegistry {
    static REG: std::sync::OnceLock<AgentdRegistry> = std::sync::OnceLock::new();
    REG.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Register an agentd sidecar handle for a run. Called at spawn time. Replaces
/// any prior handle for the same `run_id` (a run only has one live sidecar).
pub fn register_agentd(run_id: &str, handle: AgentdHandle) {
    if let Ok(mut reg) = agentd_registry().lock() {
        reg.insert(run_id.to_string(), handle);
    }
}

/// Remove a run's agentd handle (called on normal completion so the registry
/// doesn't grow unbounded). Best-effort.
pub fn deregister_agentd(run_id: &str) {
    if let Ok(mut reg) = agentd_registry().lock() {
        reg.remove(run_id);
    }
}

/// Outcome of attempting to terminate a run's agentd sidecar during cancel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// The sidecar process was signaled (SIGTERM sent to a known pid).
    Signaled,
    /// No sidecar was registered for this run (llm-dispatch path, older run,
    /// or already deregistered). Nothing to signal — cancel still succeeds.
    NoProcess,
    /// A handle was registered but carried no usable pid, so we could not
    /// signal it. The CLI should warn the operator that the agent process may
    /// still be running.
    Unknown,
}

/// Attempt to SIGTERM the agentd sidecar registered for `run_id`. Returns a
/// [`CancelOutcome`] describing what happened; NEVER errors, so a cancel is
/// never blocked by sidecar bookkeeping. The handle is removed from the
/// registry regardless of outcome (a cancelled run won't reuse it).
pub fn signal_agentd_for_run(run_id: &str) -> CancelOutcome {
    let handle = match agentd_registry().lock() {
        Ok(mut reg) => reg.remove(run_id),
        Err(_) => None,
    };
    let Some(handle) = handle else {
        return CancelOutcome::NoProcess;
    };
    match handle.pid {
        Some(pid) => {
            send_sigterm(pid);
            CancelOutcome::Signaled
        }
        None => CancelOutcome::Unknown,
    }
}

/// Send SIGTERM to a pid on Unix via the `kill(1)` utility (no extra crate
/// dependency). No-op on non-Unix (the sidecar is Unix-targeted in v1).
/// Best-effort: a dead/reaped pid just makes `kill` exit non-zero, which we
/// ignore. Spawns and detaches so cancel is never blocked on the subprocess.
#[cfg(unix)]
fn send_sigterm(pid: u32) {
    let _ = std::process::Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(not(unix))]
fn send_sigterm(_pid: u32) {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRunsRequest {
    pub agent_id: Option<String>,
    pub scenario_id: Option<String>,
    /// One or more statuses to filter on. `None` = no filter; a
    /// single-element Vec behaves identically to the previous
    /// single-`Option<RunStatus>` API. Serialises as a JSON array so
    /// MCP / wizard callers that JSON-encode `ListRunsRequest` still
    /// work after the change.
    pub status: Option<Vec<RunStatus>>,
    /// Optional pagination — when both fields are absent, every matching
    /// row is returned. The dashboard's list endpoint passes both;
    /// internal callers (retry idempotency, chart preview) pass neither
    /// because they need the full match set.
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
    /// bead-008: optional INCLUSIVE lower bound on `started_at` (RFC-3339,
    /// already validated/parsed by the dashboard route). `None` applies no
    /// time filter.
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
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

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStrategyMetadata {
    pub id: String,
    pub display_name: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunScenarioMetadata {
    pub id: String,
    pub display_name: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<RunStrategyMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<RunScenarioMetadata>,
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
    #[serde(default)]
    pub auto_fire_review: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_model: Option<ReviewModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_annotations_per_review: Option<u32>,
    /// A1 per-run pause flag. `true` ⇒ the live executor is skipping broker
    /// submits for this run's cycles (additive to the global safety pause)
    /// while it keeps iterating. Defaults to `false` for pre-061 runs.
    #[serde(default)]
    pub paused: bool,
    /// RFC3339 timestamp of the most recent pause (`eval_runs.paused_at`,
    /// migration 061); `null` when never paused or after resume. Mirrors how
    /// `safety_state.paused_at` is surfaced on the global safety status. Track
    /// B (cockpit) reads this to show "paused since …".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<String>,
    /// A3 one-shot "flatten positions" request flag (`eval_runs.flatten_requested`,
    /// migration 062). `true` ⇒ the live executor will close ALL open broker
    /// positions on its next cycle and then clear the flag, WITHOUT terminating
    /// the run. The cockpit (spec §2.7) reads this to show a pending-flatten
    /// state. Defaults to `false` for pre-062 runs.
    #[serde(default)]
    pub flatten_requested: bool,
    /// Live launch envelope (`mode = live` runs only): venue label, stop
    /// policy, capital, display name. `None` for backtests. Surfaced so the
    /// live inspector can render deployment config without a second fetch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub live_config: Option<LiveConfig>,
    /// CT5 deployment-source discriminator (`eval_runs.source`, migration 065):
    /// `Human` for operator-queued runs, `Optimizer` for autooptimizer runs.
    /// Drives `awm`'s Cancel-gate. Defaults to `Human` for pre-065 runs.
    #[serde(default)]
    pub source: DeploymentSource,
    /// CT5 per-run mark-to-market unrealized PnL in USD
    /// (`eval_runs.unrealized_pnl_usd`, migration 065). `None` when unavailable
    /// / pre-first-fill — surfaced as "—" in the UI, NEVER a faked 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl_usd: Option<f64>,
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
    /// Distinct signal tool names (Nansen + Elfa) actually called during this
    /// run, sorted alphabetically. `None` when no signal tools were called or
    /// when the trace data is unavailable (old runs, missing agent_runs rows).
    /// Drives the "signals used" chip row in the cycle-detail UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signals_used: Option<Vec<String>>,
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
        status: req.status.clone(),
        mode: None,
        limit: req.limit,
        offset: req.offset,
        since: req.since,
        ..Default::default()
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
        status: req.status.clone(),
        mode: None,
        limit: req.limit,
        offset: req.offset,
        since: req.since,
        ..Default::default()
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
    cancel_with_outcome(ctx, run_id).await.map(|(run, _)| run)
}

/// Like [`cancel`], but also returns the [`CancelOutcome`] for the run's agentd
/// sidecar so callers (e.g. the CLI) can tell the operator whether the agent
/// process was actually signaled or may still be running. Never errors on the
/// signal itself — the outcome is advisory.
pub async fn cancel_with_outcome(ctx: &ApiContext, run_id: &str) -> ApiResult<(Run, CancelOutcome)> {
    let started = Instant::now();
    let store = RunStore::new(ctx.db.clone());
    // U13: terminate the run's agentd sidecar (if any) so it stops competing for
    // the Ollama backend. Degrades gracefully — never blocks the cancel.
    let agentd_outcome = signal_agentd_for_run(run_id);
    match agentd_outcome {
        CancelOutcome::Signaled => {
            tracing::info!(run_id, "sent SIGTERM to agentd sidecar on cancel");
        }
        CancelOutcome::Unknown => {
            tracing::warn!(
                run_id,
                "run cancelled but agentd pid unknown; the agent process may still be running"
            );
        }
        CancelOutcome::NoProcess => {}
    }
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
    result.map(|run| (run, agentd_outcome))
}

/// A1 per-run pause: set the run's `paused` flag to `true`.
///
/// Additive to the global `SafetyManager` pause — a paused run keeps
/// iterating but submits no broker orders for the affected cycles. It does
/// NOT terminate the run. Idempotent (re-pausing is a no-op). Returns the
/// refreshed `Run`. Errors `NotFound` for an unknown run id.
pub async fn pause(ctx: &ApiContext, run_id: &str) -> ApiResult<Run> {
    set_paused_inner(ctx, run_id, true, "pause").await
}

/// A1 per-run pause: clear the run's `paused` flag (resume trading).
///
/// Counterpart to [`pause`]. Idempotent. Returns the refreshed `Run`.
/// Errors `NotFound` for an unknown run id.
pub async fn resume(ctx: &ApiContext, run_id: &str) -> ApiResult<Run> {
    set_paused_inner(ctx, run_id, false, "resume").await
}

/// A3 one-shot flatten: request that the live executor close ALL open broker
/// positions on its next cycle, WITHOUT terminating the run (spec §2.7's
/// cockpit [Flatten positions] action). Sets `eval_runs.flatten_requested`;
/// the executor flattens then clears the flag (one-shot) and keeps the run
/// running (it typically stays paused). Additive to A1 pause / A2 cancel and
/// shares their audit + error surface. Idempotent (re-requesting before the
/// executor consumes the flag is a no-op). Returns the refreshed `Run`. Errors
/// `NotFound` for an unknown run id.
pub async fn flatten(ctx: &ApiContext, run_id: &str) -> ApiResult<Run> {
    let started = Instant::now();
    let store = RunStore::new(ctx.db.clone());
    let result = async {
        // `request_flatten` bails with "no run with id" when the id is unknown;
        // map that to NotFound (mirroring `set_paused_inner`) so we surface the
        // right status without a redundant pre-write existence round-trip.
        store.request_flatten(run_id).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("no run with id") {
                ApiError::NotFound(format!("run '{run_id}'"))
            } else {
                ApiError::Internal(format!("flatten run: {e}"))
            }
        })?;
        // Re-read so the returned Run reflects the request.
        get_inner(ctx, run_id).await
    }
    .await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "eval",
        "flatten",
        Some(run_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

/// Shared body for [`pause`]/[`resume`]: flips `eval_runs.paused`, audits the
/// op (mirroring `cancel`), and returns the refreshed run.
async fn set_paused_inner(ctx: &ApiContext, run_id: &str, paused: bool, action: &str) -> ApiResult<Run> {
    let started = Instant::now();
    let store = RunStore::new(ctx.db.clone());
    let result = async {
        // `set_paused` bails with "no run with id" when the id is unknown;
        // map that to NotFound (mirroring `get_inner`) so we surface the
        // right status without a redundant pre-write existence round-trip.
        store.set_paused(run_id, paused).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("no run with id") {
                ApiError::NotFound(format!("run '{run_id}'"))
            } else {
                ApiError::Internal(format!("{action} run: {e}"))
            }
        })?;
        // Re-read so the returned Run reflects the new flag.
        get_inner(ctx, run_id).await
    }
    .await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "eval",
        action,
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
        live_config: source.live_config.clone(),
        limits: None,
        skip_preflight: false,
        provider_override: None,
        assets_subset: None,
        auto_fire_review: source.auto_fire_review,
        review_model: source.review_model.clone(),
        max_annotations_per_review: source.max_annotations_per_review,
        // Retries default to no recording (Live); a re-record is requested
        // explicitly via a fresh launch.
        trajectory_mode: RunTrajectoryMode::default(),
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
    let mut report = compare_runs(&req.run_ids, &store, &options).await.map_err(|e| {
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
    })?;
    enrich_compare_strategy_names(ctx, &mut report).await;
    Ok(report)
}

async fn enrich_compare_strategy_names(ctx: &ApiContext, report: &mut ComparisonReport) {
    for run in &mut report.runs {
        if run.strategy_name.is_some() {
            continue;
        }
        if let Ok(strategy) = api_strategy::get(ctx, &run.agent_id).await {
            let name = strategy.manifest.display_name.trim();
            if !name.is_empty() {
                run.strategy_name = Some(name.to_string());
            }
        }
    }
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
    enrich_run_summary_metadata(ctx, &mut summary).await;
    summary.filter_summaries = filter_summaries.clone();

    // Best-effort: load signal tools from the observability trace. Returns
    // None if the run predates the agent_runs trace path or called no signals.
    let signals_used = load_signals_used(&ctx.db, id).await;

    Ok(RunDetail {
        summary,
        decisions,
        equity_curve,
        filter_events,
        filter_summaries,
        signals_used,
    })
}

/// Derive the distinct set of signal tool names (Nansen + Elfa) from a slice
/// of raw tool names (as recorded in `tool_calls.tool_name`). Returns `None`
/// when no signal tools are present so the field is omitted from the JSON.
/// The result is sorted alphabetically so the output is deterministic.
pub fn signals_used_from_tool_names(tool_names: &[String]) -> Option<Vec<String>> {
    let mut seen: std::collections::HashSet<String> = tool_names
        .iter()
        .filter(|n| crate::tools::signal_policy::signal_tool_policy(n.as_str()).is_some())
        .cloned()
        .collect();
    if seen.is_empty() {
        return None;
    }
    let mut result: Vec<String> = seen.drain().collect();
    result.sort();
    Some(result)
}

/// Load the distinct signal tool names called for an eval run from the
/// `tool_calls` table (via the `agent_runs → spans` join path).
/// Returns `None` on any error or when no signal tools were called.
/// Best-effort: a missing/empty trace is not an error.
async fn load_signals_used(pool: &sqlx::SqlitePool, eval_run_id: &str) -> Option<Vec<String>> {
    let rows: Result<Vec<(String,)>, _> = sqlx::query_as(
        "SELECT DISTINCT tc.tool_name \
         FROM tool_calls tc \
         JOIN spans s ON s.id = tc.span_id \
         JOIN agent_runs ar ON ar.id = s.run_id \
         WHERE ar.eval_run_id = ?",
    )
    .bind(eval_run_id)
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) => {
            let names: Vec<String> = rows.into_iter().map(|(n,)| n).collect();
            signals_used_from_tool_names(&names)
        }
        Err(_) => None,
    }
}

async fn enrich_run_summary_metadata(ctx: &ApiContext, summary: &mut RunSummary) {
    match api_strategy::get(ctx, &summary.agent_id).await {
        Ok(strategy) => {
            summary.strategy = Some(RunStrategyMetadata {
                id: strategy.manifest.id,
                display_name: strategy.manifest.display_name,
            });
        }
        Err(err) => {
            tracing::debug!(
                run_id = %summary.id,
                strategy_id = %summary.agent_id,
                error = %err,
                "eval run summary strategy metadata unavailable"
            );
        }
    }
    match api_scenario::get(ctx, &summary.scenario_id).await {
        Ok(scenario) => {
            summary.scenario = Some(RunScenarioMetadata {
                id: scenario.id,
                display_name: scenario.display_name,
            });
        }
        Err(err) => {
            tracing::debug!(
                run_id = %summary.id,
                scenario_id = %summary.scenario_id,
                error = %err,
                "eval run summary scenario metadata unavailable"
            );
        }
    }
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderOverride {
    /// Provider name as it appears in `[[providers]]` (e.g. `anthropic`,
    /// `openrouter`). Must be paired with a concrete `model` — passing
    /// one without the other is a CLI/API usage error.
    pub provider: String,
    /// Model id to use for the duration of this run. Must be enabled on
    /// the named provider (verified through
    /// `effective_providers::resolve_provider`).
    pub model: String,
}

/// §2-D — per-run trajectory-recording mode. The operator's chosen driver
/// for Cline trajectory recording (replaces the §2-B `XVN_TRAJECTORY_RECORD`
/// env gate). The caller declares intent on the request; the engine acts on it.
///
/// * `Live` (default) — no recording. Byte-identical to the pre-§2-D /
///   pre-§2-B behaviour for backtest + live + non-record Cline runs:
///   `trajectory_mode != Record` ⇒ `recording_request = None` ⇒ the §2-B
///   `None` spawn path (no event sink bound).
/// * `Record` — mint a trajectory recording for the run's primary recorded
///   slot and bind the event sink so frames persist into the store.
///
/// Replay through the engine eval path is intentionally NOT a variant here.
/// Adding engine-eval replay would require threading a recording id + store
/// into every slot dispatch — out of scope for §2-D.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunTrajectoryMode {
    /// No recording (default — preserves byte-identity for non-record runs).
    #[default]
    Live,
    /// Mint a recording for this run and persist trajectory frames.
    Record,
}

impl RunTrajectoryMode {
    /// True when this run should mint a trajectory recording.
    pub fn records(self) -> bool {
        matches!(self, RunTrajectoryMode::Record)
    }
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
    /// Run mode. `Backtest` replays the scenario's parquet fixture in-process
    /// without any broker. `Live` is routed to `Executor::live(...)`, which
    /// currently returns a not-implemented error pending the
    /// `live-bar-source-alpaca` track + the Phase 3 launch endpoint.
    pub mode: RunMode,
    /// Optional free-form per-run config bag, persisted verbatim as
    /// `eval_runs.params_override_json`. Used as part of the run's dedup
    /// fingerprint and read by the watchdog (e.g. `max_run_duration_secs`).
    #[cfg_attr(feature = "ts-export", ts(type = "Record<string, unknown> | null"))]
    pub params_override: Option<serde_json::Value>,
    /// Required for `mode = live`. Backtest runs must leave this unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_config: Option<LiveConfig>,
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
    /// Optional per-launch override of the strategy's bound `(provider,
    /// model)`. When set, the override is resolved through
    /// `effective_providers::resolve_provider`; if the override is
    /// unreachable (key_missing, provider_disabled, model_disabled,
    /// provider_unknown) the launch refuses with the typed `reason`.
    ///
    /// The override does NOT mutate the strategy on disk — it is a
    /// per-run swap. Hard limits in `EvalLimits` still apply. CLI:
    /// `--provider <X> --model <Y>` (both required together).
    ///
    /// Wave B #5: `cli-eval-model-override`.
    #[serde(default)]
    pub provider_override: Option<ProviderOverride>,
    /// Optional per-run subset of the strategy's `asset_universe`. When set,
    /// only the listed assets are traded in this run (backtest only — paper
    /// mode ignores this field today). Every entry must be present in the
    /// strategy's `asset_universe`; validation is performed inside
    /// `build_backtest_executor` via `active_assets`.
    ///
    /// CLI: `--assets ETH,SOL` (comma-separated). `None` (default) trades
    /// the full universe as declared in the strategy manifest.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "Array<string> | null"))]
    pub assets_subset: Option<Vec<xvision_core::trading::AssetSymbol>>,
    /// When true, finalizing a successful run fires the rule-based review
    /// agent and stores chart annotations on `eval_reviews.annotations_json`.
    /// Default false: auto-review is opt-in per run.
    #[serde(default)]
    pub auto_fire_review: bool,
    /// Optional display override for the review model requested by the
    /// launcher. The current wave persists this for audit/UI; manual review
    /// engine routing continues to use the selected agent profile.
    #[serde(default)]
    pub review_model: Option<ReviewModel>,
    /// Maximum annotations the review contract should emit. Stored on the
    /// run so UI/CLI launches round-trip their annotation budget.
    #[serde(default)]
    pub max_annotations_per_review: Option<u32>,
    /// §2-D — per-run Cline trajectory-recording mode. The operator-chosen
    /// driver for recording (replaces the §2-B `XVN_TRAJECTORY_RECORD` env
    /// gate). `Live` (default) records nothing and is byte-identical to the
    /// pre-§2-D behaviour; `Record` mints a trajectory recording for the
    /// run's primary recorded slot. Only consulted when the run's
    /// `agent_runtime` resolves to `Cline`; backtest/live LlmDispatch runs
    /// ignore it. CLI: `--record-trajectory`.
    #[serde(default)]
    pub trajectory_mode: RunTrajectoryMode,
}

/// Stable role string used on the `supervisor_notes` row that captures
/// the per-launch provider/model override. Read at export time so
/// `EvalRunExport.provider_diagnostics.override` round-trips.
pub const PROVIDER_OVERRIDE_NOTE_ROLE: &str = "provider_override";

/// Public env-bound entry point: constructs dispatch / tools from
/// environment variables and dispatches to `run_with_deps`.
///
/// Required env:
/// - backtest mode: `ANTHROPIC_API_KEY` only (no broker constructed)
/// - live mode: currently returns a stable not-implemented validation error
///
/// Validation that doesn't depend on env (missing strategy, missing
/// scenario) runs FIRST so the operator sees a clean "strategy not found"
/// error rather than buried-behind an `APCA_API_KEY_ID not found` from the
/// broker constructor.
pub async fn run(ctx: &ApiContext, req: EvalRunRequest) -> ApiResult<Run> {
    // Early NotFound surfaces without env-var noise. Resolve the scenario
    // via the DB-backed registry (with a legacy `canonical_scenarios()`
    // fallback for test contexts that haven't applied migration 006).
    validate_provider_override_shape(req.provider_override.as_ref())?;
    let strategy = api_strategy::get(ctx, &req.agent_id).await?;
    validate_live_request_shape(&req)?;
    if req.mode == RunMode::Backtest {
        let _scenario = resolve_scenario(ctx, &req.scenario_id).await?;
    }

    // Live mode is reserved for the launch endpoint (Phase 3, see
    // `live-bar-source-alpaca` + `live-eval-launch-and-freeze`). The
    // engine surface today only ships the Backtest path; Live constructs
    // through `Executor::live(...)` and returns a stable not-implemented
    // error. Broker construction is deferred entirely — Live no longer
    // shares the eval-paper paper-broker code path.
    let broker: Option<Arc<dyn BrokerSurface>> = None;
    let mut agent_slots = resolve_agent_slots(ctx, &strategy).await?;
    validate_eval_trader_source(&strategy, &agent_slots)?;
    apply_provider_override(&mut agent_slots, req.provider_override.as_ref());

    let provider_names = validate_provider_preflight(ctx, &req, &strategy, &agent_slots).await?;
    let skip_preflight = req.skip_preflight;
    let (dispatch_arc, findings_model) =
        build_eval_dispatch(ctx, &strategy, &agent_slots, req.provider_override.as_ref()).await?;
    let sig_cfg = resolve_signal_tool_config(ctx);
    let tools_arc = Arc::new(build_tool_registry(ctx, &sig_cfg));
    let run = run_with_deps(ctx, req, broker, dispatch_arc, findings_model, tools_arc).await?;
    let store = RunStore::new(ctx.db.clone());
    write_preflight_supervisor_notes(&store, &run.id, &provider_names, skip_preflight).await;
    // The override receipt is written inside `run_inner` (called via
    // `run_with_deps`), so it lands once per launched run regardless of
    // entry point — no duplicate write needed here.
    Ok(run)
}

/// Reject a malformed `ProviderOverride` (either field empty after trim)
/// with `ApiError::Validation`. CLIs validate both flags are supplied
/// together up front, but the engine boundary keeps its own gate so the
/// dashboard/MCP API cannot bypass it.
fn validate_provider_override_shape(override_value: Option<&ProviderOverride>) -> ApiResult<()> {
    let Some(o) = override_value else { return Ok(()) };
    let p = o.provider.trim();
    let m = o.model.trim();
    if p.is_empty() && m.is_empty() {
        return Err(ApiError::Validation(
            "per-launch override requires both `provider` and `model`; both fields are empty".into(),
        ));
    }
    if p.is_empty() {
        return Err(ApiError::Validation(
            "per-launch override has empty `provider`; both `provider` and `model` are required together"
                .into(),
        ));
    }
    if m.is_empty() {
        return Err(ApiError::Validation(
            "per-launch override has empty `model`; both `provider` and `model` are required together".into(),
        ));
    }
    Ok(())
}

fn validate_live_request_shape(req: &EvalRunRequest) -> ApiResult<()> {
    match (&req.mode, req.live_config.as_ref()) {
        (RunMode::Live, Some(cfg)) => cfg
            .validate()
            // Surface `Display` (human-readable, actionable), not the `{e:?}`
            // Debug variant — operators (CLI + dashboard) see this string.
            .map_err(|e| ApiError::Validation(format!("invalid live_config at {}: {e}", e.field_path()))),
        (RunMode::Live, None) => Err(ApiError::Validation(
            "mode=live requires live_config (strategy_id, assets, capital, broker_creds_ref, stop_policy)"
                .into(),
        )),
        (RunMode::Backtest, Some(_)) => Err(ApiError::Validation(
            "mode=backtest must not include live_config".into(),
        )),
        (RunMode::Backtest, None) if req.scenario_id.trim().is_empty() => {
            Err(ApiError::Validation("mode=backtest requires scenario_id".into()))
        }
        (RunMode::Backtest, None) => Ok(()),
    }
}

fn scenario_from_live_config(cfg: &LiveConfig) -> Scenario {
    let now = Utc::now();
    Scenario {
        id: String::new(),
        parent_scenario_id: None,
        source: ScenarioSource::Generated,
        display_name: cfg.display_name.clone(),
        description: cfg.description.clone().unwrap_or_default(),
        tags: cfg.tags.clone(),
        notes: cfg.notes.clone(),
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow { start: now, end: now },

        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: Some("crypto".into()),
            adjustment: AdjustmentMode::Raw,
        },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees {
                maker_bps: 0,
                taker_bps: 0,
            },
            slippage: SlippageModel::None,
            latency: LatencyModel {
                decision_to_fill_ms: 0,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: Vec::new(),
            borrow_bps_per_day: 5.0,
        },
        replay_mode: ReplayMode::Realtime,
        capital: cfg.capital.clone(),
        bar_cache_policy: BarCachePolicy {
            cache_key: "live-alpaca".into(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: cfg.warmup_bars.unwrap_or(200),
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: now,
        created_by: "live".into(),
        archived_at: None,
        venue_label: cfg.venue_label,
        safety_limits: cfg.safety_limits.clone(),
    }
}

/// Rewrite each `ResolvedAgentSlot.slot.provider` / `.model` (and the
/// `attested_with` echo) in place when a per-launch override is set.
/// No-op when the override is `None`. Empty/whitespace overrides are
/// treated as no-op too — validation must reject those upstream (the
/// CLI requires both flags together and rejects blank values).
fn apply_provider_override(agent_slots: &mut [ResolvedAgentSlot], override_value: Option<&ProviderOverride>) {
    let Some(o) = override_value else { return };
    let p = o.provider.trim();
    let m = o.model.trim();
    if p.is_empty() || m.is_empty() {
        return;
    }
    for resolved in agent_slots.iter_mut() {
        resolved.slot.provider = Some(p.to_string());
        resolved.slot.model = Some(m.to_string());
        resolved.slot.attested_with = format!("{p}.{m}");
    }
}

/// Read the per-launch override receipt that was persisted via
/// `record_provider_override_note`. Scans `supervisor_notes` for the
/// `provider_override` role and parses the JSON content. Returns `None`
/// when no override was applied for this run (the common case) or when
/// the note row is malformed (best-effort surface — we don't fail the
/// export over a malformed historical row).
pub async fn load_provider_override(ctx: &ApiContext, run_id: &str) -> Option<ProviderOverride> {
    let store = RunStore::new(ctx.db.clone());
    let notes = store.read_supervisor_notes(run_id).await.ok()?;
    for (role, _severity, content) in notes {
        if role == PROVIDER_OVERRIDE_NOTE_ROLE {
            if let Ok(parsed) = serde_json::from_str::<ProviderOverride>(&content) {
                return Some(parsed);
            }
        }
    }
    None
}

/// Persist the per-launch override receipt as a `supervisor_notes` row so
/// it round-trips into the eval export and `xvn eval results --json`.
/// Best-effort; failures log but don't abort the run (the override
/// already took effect at dispatch time).
async fn record_provider_override_note(
    store: &RunStore,
    run_id: &str,
    override_value: Option<&ProviderOverride>,
) {
    let Some(o) = override_value else { return };
    let payload = serde_json::json!({
        "provider": o.provider,
        "model": o.model,
    });
    let content = payload.to_string();
    if let Err(e) = store
        .record_supervisor_note(run_id, PROVIDER_OVERRIDE_NOTE_ROLE, "info", &content)
        .await
    {
        tracing::warn!(
            run_id,
            err = %e,
            "failed to record provider_override supervisor note (run continues; override already applied at dispatch)",
        );
    }
}

/// Stable role string for the `supervisor_notes` row that records which agent
/// runtime a run resolved to and why (Cline sidecar vs legacy LlmDispatch).
/// Round-trips into the eval export / `xvn eval results --json` alongside the
/// `provider_override` receipt so a silent runtime fallback is auditable per
/// run, not just in process logs.
pub const AGENT_RUNTIME_NOTE_ROLE: &str = "agent_runtime";

/// Persist the resolved agent runtime (+ reason) as a `supervisor_notes` row.
/// Mirrors `record_provider_override_note`: best-effort — a failed note write
/// logs but never aborts the run (the runtime already took effect at spawn).
async fn record_agent_runtime_note(store: &RunStore, run_id: &str, runtime: AgentRuntime, reason: &str) {
    let payload = serde_json::json!({
        "runtime": match runtime {
            AgentRuntime::Cline => "cline",
        },
        "reason": reason,
    });
    let severity = match runtime {
        AgentRuntime::Cline => "info",
    };
    if let Err(e) = store
        .record_supervisor_note(run_id, AGENT_RUNTIME_NOTE_ROLE, severity, &payload.to_string())
        .await
    {
        tracing::warn!(
            run_id,
            err = %e,
            "failed to record agent_runtime supervisor note (run continues; runtime already resolved)",
        );
    }
}

// `build_alpaca_paper_broker` was removed alongside the paper-mode-executor-deleted
// deletion (executor-collapse-paper-mode, 2026-05-22). The live launch
// endpoint (`live-bar-source-alpaca`) owns broker construction for Live
// runs going forward; Backtest runs never built one.

/// Build the LLM dispatch the eval will use plus the findings-extractor
/// model id appropriate for that provider. The second tuple element
/// exists because the postprocess path reuses this same dispatch, and
/// the right Haiku id varies by provider (Anthropic-native vs OpenRouter
/// slug); see [`crate::eval::postprocess::findings_model_for_provider`].
async fn build_eval_dispatch(
    ctx: &ApiContext,
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
    provider_override: Option<&ProviderOverride>,
) -> ApiResult<(Arc<dyn LlmDispatch>, String)> {
    // Per-launch override (Wave B #5, `cli-eval-model-override`). When set,
    // the override replaces the strategy-bound `(provider, model)` for
    // *this run only*. Resolved through the canonical
    // `resolve_provider` helper so the typed `reason` discriminant on
    // refusal matches the strategy-bound path: an unreachable override
    // surfaces as the same `key_missing` / `provider_disabled` /
    // `model_disabled` / `provider_unknown` ApiError::Validation.
    if let Some(o) = provider_override {
        let cfg_path = runtime_config_path(ctx);
        let entry = match crate::api::settings::providers::resolve_provider(
            ctx,
            &cfg_path,
            &o.provider,
            Some(&o.model),
        )
        .await
        {
            Ok(e) => e,
            Err(unavailable) => {
                let model_clause = unavailable
                    .model
                    .as_ref()
                    .map(|m| format!(" model `{m}`,"))
                    .unwrap_or_default();
                return Err(ApiError::Validation(format!(
                    "per-launch override provider `{}`{} is not launchable (reason={}): {}",
                    unavailable.provider,
                    model_clause,
                    unavailable.reason.as_str(),
                    unavailable.hint,
                )));
            }
        };
        let findings_model = crate::eval::postprocess::findings_model_for_provider(&entry);
        let dispatch = dispatch_from_provider(&ctx.xvn_home, &entry).await?;
        return Ok((dispatch, findings_model));
    }

    let provider_name = select_eval_provider(ctx, strategy, agent_slots).await?;
    // Route through the canonical helper so the CLI, dashboard, and
    // eval-launch all agree on what "configured + launchable" means.
    let runtime_slots = runtime_slots(strategy, agent_slots);
    // Find the model that will be used for this provider — needed so
    // `resolve_provider` can verify the model is enabled. Strategies are
    // single-provider today (validated by `validate_eval_provider_models`
    // below) so the first non-empty model on a matching slot wins.
    let requested_model: Option<String> = runtime_slots
        .iter()
        .filter(|slot| {
            slot.provider
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| p == provider_name)
                .unwrap_or(false)
        })
        .filter_map(|slot| {
            slot.model
                .as_deref()
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .map(str::to_string)
        })
        .next();
    let cfg_path = runtime_config_path(ctx);
    let entry = match crate::api::settings::providers::resolve_provider(
        ctx,
        &cfg_path,
        &provider_name,
        requested_model.as_deref(),
    )
    .await
    {
        Ok(e) => e,
        Err(unavailable) => {
            let model_clause = unavailable
                .model
                .as_ref()
                .map(|m| format!(" model `{m}`,"))
                .unwrap_or_default();
            return Err(ApiError::Validation(format!(
                "provider `{}`{} is not launchable (reason={}): {}",
                unavailable.provider,
                model_clause,
                unavailable.reason.as_str(),
                unavailable.hint,
            )));
        }
    };
    validate_eval_provider_models(&entry, &runtime_slots)?;
    let findings_model = crate::eval::postprocess::findings_model_for_provider(&entry);
    let dispatch = dispatch_from_provider(&ctx.xvn_home, &entry).await?;
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

    // 1. Legacy inline slots on the strategy (trader / regime).
    for slot in [strategy.trader_slot.as_ref(), strategy.regime_slot.as_ref()]
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

/// Phase 4 launch gate + preflight guardrails (live eval path).
///
/// Runs BEFORE the executor is built/spawned in [`start_run`]. It refuses
/// the launch with a typed `ApiError::Validation` when a strategy is not
/// launchable for a capability-completeness reason (Phase 4.1
/// `diagnostics::assert_launchable`) OR when one of the cleanly-reachable
/// Phase 4.2 short-circuit detectors fires at launch-preflight time:
///
///   * `strategy_references_unattached_slot` — an `AgentRef` whose agent
///     resolves but has no slot fulfilling the role.
///   * `missing_prompt` — a required-capability slot with an empty/
///     whitespace-only system prompt.
///   * `missing_tool` — a tool the required capability needs is granted
///     nowhere (built-ins ∪ manifest `required_tools` ∪ slot grants).
///   * `provider_unavailable` — the provider bound to a slot is not in the
///     resolved provider set for this launch.
///
/// Because this runs before the `eval_runs` row (and thus the obs emitter)
/// exists, a fired guardrail surfaces its typed error synchronously as the
/// refused-launch `ApiError::Validation` body — the failure is recorded as
/// the launch refusal rather than a silent success. The message embeds the
/// guardrail `code()` + `remediation()` so the CLI/UI can branch on it and
/// the obs run is never spawned. A backtest of a strategy missing a
/// REQUIRED capability does NOT start.
async fn assert_launchable_with_guardrails(
    ctx: &ApiContext,
    strategy_id: &str,
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
) -> ApiResult<()> {
    // Resolve the set of providers configured in the runtime config — the
    // enabled-provider set the `provider_unavailable` guardrail checks slot
    // bindings against. A slot bound to a provider absent from this set is a
    // hard short-circuit. A config-load failure leaves the set empty, so any
    // bound provider is reported unavailable (fail-closed). local-candle and
    // every other configured kind are included by name.
    let cfg_path = runtime_config_path(ctx);
    let available_providers: Vec<String> =
        match tokio::task::spawn_blocking(move || config::load_runtime(&cfg_path)).await {
            Ok(Ok(cfg)) => cfg.providers.iter().map(|p| p.name.clone()).collect(),
            _ => Vec::new(),
        };
    let available_providers = available_providers.as_slice();
    // ── Tool-readiness launch gate ──────────────────────────────────────
    let diag = crate::diagnostics::capability_diagnostics(ctx, strategy_id).await?;
    if let Err(e) = crate::diagnostics::assert_launchable(&diag) {
        tracing::warn!(
            strategy_id,
            error = %e,
            "eval launch blocked by tool diagnostics (not launchable)",
        );
        return Err(ApiError::Validation(format!(
            "strategy `{strategy_id}` is not launchable: {e}",
        )));
    }

    // ── Phase 4.2: launch-preflight short-circuits ─────────────────────
    // Assemble the set of tools available to the run: built-ins ∪ the
    // strategy manifest's `required_tools` ∪ any per-slot grants. This is
    // the same union `check_missing_tool` expects.
    let mut available_tools: Vec<String> = crate::tools::built_in_tool_descriptors()
        .into_iter()
        .map(|t| t.name)
        .collect();
    for t in &strategy.manifest.required_tools {
        if !available_tools.contains(t) {
            available_tools.push(t.clone());
        }
    }

    // strategy_references_unattached_slot — defense-in-depth.
    //
    // NOTE: as the engine resolves slots today (`resolve_agent_slots`),
    // an `AgentRef` whose agent is missing or has zero slots already fails
    // EARLIER with `ApiError::NotFound` / "agent has no executable slots",
    // and a resolved ref always maps the agent's first slot to the ref's
    // role (so `resolved.role == agent_ref.role` by construction). The
    // primary trigger for this guardrail is therefore pre-empted upstream.
    // We keep the cheap check as a regression guard: if the resolution
    // semantics ever change so a ref can survive resolution without a
    // matching slot, this fires the typed short-circuit rather than
    // launching a position that cannot execute.
    for agent_ref in &strategy.agents {
        let slot_attached = agent_slots.iter().any(|resolved| {
            resolved.agent_id == agent_ref.agent_id
                && resolved.role.trim().eq_ignore_ascii_case(agent_ref.role.trim())
        });
        if let Err(sc) =
            crate::guardrails::check_slot_attached(&agent_ref.role, &agent_ref.agent_id, slot_attached)
        {
            return Err(short_circuit_validation(strategy_id, &sc));
        }
    }

    // Per-slot prompt / provider / tool preflight.
    for resolved in agent_slots {
        let role = resolved.role.as_str();

        // missing_prompt: a launchable position must have a prompt to send.
        if let Err(sc) = crate::guardrails::check_prompt_present(role, &resolved.system_prompt) {
            return Err(short_circuit_validation(strategy_id, &sc));
        }

        // provider_unavailable: the slot's bound provider must be resolvable.
        if let Some(provider) = resolved.slot.provider.as_deref() {
            let provider = provider.trim();
            if !provider.is_empty() {
                if let Err(sc) =
                    crate::guardrails::check_provider_available(role, provider, available_providers)
                {
                    return Err(short_circuit_validation(strategy_id, &sc));
                }
            }
        }

        let effective_tools = if resolved.slot.allowed_tools.is_empty() {
            strategy.manifest.required_tools.as_slice()
        } else {
            resolved.slot.allowed_tools.as_slice()
        };
        for tool in effective_tools {
            if let Err(sc) = crate::guardrails::check_missing_tool(role, tool, &available_tools) {
                return Err(short_circuit_validation(strategy_id, &sc));
            }
        }
    }

    Ok(())
}

/// Map a launch-preflight [`ShortCircuit`] into the refused-launch
/// `ApiError::Validation` body. The message carries the stable `code()` and
/// the operator-facing `remediation()` so the failure is recorded with its
/// machine identifier rather than a free-text warning.
fn short_circuit_validation(strategy_id: &str, sc: &crate::guardrails::ShortCircuit) -> ApiError {
    tracing::warn!(
        strategy_id,
        short_circuit = sc.code(),
        "eval launch blocked by guardrail short-circuit: {sc}",
    );
    ApiError::Validation(format!(
        "[{code}] {sc} — {remediation}",
        code = sc.code(),
        remediation = sc.remediation(),
    ))
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
    [strategy.trader_slot.as_ref(), strategy.regime_slot.as_ref()]
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
        // F10: build the resolved slot through the single shared
        // `resolve_agent_slot` helper (also used by the pool-based
        // `resolve_agent_slots_for_strategy` the optimizer paper-test path
        // calls) so the executor-ready slot fields never drift between the
        // two resolvers. This loop keeps the HTTP-typed errors above.
        let mut resolved = crate::agent::pipeline::resolve_agent_slot(&agent_ref.role, slot, &agent.agent_id);
        // Honor per-AgentRef prompt/model overrides on the EVAL path too — the
        // optimizer gates candidates by backtest through here, so the override
        // must take effect or prompt/model mutations would be runtime no-ops
        // (identical ΔSharpe to the parent → always rejected). Centralized in
        // `apply_agent_ref_overrides` so both resolvers behave identically.
        crate::agent::pipeline::apply_agent_ref_overrides(&mut resolved, agent_ref);
        // Nano DB lookup — only when AgentRef carries a CheckpointRef.
        if let Some(checkpoint_ref) = agent_ref.checkpoint.as_ref() {
            let nano_store = crate::nanochat::store::NanochatStore::new(ctx.db.clone());
            let model = nano_store
                .get_model(&checkpoint_ref.model_id)
                .await
                .map_err(|e| {
                    ApiError::Internal(format!(
                        "nanochat store error for {}: {e}",
                        checkpoint_ref.model_id
                    ))
                })?
                .ok_or_else(|| {
                    ApiError::NotFound(format!(
                        "nanochat checkpoint {} not found in trained_models",
                        checkpoint_ref.model_id
                    ))
                })?;
            let input_spec: crate::agent::nano_dispatch::NanoInputSpec =
                serde_json::from_str(&model.input_spec).map_err(|e| {
                    ApiError::Validation(format!(
                        "bad input_spec JSON for {}: {e}",
                        checkpoint_ref.model_id
                    ))
                })?;
            resolved.nano = Some(crate::agent::pipeline::NanoSlotConfig {
                checkpoint: checkpoint_ref.clone(),
                veto: agent_ref.veto,
                input_spec,
                weights_sha256: model.weights_sha256,
            });
        }
        out.push(resolved);
    }
    Ok(out)
}

async fn dispatch_from_provider(xvn_home: &Path, entry: &ProviderEntry) -> ApiResult<Arc<dyn LlmDispatch>> {
    // Resolve the key with the SAME env-first-then-secrets-file priority that
    // `provider check` uses, so a fresh `docker exec xvn-app xvn ...` (no key
    // bridged into env) still finds the key persisted in
    // `$XVN_HOME/secrets/providers.toml`. Env wins when both are present.
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        crate::api::settings::providers::resolve_provider_key_value(xvn_home, entry)
            .await?
            .ok_or_else(|| {
                ApiError::Validation(format!(
                    "no API key for provider `{}` (env var {} is unset and no key stored in secrets/providers.toml). Paste a key in Settings → Providers or export {} before running eval.",
                    entry.name, entry.api_key_env, entry.api_key_env
                ))
            })?
    };
    let no_auth_eval = matches!(
        entry.kind,
        ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm
    );
    if api_key.is_empty() && !no_auth_eval {
        return Err(ApiError::Validation(format!(
            "provider `{}` has no API key set. Paste one in Settings → Providers.",
            entry.name
        )));
    }
    match entry.kind {
        ProviderKind::Anthropic => Ok(Arc::new(AnthropicDispatch::new(api_key))),
        ProviderKind::OpenaiCompat | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm => {
            Ok(Arc::new(OpenaiCompatDispatch::new(
                entry.base_url.clone(),
                api_key,
            )))
        }
        ProviderKind::LocalCandle => Ok(Arc::new(MockDispatch::echo(
            r#"{"action":"hold","conviction":0.0,"justification":"local-candle deterministic hold"}"#,
        ))),
    }
}

/// Resolve the API key for a provider entry (mirrors the key-resolution
/// half of [`dispatch_from_provider`]). Returns `Ok(None)` for keyless
/// local endpoints, `Ok(Some(key))` otherwise, and a typed validation
/// error when the configured env var is unset.
async fn resolve_provider_api_key(xvn_home: &Path, entry: &ProviderEntry) -> ApiResult<Option<String>> {
    if entry.api_key_env.is_empty() {
        return Ok(None);
    }
    // Env-first-then-secrets-file fallback — mirrors `provider check`. Only
    // error when BOTH env var and the secrets file lack the key.
    let key = crate::api::settings::providers::resolve_provider_key_value(xvn_home, entry)
        .await?
        .ok_or_else(|| {
            ApiError::Validation(format!(
                "no API key for provider `{}` (env var {} is unset and no key stored in secrets/providers.toml). Paste a key in Settings → Providers or export {} before running eval.",
                entry.name, entry.api_key_env, entry.api_key_env
            ))
        })?;
    if key.is_empty()
        && !matches!(
            entry.kind,
            ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm
        )
    {
        return Err(ApiError::Validation(format!(
            "provider `{}` has no API key set. Paste one in Settings → Providers.",
            entry.name
        )));
    }
    Ok(Some(key))
}

/// Read the effective agent runtime for an eval run.
///
/// The flag default is `Cline` (Task 9 flip). But the Cline path can only
/// physically run when the `xvision-agentd` sidecar binary is available, so
/// Resolve the agent runtime (WU-6: always `Cline`).
///
/// Since WU-6 retired the `LlmDispatch` trader path, every run unconditionally
/// uses the Cline sidecar. This function is kept for call-site compatibility
/// and for logging the resolved runtime as a supervisor note. If the sidecar
/// is not provisioned (`XVN_AGENTD_BIN` unset), `spawn_cline_ctx` will fail
/// with a clear actionable error — never a silent downgrade.
async fn resolve_agent_runtime(ctx: &ApiContext) -> (AgentRuntime, &'static str) {
    let _ = ctx; // ctx retained for future per-workspace overrides
    tracing::info!(target: "agent_runtime", "agent_runtime=cline (unconditional since WU-6)");
    (
        AgentRuntime::Cline,
        "cline (unconditional — LlmDispatch retired in WU-6)",
    )
}

/// Tool-response cache handle. Enables deterministic backtest re-runs by
/// serving cached external tool responses (Nansen/Elfa) from the trajectory
/// store instead of hitting the live network.
///
/// `replay = true`  → serve from cache, return Err on miss (loud — never
///                    silently re-fetches on a miss; missing cache = broken
///                    recording).
/// `replay = false` → record path: forward to live tool, then write to cache
///                    (best-effort; a cache write failure does NOT fail the run).
///
/// Production replay wiring (engine-eval replay mode) is a follow-up that
/// lands alongside `RunTrajectoryMode::Replay`; at present only `record`
/// (`replay: false`) is built into the `spawn_cline_ctx` path.
#[derive(Clone)]
struct ToolHttpCacheHandle {
    store: std::sync::Arc<xvision_observability::trajectory::store::TrajectoryStore>,
    recording_id: xvision_observability::trajectory::key::RecordingId,
    /// `true` => replay (serve from cache, no HTTP); `false` => record (fetch then cache).
    replay: bool,
}

/// Bridges sidecar tool callbacks to the engine's [`ToolRegistry`]. The
/// Cline agent invokes registry-backed tools (indicators, ohlcv, …) over
/// the callback socket; this adapter routes them to the same
/// `tool_call::invoke` path the `LlmDispatch` executor uses, so both
/// runtimes share one tool surface. `submit_decision` is NOT routed here —
/// it is a built-in lifecycle tool captured locally by the sidecar.
struct ToolRegistryDispatch {
    tools: Arc<ToolRegistry>,
    current_asset: Arc<tokio::sync::RwLock<Option<String>>>,
    /// The run's mode. Drives the forward-only guard + the Nansen backtest
    /// `as_of_date` injection in `invoke` (later tasks). `Copy`.
    run_mode: crate::eval::run::RunMode,
    /// Simulated-clock anchor for the current decision, written by the
    /// executor per cycle (Task 1.4). `None` until the first decision.
    as_of: Arc<tokio::sync::RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
    /// Backtest lookahead lag (days). Default `DEFAULT_NANSEN_LOOKAHEAD_LAG_DAYS`.
    nansen_lag_days: i64,
    /// Tool-response cache for deterministic backtest re-runs. `None` => no
    /// recording/replay (live/forward runs always fetch).
    tool_cache: Option<ToolHttpCacheHandle>,
    /// Per-run Nansen credit budget (D8/im2r.4). `None` => uncapped.
    nansen_budget: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
    /// Per-run Elfa credit budget (im2r.4). `None` => uncapped.
    elfa_budget: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
}

/// True when `v` is a signal-tool degrade response (`{available: false, …}`).
/// Used to avoid caching transient degrade responses (xvision-im2r.3).
fn is_degrade(v: &serde_json::Value) -> bool {
    v.get("available") == Some(&serde_json::Value::Bool(false))
}

/// Compute a stable SHA-256 hash over the canonical JSON of `input`, keyed by
/// `name`. The canonical form sorts object keys so that `{"b":1,"a":2}` and
/// `{"a":2,"b":1}` produce the same hash (deterministic across model runs).
fn canonical_input_hash(name: &str, input: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    let canon = crate::autooptimizer::canonicalize_json(input);
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    h.update([0u8]); // NUL separator — prevents name from bleeding into payload
    h.update(serde_json::to_vec(&canon).unwrap_or_default());
    format!("{:x}", h.finalize())
}

fn normalize_callback_asset_for_compare(asset: &str) -> String {
    let upper = asset.trim().to_ascii_uppercase();
    let base = upper.split('/').next().unwrap_or(&upper);
    base.strip_suffix("USD").unwrap_or(base).to_string()
}

fn callback_market_data_tool_asset_mismatch(
    name: &str,
    input: &serde_json::Value,
    current_asset: Option<&str>,
) -> Option<String> {
    if !matches!(name, "ohlcv" | "indicator_panel") {
        return None;
    }
    let current_asset = current_asset?;
    let requested_asset = input.get("asset").and_then(|v| v.as_str())?;
    if normalize_callback_asset_for_compare(current_asset)
        == normalize_callback_asset_for_compare(requested_asset)
    {
        return None;
    }

    Some(format!(
        "asset mismatch for {name}: current decision asset is {current_asset} but tool requested \
         {requested_asset}. Use the current decision asset only; do not fetch cross-asset market \
         data for this per-asset decision."
    ))
}

#[async_trait::async_trait]
impl ToolDispatch for ToolRegistryDispatch {
    async fn invoke(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, ToolDispatchError> {
        let current_asset = self.current_asset.read().await.clone();
        if let Some(message) =
            callback_market_data_tool_asset_mismatch(name, &input, current_asset.as_deref())
        {
            return Err(ToolDispatchError::Failed(message));
        }

        // Compute the policy once — used for the forward-only gate below and
        // for the cache gate later in this function (xvision-im2r.9).
        let policy = crate::tools::signal_policy::signal_tool_policy(name);

        // Forward-only gate (defense in depth — the advertisement filter (Task 1.6)
        // already strips mode-forbidden tools, but a hand-crafted call must also fail).
        if let Some(p) = policy {
            let allowed = match self.run_mode {
                crate::eval::run::RunMode::Live => p.live,
                crate::eval::run::RunMode::Backtest => p.backtest,
            };
            if !allowed {
                return Err(ToolDispatchError::Failed(format!(
                    "{name} is forward-only; unavailable in backtest"
                )));
            }
        }
        // Anchor Nansen backtest calls to the simulated clock (overwrites any
        // model-supplied as_of_date — the lookahead-safety invariant).
        let input = self.inject_backtest_as_of_async(name, input).await?;

        // D8 / im2r.4: per-provider per-run credit budget for signal tools.
        // Replay is free. Credits are refunded if the downstream fetch Errors
        // (im2r.5) so a failed/unknown-tool call doesn't permanently burn one.
        let budget_arc: Option<std::sync::Arc<std::sync::atomic::AtomicU32>> =
            match crate::tools::signal_policy::tool_provider(name) {
                Some(crate::tools::signal_policy::SignalProvider::Nansen) => self.nansen_budget.clone(),
                Some(crate::tools::signal_policy::SignalProvider::Elfa) => self.elfa_budget.clone(),
                None => None,
            };
        let mut credit_consumed = false;
        if let Some(budget) = &budget_arc {
            let is_replay = self.tool_cache.as_ref().map(|c| c.replay).unwrap_or(false);
            if !is_replay {
                // Atomic check-and-decrement: consume one credit; if there was
                // none, degrade. saturating_sub keeps it >= 0.
                let prev = budget
                    .fetch_update(
                        std::sync::atomic::Ordering::Relaxed,
                        std::sync::atomic::Ordering::Relaxed,
                        |v| Some(v.saturating_sub(1)),
                    )
                    .unwrap_or(0);
                if prev == 0 {
                    return Ok(crate::tools::signal_policy::signal_unavailable(
                        "budget exhausted",
                    ));
                }
                credit_consumed = true;
            }
        }

        // Deterministic replay / record of external (signal) tool responses.
        if let Some(cache) = &self.tool_cache {
            if policy.is_some() {
                let hash = canonical_input_hash(name, &input);
                let as_of_date = input
                    .get("as_of_date")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if cache.replay {
                    return match cache
                        .store
                        .get_cached_tool_response(&cache.recording_id, name, &hash)
                        .await
                    {
                        Ok(Some(v)) => Ok(v),
                        Ok(None) => Err(ToolDispatchError::Failed(format!(
                            "replay: no cached response for {name} (hash {hash}) — \
                             recording incomplete"
                        ))),
                        Err(e) => Err(ToolDispatchError::Failed(format!("replay store error: {e}"))),
                    };
                }
                // Record path: fetch live, then cache before returning.
                // Best-effort cache write — a write failure must NOT fail the run.
                // Do NOT cache degrade responses: they are transient (budget
                // exhausted, provider unavailable) and should not be replayed as
                // authoritative results in future backtest re-runs (xvision-im2r.3).
                let result = match self.dispatch_inner(name, input.clone()).await {
                    Ok(v) => v,
                    Err(e) => {
                        // im2r.5: refund the credit on a failed fetch so a network
                        // error or unknown-tool call doesn't permanently burn one.
                        if credit_consumed {
                            if let Some(budget) = &budget_arc {
                                budget.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        return Err(e);
                    }
                };
                if !is_degrade(&result) {
                    let _ = cache
                        .store
                        .cache_tool_response(&cache.recording_id, name, &hash, as_of_date.as_deref(), &result)
                        .await;
                }
                return Ok(result);
            }
        }

        // Uncached path (live run or non-signal tool).
        match self.dispatch_inner(name, input).await {
            Ok(v) => Ok(v),
            Err(e) => {
                // im2r.5: refund on error (no wasted credit for transient failures).
                if credit_consumed {
                    if let Some(budget) = &budget_arc {
                        budget.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                Err(e)
            }
        }
    }
}

impl ToolRegistryDispatch {
    /// Forward the call to the real tool registry. Shared by the cache record
    /// path and the uncached live path.
    async fn dispatch_inner(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolDispatchError> {
        match crate::agent::tool_call::invoke(name, input, self.tools.clone()).await {
            Ok(s) => {
                // Tool outputs are JSON-shaped strings; pass parsed JSON
                // through when possible, else wrap the raw string.
                Ok(serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s)))
            }
            Err(e) => Err(ToolDispatchError::Failed(format!("{e:#}"))),
        }
    }

    /// For a Nansen tool under `RunMode::Backtest`, overwrite `as_of_date` with
    /// the framework-computed anchor (the model cannot influence the backtest
    /// anchor — lookahead-safety invariant).
    ///
    /// For a Nansen tool under `RunMode::Live`, strip any model-supplied
    /// `as_of_date` — a hallucinated date would silently route the call to the
    /// historical endpoint (xvision-im2r.2).
    ///
    /// Non-Nansen tools pass through unchanged.
    /// Errors if a Nansen backtest call has no clock anchor set.
    async fn inject_backtest_as_of_async(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolDispatchError> {
        use crate::eval::run::RunMode;
        use crate::tools::signal_policy::{is_nansen_tool, nansen_as_of_date};
        if !is_nansen_tool(name) {
            return Ok(input);
        }
        // Nansen live: strip any model-supplied as_of_date (xvision-im2r.2).
        if self.run_mode != RunMode::Backtest {
            let mut input = input;
            if let Some(obj) = input.as_object_mut() {
                obj.remove("as_of_date");
            }
            return Ok(input);
        }
        // Copy the Copy DateTime out of the read guard before consuming it.
        let anchor: Option<chrono::DateTime<chrono::Utc>> = { *self.as_of.read().await };
        let anchor = anchor.ok_or_else(|| {
            ToolDispatchError::Failed(format!(
                "{name}: no simulated-clock anchor set for backtest (executor did not publish as_of)"
            ))
        })?;
        let date = nansen_as_of_date(anchor, self.nansen_lag_days);
        let mut input = input;
        match input.as_object_mut() {
            Some(obj) => {
                obj.insert("as_of_date".into(), serde_json::Value::String(date.to_string()));
            }
            None => {
                return Err(ToolDispatchError::Failed(format!(
                    "{name}: backtest input must be a JSON object to anchor as_of_date"
                )));
            }
        }
        Ok(input)
    }
}

/// Stage 1 (Cline runtime unification, Task 6 eval wiring). When the
/// runtime flag selects `Cline`, spawn the `xvision-agentd` sidecar and
/// build the [`crate::agent::dispatch_capability::ClineDispatchCtx`] the
/// executor threads into every slot dispatch.
///
/// The sidecar binary is resolved from `XVN_AGENTD_BIN`; a real Cline run
/// with the env var unset is a hard, clearly-messaged error (NO silent
/// fallback to LlmDispatch — the operator asked for Cline). The client is
/// spawned with the observability event sink so live runs surface in the
/// agent-runs UI; when no obs bus is configured a fresh bus is used so the
/// spawn still succeeds.
async fn spawn_cline_ctx(
    ctx: &ApiContext,
    entry: ProviderEntry,
    tools: Arc<ToolRegistry>,
    recording_request: Option<RecordingRequest>,
    run_mode: crate::eval::run::RunMode,
) -> ApiResult<(
    crate::agent::dispatch_capability::ClineDispatchCtx,
    Option<crate::agent::cline_recording::RunRecording>,
)> {
    use crate::agent::cline_recording as rec;

    let bin = std::env::var("XVN_AGENTD_BIN").map_err(|_| {
        ApiError::Validation(
            "agent_runtime = cline but XVN_AGENTD_BIN is unset. Set it to the built \
             xvision-agentd entrypoint (e.g. xvision-agentd/dist/index.js) or switch \
             agent_runtime back to llm-dispatch."
                .to_string(),
        )
    })?;
    let api_key = resolve_provider_api_key(&ctx.xvn_home, &entry).await?;

    // §2-B/§2-D: when recording is requested (per-run `trajectory_mode =
    // record`) AND we have a primary slot role to key it by, mint the
    // recording BEFORE spawning the client
    // so the event sink is bound to the store + recording id. The record
    // path and the replay path build the same TrajectoryKey
    // (`cline_recording::build_key`), so a recorded run replays from the
    // persisted store with no test seeding.
    // U13: capture the run id (when a recording is requested) before the
    // request is consumed, so we can register the agentd sidecar against it for
    // `eval cancel`.
    let spawned_run_id: Option<String> = recording_request.as_ref().map(|r| r.run_id.clone());
    let recording = if let Some(req) = recording_request {
        let blob_root = ctx.xvn_home.join("agent_runs").join("blobs");
        let store = rec::open_store(ctx.db.clone(), blob_root)
            .await
            .map_err(|e| ApiError::Internal(format!("open trajectory store: {e}")))?;
        let store = Arc::new(store);
        let key = rec::build_key(&req.run_id, &req.slot_role, &entry.name, &req.model);
        let recording_id = rec::begin(&store, &key)
            .await
            .map_err(|e| ApiError::Internal(format!("begin trajectory recording: {e}")))?;
        tracing::info!(
            target: "xvision_engine::cline_recording",
            recording_id = %recording_id,
            slot_role = %req.slot_role,
            run_id = %req.run_id,
            "trajectory recording minted (record mode)"
        );
        Some((store, recording_id, req.slot_role))
    } else {
        None
    };

    // Per-run socket paths under the xvn home so concurrent runs don't
    // collide. The sidecar process is reaped on client drop.
    let sock_dir = ctx.xvn_home.join("agent_runs").join("sockets");
    std::fs::create_dir_all(&sock_dir)
        .map_err(|e| ApiError::Internal(format!("create sidecar socket dir: {e}")))?;
    let uniq = ulid::Ulid::new().to_string();
    let main_sock = xvision_ipc::local_socket_path(&sock_dir, &format!("agentd-{uniq}.sock"));
    let cb_sock = xvision_ipc::local_socket_path(&sock_dir, &format!("agentd-{uniq}.cb.sock"));
    let ev_sock = xvision_ipc::local_socket_path(&sock_dir, &format!("agentd-{uniq}.ev.sock"));

    let tool_asset_guard = Arc::new(tokio::sync::RwLock::new(None));
    let as_of_guard: Arc<tokio::sync::RwLock<Option<chrono::DateTime<chrono::Utc>>>> =
        Arc::new(tokio::sync::RwLock::new(None));
    // Build the record-mode cache handle from the minted recording (Task 3.3).
    // Replay-mode wiring is intentionally deferred — production replay trigger
    // (RunTrajectoryMode::Replay → replay: true) lands with the engine-eval
    // replay follow-up; here we only wire the record path.
    let tool_cache = recording.as_ref().map(|(store, rid, _)| ToolHttpCacheHandle {
        store: store.clone(),
        recording_id: rid.clone(),
        replay: false,
    });
    // Read lag + budgets from the SignalToolConfig already embedded in the
    // registry by build_tool_registry — no second xvn.toml parse (im2r.6).
    let (nansen_lag_days, nansen_budget_arc, elfa_budget_arc) = {
        let sig = tools.signal_cfg.as_deref().cloned().unwrap_or_default();
        (
            sig.nansen_lag_days(),
            sig.nansen_budget_arc(),
            sig.elfa_budget_arc(),
        )
    };
    let dispatch: Arc<dyn ToolDispatch> = Arc::new(ToolRegistryDispatch {
        tools: tools.clone(),
        current_asset: tool_asset_guard.clone(),
        run_mode,
        as_of: as_of_guard.clone(),
        nansen_lag_days,
        tool_cache,
        nansen_budget: nansen_budget_arc,
        elfa_budget: elfa_budget_arc,
    });
    let bus = ctx
        .obs_event_bus
        .clone()
        .unwrap_or_else(|| Arc::new(xvision_observability::RunEventBus::new(Vec::new())));

    // Bind the recording sink at spawn time (§2-B). `None` keeps the live
    // path byte-identical to the pre-§2-B behaviour.
    let sink_recording = recording
        .as_ref()
        .map(|(store, rid, _)| (store.clone(), rid.clone()));

    // Bug 2: the sidecar can intermittently fail to start (timing race or
    // cold Node start). Retry up to 3 times with a short delay — the gist
    // repro says retry "usually succeeds on second attempt".
    let mut client_result: Result<AgentClient, xvision_agent_client::AgentClientError> =
        Err(xvision_agent_client::AgentClientError::TransportClosed);
    for attempt in 1..=3u32 {
        match AgentClient::spawn_with_event_sink(
            std::path::Path::new(&bin),
            &main_sock,
            &cb_sock,
            &ev_sock,
            dispatch.clone(),
            bus.clone(),
            sink_recording.clone(),
        )
        .await
        {
            Ok(client) => {
                client_result = Ok(client);
                break;
            }
            Err(e) if attempt < 3 => {
                tracing::warn!(
                    target: "xvision_engine::cline",
                    attempt,
                    error = %e,
                    "sidecar spawn failed, retrying (Bug 2)"
                );
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            Err(e) => client_result = Err(e),
        }
    }
    let client = match client_result {
        Ok(c) => c,
        Err(e) => {
            if let Some((store, recording_id, _)) = recording.as_ref() {
                let _ = store.mark_corrupt(recording_id, rec::RECOVERY_RUN_FAILED).await;
            }
            return Err(ApiError::Internal(format!(
                "failed to spawn xvision-agentd sidecar (XVN_AGENTD_BIN={bin}) after 3 attempts: {e}"
            )));
        }
    };

    // U13: register the agentd sidecar against the run so `eval cancel` can
    // SIGTERM it. The run id is available when a recording was requested (the
    // common Cline eval path; captured into `spawned_run_id` above before the
    // request was consumed). The sidecar supervisor now snapshots the child pid
    // at spawn time, so `cancel` can deliver a real SIGTERM
    // (`CancelOutcome::Signaled`) instead of degrading to a warning.
    if let Some(run_id) = spawned_run_id.as_deref() {
        register_agentd(
            run_id,
            AgentdHandle {
                pid: client.sidecar_pid(),
                socket_path: main_sock.clone(),
            },
        );
    }

    client
        .register_tools(crate::tools::sidecar_descriptors(&tools))
        .await
        .map_err(|e| ApiError::Internal(format!("register agentd tools: {e}")))?;

    // Couple the dispatcher's `StartRunParams.slot_role` to the recording's
    // key slot_role (footgun c): the dispatcher stamps `recording_slot_role`
    // on frames, and `read_frames` filters on it.
    let recording_slot_role = recording.as_ref().map(|(_, _, role)| role.clone());

    let run_recording =
        recording.map(
            |(store, recording_id, slot_role)| crate::agent::cline_recording::RunRecording {
                store,
                recording_id,
                slot_role,
            },
        );

    Ok((
        crate::agent::dispatch_capability::ClineDispatchCtx {
            client: Arc::new(client),
            provider_entry: entry,
            api_key,
            recording_slot_role,
            tool_asset_guard: Some(tool_asset_guard),
            as_of_guard: Some(as_of_guard),
            run_mode,
        },
        run_recording,
    ))
}

/// Inputs for minting a per-run trajectory recording at `spawn_cline_ctx`
/// time (§2-B). Built only when recording is enabled and a primary recorded
/// slot role is available.
struct RecordingRequest {
    /// The eval run id — derives the recording's `cycle_id` + simulation id.
    run_id: String,
    /// The primary recorded slot's role. COUPLED to both the
    /// `TrajectoryKey.slot_role` and the `StartRunParams.slot_role` the
    /// dispatcher stamps (footgun c).
    slot_role: String,
    /// The model id, for the recording key + row.
    model: String,
}

/// Pick the `(slot_role, model)` of the primary recorded slot for §2-B
/// per-run recording. Prefers the trader-role slot (the canonical
/// decision producer), then the legacy `trader_slot`, then the first
/// attached slot with a non-empty model. Returns `None` when no slot has a
/// usable model (a misconfigured strategy that won't reach a Cline run
/// anyway). The role returned is the EXACT `ResolvedAgentSlot.role` the
/// dispatcher matches on so `record_slot_role` couples to it (footgun c).
fn primary_recorded_slot(
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
) -> Option<(String, String)> {
    // 1. Attached agent slot with role == "trader".
    if let Some(trader) = agent_slots
        .iter()
        .find(|resolved| resolved.role.trim().eq_ignore_ascii_case("trader"))
    {
        let model = trader.slot.effective_model();
        if !model.is_empty() {
            return Some((trader.role.clone(), model));
        }
    }
    // 2. Legacy `trader_slot` on the strategy — role is conventionally
    //    "trader".
    if let Some(slot) = strategy.trader_slot.as_ref() {
        let model = slot.effective_model();
        if !model.is_empty() {
            return Some(("trader".to_string(), model));
        }
    }
    // 3. First attached slot with a non-empty model.
    for resolved in agent_slots {
        let model = resolved.slot.effective_model();
        if !model.is_empty() {
            return Some((resolved.role.clone(), model));
        }
    }
    None
}

/// Read the spawned Cline client's latched frame-persist-failure flag
/// (§2-B footgun d). `false` when not recording / no client.
fn recording_persist_failed(client: &Option<Arc<AgentClient>>) -> bool {
    client.as_ref().map(|c| c.recording_failed()).unwrap_or(false)
}

fn runtime_config_path(ctx: &ApiContext) -> std::path::PathBuf {
    xvision_core::config::runtime_config_path(&ctx.xvn_home)
}

/// Parse `xvn.toml` once and return the resolved signal-tool configuration
/// (enabled Nansen + Elfa entries). Best-effort: returns an empty/default
/// `SignalToolConfig` when the config file is missing or malformed so a
/// missing `xvn.toml` never prevents a run from starting.
///
/// Callers must invoke this ONCE per run and pass the result to both
/// `build_tool_registry` and `spawn_cline_ctx` so `xvn.toml` is parsed
/// only once per run (xvision-im2r.6).
fn resolve_signal_tool_config(ctx: &ApiContext) -> crate::tools::signal_policy::SignalToolConfig {
    /// Minimal TOML-deserializable wrapper that only reads `data_tools`, so
    /// the full `RuntimeConfig` (with its required `runtime`/`trader`/`backtest`
    /// sections) doesn't have to be valid.  Used solely inside
    /// `resolve_signal_tool_config`.
    #[derive(serde::Deserialize, Default)]
    struct DataToolsOnly {
        #[serde(default)]
        data_tools: Vec<xvision_core::config::DataToolEntry>,
    }

    let cfg_path = runtime_config_path(ctx);
    let data_tools: Vec<xvision_core::config::DataToolEntry> = std::fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|s| toml::from_str::<DataToolsOnly>(&s).ok())
        .map(|c| c.data_tools)
        .unwrap_or_default();

    let nansen_entry = data_tools
        .iter()
        .find(|e| e.kind == xvision_core::config::DataToolKind::Nansen && e.enabled)
        .cloned();
    let elfa_entry = data_tools
        .iter()
        .find(|e| e.kind == xvision_core::config::DataToolKind::Elfa && e.enabled)
        .cloned();

    crate::tools::signal_policy::SignalToolConfig {
        nansen_entry,
        elfa_entry,
    }
}

/// Build a `ToolRegistry` with builtins plus any enabled signal tools
/// configured in `xvn.toml`. Accepts a pre-resolved `SignalToolConfig` so the
/// config file is not read again (xvision-im2r.6). Best-effort: a missing or
/// malformed `xvn.toml` is handled upstream in `resolve_signal_tool_config`,
/// which returns an empty config so no signal tools are registered. The Nansen
/// and Elfa API keys are resolved from the env vars named by
/// `DataToolEntry.api_key_env`; an empty or missing var silently skips the
/// tool.
///
/// The resolved `SignalToolConfig` is stored on the returned `ToolRegistry` so
/// `spawn_cline_ctx` can read lag/budgets without re-parsing the config.
fn build_tool_registry(
    ctx: &ApiContext,
    sig_cfg: &crate::tools::signal_policy::SignalToolConfig,
) -> ToolRegistry {
    let _ = ctx; // retained for future per-workspace overrides
    let mut registry = ToolRegistry::default_with_builtins();

    // Use the process-global client cache so identical config reuses the same
    // Arc<NansenClient> / Arc<ElfaClient> (and thus the same in-memory rate
    // limiter) across back-to-back runs (xvision-im2r.8).
    let nansen_client = sig_cfg.nansen_entry.as_ref().and_then(|entry| {
        let api_key = std::env::var(&entry.api_key_env).unwrap_or_default();
        if api_key.is_empty() {
            return None;
        }
        Some(xvision_data::client_cache::get_or_create_nansen(
            &entry.base_url,
            &api_key,
            300u32,
        ))
    });

    let elfa_client = sig_cfg.elfa_entry.as_ref().and_then(|entry| {
        let api_key = std::env::var(&entry.api_key_env).unwrap_or_default();
        if api_key.is_empty() {
            return None;
        }
        Some(xvision_data::client_cache::get_or_create_elfa(
            &entry.base_url,
            &api_key,
            60u32,
        ))
    });

    registry.register_signal_tools(nansen_client, elfa_client);
    // Store the resolved config on the registry so `spawn_cline_ctx` can read
    // lag/budgets without re-parsing `xvn.toml` (xvision-im2r.6).
    registry.signal_cfg = Some(std::sync::Arc::new(sig_cfg.clone()));

    registry
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
    // Token cost is priced from OpenRouter (the only catalog that
    // publishes per-token pricing). Slots that run directly against
    // Anthropic / OpenAI would otherwise leave `model_calls.cost_usd`
    // NULL, so ensure an OpenRouter pricing reference is loaded even when
    // OpenRouter isn't a configured execution provider.
    ensure_openrouter_pricing(&svc, &mut out).await;
    out
}

/// Best-effort: guarantee an OpenRouter pricing catalog is present in
/// `out` so token cost can be computed even for slots that run against a
/// non-OpenRouter provider. OpenRouter's `/api/v1/models` is public — no
/// key required — so it works as a pricing reference regardless of which
/// providers the operator configured for *execution*.
///
/// Strictly best-effort and bounded:
///   - If a fresh OpenRouter catalog is already loaded (configured
///     execution provider) or freshly cached on disk, use it — no
///     network.
///   - Only when the cache is missing or stale do we attempt one network
///     fetch under a hard timeout.
///   - On any timeout / error, fall back to whatever (possibly stale)
///     catalog is cached on disk, or nothing.
///
/// A run must never hang or fail because pricing was unavailable — in
/// the worst case `model_calls.cost_usd` stays NULL and the UI shows
/// "unknown" rather than a wrong or zero number. The on-disk cache (24h
/// TTL) makes the network fetch a once-per-TTL cost, not per-run.
async fn ensure_openrouter_pricing(
    svc: &crate::providers::CatalogService,
    out: &mut std::collections::HashMap<String, std::sync::Arc<xvision_core::providers::Catalog>>,
) {
    use std::time::Duration;

    // Bound the cold-fetch so a slow/unreachable OpenRouter can't slow a
    // run by more than this.
    const FETCH_BUDGET: Duration = Duration::from_secs(15);

    let now = chrono::Utc::now();
    let is_fresh = |c: &xvision_core::providers::Catalog| {
        c.source_url.contains("openrouter.ai")
            && !crate::providers::is_stale(c, crate::providers::DEFAULT_TTL, now)
    };

    // 1. A fresh OpenRouter catalog already loaded as a configured
    //    execution provider (under any name)? Nothing to do.
    if out.values().any(|c| is_fresh(c)) {
        return;
    }

    // Keyless reference entry pointed at the canonical public endpoint.
    // Pricing needs no auth, so we deliberately don't depend on an
    // operator having configured an OpenRouter provider or set a key.
    let entry = ProviderEntry {
        name: "openrouter".to_string(),
        kind: ProviderKind::OpenaiCompat,
        base_url: "https://openrouter.ai/api/v1".to_string(),
        api_key_env: String::new(),
        enabled_models: Vec::new(),
    };

    // 2. Fresh on-disk cache under our reference name? Use it — no
    //    network. (refresh() always hits the network, so the staleness
    //    gate lives here, not in get_or_load.)
    if let Ok(Some(cached)) = svc.get_or_load(&entry.name).await {
        if !crate::providers::is_stale(&cached, crate::providers::DEFAULT_TTL, now) {
            out.insert(entry.name.clone(), cached);
            return;
        }
    }

    // 3. Missing or stale — attempt one bounded refresh, falling back to
    //    a stale cache (or nothing) on failure.
    match tokio::time::timeout(FETCH_BUDGET, svc.refresh(&entry)).await {
        Ok(Ok(cat)) => {
            out.insert(entry.name.clone(), cat);
        }
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "openrouter pricing refresh failed; using cached if any");
            if let Ok(Some(cat)) = svc.get_or_load(&entry.name).await {
                out.entry(entry.name.clone()).or_insert(cat);
            }
        }
        Err(_elapsed) => {
            tracing::debug!("openrouter pricing refresh timed out; using cached if any");
            if let Ok(Some(cat)) = svc.get_or_load(&entry.name).await {
                out.entry(entry.name.clone()).or_insert(cat);
            }
        }
    }
}

/// Testable / deps-injecting variant of `run`. Tests pass a
/// `MockBrokerSurface` + `MockDispatch` so no network is required;
/// production callers go through `run` which constructs deps from env.
///
/// `broker` is ignored by the collapsed Backtest path today and reserved for
/// follow-on Live wiring.
pub async fn run_with_deps(
    ctx: &ApiContext,
    req: EvalRunRequest,
    broker: Option<Arc<dyn BrokerSurface>>,
    dispatch: Arc<dyn LlmDispatch>,
    findings_model: String,
    tools: Arc<ToolRegistry>,
) -> ApiResult<Run> {
    let started = Instant::now();
    validate_provider_override_shape(req.provider_override.as_ref())?;
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
    let mut agent_slots = resolve_agent_slots(ctx, &strategy).await?;
    validate_eval_trader_source(&strategy, &agent_slots)?;
    // Per-launch override (Wave B #5): resolve against the canonical
    // `effective_providers::resolve_provider` gate. An unreachable
    // override (key_missing / model_disabled / provider_unknown /
    // provider_disabled) refuses the launch with the typed reason.
    //
    // This gate runs in `run_inner` (not only in `build_eval_dispatch`)
    // so it covers the `run_with_deps` entry point too — that path lets
    // callers inject a `MockDispatch`, bypassing the dispatch builder,
    // but the override still must be validated against the resolver
    // before slot rewriting.
    if let Some(o) = req.provider_override.as_ref() {
        let cfg_path = runtime_config_path(ctx);
        if let Err(unavailable) =
            crate::api::settings::providers::resolve_provider(ctx, &cfg_path, &o.provider, Some(&o.model))
                .await
        {
            let model_clause = unavailable
                .model
                .as_ref()
                .map(|m| format!(" model `{m}`,"))
                .unwrap_or_default();
            return Err(ApiError::Validation(format!(
                "per-launch override provider `{}`{} is not launchable (reason={}): {}",
                unavailable.provider,
                model_clause,
                unavailable.reason.as_str(),
                unavailable.hint,
            )));
        }
    }
    // Rewrite the resolved slots so the executor's `model_id` parameter
    // on every model call matches the override. When `run_with_deps`
    // wires a pre-built dispatch in, the override only swaps the slot's
    // `(provider, model)` echoed onto observability; when `run` builds
    // the dispatch from the override, the resolver above already
    // produced the matching `ProviderEntry`.
    apply_provider_override(&mut agent_slots, req.provider_override.as_ref());

    validate_live_request_shape(&req)?;
    let live_config = req.live_config.clone();

    // 2. Look up the scenario for Backtest, or synthesize the scenario-like
    //    envelope Live still needs internally for capital/venue/cadence helpers.
    let (scenario, from_db) = if let Some(cfg) = live_config.as_ref() {
        (scenario_from_live_config(cfg), false)
    } else {
        resolve_scenario_with_source(ctx, &req.scenario_id).await?
    };

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
    if let Some(cfg) = live_config.clone() {
        run = run.with_live_config(cfg);
    }
    apply_review_launch_options(&mut run, &req);
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
    let obs_config = effective_obs_config(ctx);
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
                &obs_config,
            ))
            .with_blob_store(blob_store)
            .with_catalogs(obs_catalogs.clone())
    });

    // WU-6: runtime is always Cline. Spawn the sidecar unconditionally.
    // An unmapped provider or an unset XVN_AGENTD_BIN surfaces as a typed
    // error here — never a silent fallback.
    let (agent_runtime, agent_runtime_reason) = resolve_agent_runtime(ctx).await;
    // `run_recording` is `Some` only when recording is enabled (per-run
    // `trajectory_mode = record`) AND a Cline client was spawned with a
    // recording sink. The eval finalizer below closes it out (complete /
    // corrupt) after the run.
    let (cline_ctx, run_recording) = {
        let provider_name = select_eval_provider(ctx, &strategy, &agent_slots).await?;
        let cfg_path = runtime_config_path(ctx);
        let entry = crate::api::settings::providers::resolve_provider(ctx, &cfg_path, &provider_name, None)
            .await
            .map_err(|u| {
                ApiError::Validation(format!(
                    "agent_runtime = cline: provider `{}` is not launchable (reason={}): {}",
                    u.provider,
                    u.reason.as_str(),
                    u.hint
                ))
            })?;
        // §2-D: build the recording request when the run's per-run
        // `trajectory_mode` selects `Record` (the operator-chosen config
        // driver — replaces the §2-B env gate) and we can identify a primary
        // recorded slot (the trader). The recording is keyed by this slot's
        // role, which the dispatcher then stamps on every recorded frame
        // (footgun c coupling). `trajectory_mode != Record` ⇒ `None` ⇒ the
        // spawn binds no sink and the live path is byte-identical to pre-§2-D.
        let recording_request = if req.trajectory_mode.records() {
            primary_recorded_slot(&strategy, &agent_slots).map(|(slot_role, model)| RecordingRequest {
                run_id: run.id.clone(),
                slot_role,
                model,
            })
        } else {
            None
        };
        let (cctx, rec) = spawn_cline_ctx(ctx, entry, tools.clone(), recording_request, req.mode).await?;
        (Some(cctx), rec)
    };
    // The recorder needs the spawned client's persist-failure flag at
    // finalize time; clone the Arc so the finalizer can read it after the
    // client has been threaded into the executor.
    let recording_client = cline_ctx.as_ref().map(|c| c.client.clone());

    // 3. Pick the executor for this run mode. For backtest mode, when the
    //    scenario came from the DB we try to source bars through the
    //    cache wrapper (`eval::bars::load_bars`); on miss / fetch error
    //    we fall back to the legacy `data/probes/<cache_key>.parquet`
    //    loader so existing test fixtures keep working.
    let executor_result: ApiResult<Box<dyn RunExecutor>> = match req.mode {
        RunMode::Backtest => {
            build_backtest_executor(
                ctx,
                &scenario,
                from_db,
                &strategy,
                req.assets_subset.as_deref(),
                obs_emitter.clone(),
                provider_catalogs.clone(),
                req.limits.as_ref(),
                agent_runtime,
                cline_ctx,
            )
            .await
        }
        RunMode::Live => {
            build_live_executor(
                ctx,
                live_config
                    .as_ref()
                    .expect("validate_live_request_shape requires live_config"),
                broker,
                obs_emitter.clone(),
                provider_catalogs.clone(),
                req.limits.as_ref(),
                agent_runtime,
                cline_ctx,
            )
            .await
        }
    };
    let executor = match executor_result {
        Ok(executor) => executor,
        Err(e) => {
            if let Some(rec) = run_recording.as_ref() {
                rec.finalize(false, recording_persist_failed(&recording_client))
                    .await;
            }
            return Err(e);
        }
    };

    let store = RunStore::new(ctx.db.clone());
    if let Err(e) = store
        .create(&run)
        .await
        .map_err(|e| ApiError::Internal(format!("create run: {e}")))
    {
        if let Some(rec) = run_recording.as_ref() {
            rec.finalize(false, recording_persist_failed(&recording_client))
                .await;
        }
        return Err(e);
    }
    // Seed the `agent_runs` baseline row synchronously so any
    // supervisor_notes / observability spans written below have a valid
    // FK target. The bus-driven `emit_run_started` (a few lines down)
    // is async and races the very next writes — that race is what
    // produced the QA "agent run not found" View Trace error across
    // multiple QA cycles. The bus recorder's RunStarted handler is now
    // an UPSERT, so it backfills metadata onto this baseline rather
    // than UNIQUE-conflicting. Single-id pattern (`agent_runs.id ==
    // eval_runs.id`) preserves the frontend's
    // `traceRunId = agent_run_id ?? eval_run.id` fallback contract.
    if let Err(e) = store
        .ensure_agent_run_baseline(&run.id, obs_config.retention.mode.as_db_str())
        .await
    {
        if let Some(rec) = run_recording.as_ref() {
            rec.finalize(false, recording_persist_failed(&recording_client))
                .await;
        }
        return Err(ApiError::Internal(format!("ensure agent_runs baseline: {e}")));
    }
    // Persist the per-launch override receipt as soon as the run row
    // exists. We write it here (not only in the outer `run` wrapper) so
    // `run_with_deps` callers — including the test surface that injects
    // a pre-built dispatch — also produce the receipt for `xvn eval
    // results --json` and the export.
    record_provider_override_note(&store, &run.id, req.provider_override.as_ref()).await;
    record_agent_runtime_note(&store, &run.id, agent_runtime, agent_runtime_reason).await;
    let started = match store
        .begin_running(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("begin run: {e}")))
    {
        Ok(started) => started,
        Err(e) => {
            if let Some(rec) = run_recording.as_ref() {
                rec.finalize(false, recording_persist_failed(&recording_client))
                    .await;
            }
            return Err(e);
        }
    };
    if !started {
        if let Some(rec) = run_recording.as_ref() {
            rec.finalize(false, recording_persist_failed(&recording_client))
                .await;
        }
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
        em.emit_run_started(objective, obs_config.retention.mode.as_db_str())
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
        // §2-B (footgun d): the run errored while a recording was open —
        // mark it corrupt so a partial trajectory is never replayed.
        if let Some(rec) = run_recording.as_ref() {
            rec.finalize(false, recording_persist_failed(&recording_client))
                .await;
        }
        // Index the failed run so it shows up in ⌘K with its current status
        // — operators frequently want to find a recently-failed run by id
        // prefix without leaving the palette.
        if let Ok(failed) = store.get(&run.id).await {
            if let Err(e) = api_search::upsert_run(ctx, &failed).await {
                tracing::warn!(error = %e, run_id = %run.id, "search index upsert (run) failed");
            }
        }
        if let Some(em) = obs_emitter.as_ref() {
            em.emit_run_finished(xvision_observability::RunStatus::Failed, Some(err_msg.clone()))
                .await;
        }
        return Err(ApiError::Internal(format!("executor: {err_msg}")));
    }

    // §2-B (footgun d): the run completed — mark the recording complete,
    // OR corrupt if the frame persist path latched a failure mid-run (store
    // fatal / dead consumer). `recording_persist_failed` reads the client's
    // latched flag set by the event-sink persister.
    if let Some(rec) = run_recording.as_ref() {
        rec.finalize(true, recording_persist_failed(&recording_client))
            .await;
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

    if let Err(e) = api_search::upsert_run(ctx, &finalized).await {
        tracing::warn!(error = %e, run_id = %finalized.id, "search index upsert (run) failed");
    }
    fire_chain_attestation_after_finalize(&finalized);

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
    if finalized.auto_fire_review {
        crate::eval::review::auto::fire_auto_review(&store_for_auto, &finalized.id).await;
    }

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
/// returned bars feed `Executor::with_bars`. Errors surface
/// fetch / cache failures so the caller can decide whether to fall
/// back to the legacy fixture loader.
async fn load_bars_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
    asset: xvision_core::trading::AssetSymbol,
    granularity: xvision_data::alpaca::BarGranularity,
) -> ApiResult<Vec<xvision_data::alpaca::MarketBar>> {
    let asset = asset.as_alpaca_pair();
    // Multi-asset correctness (QA 2026-06-03): per-asset bar loads MUST key the
    // cache by asset and strategy-derived granularity, not by scenario id.
    let cache_key = crate::eval::bars::compute_cache_key(
        &asset,
        granularity,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );
    crate::eval::bars::load_bars(
        ctx,
        &crate::eval::bars::BarCacheArgs {
            cache_key,
            asset_pair: asset,
            granularity,
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
    asset: xvision_core::trading::AssetSymbol,
    granularity: xvision_data::alpaca::BarGranularity,
) -> ApiResult<Vec<xvision_data::alpaca::MarketBar>> {
    let asset = asset.as_alpaca_pair();
    crate::eval::bars::load_warmup_bars(
        ctx,
        &asset,
        granularity,
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
            granularity.as_alpaca_str(),
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

async fn load_market_data_context_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
    strategy: &crate::strategies::Strategy,
    assets: &[xvision_core::trading::AssetSymbol],
) -> ApiResult<crate::eval::market_data::MarketDataContext> {
    let mut market_data = crate::eval::market_data::MarketDataContext::new();
    let native_granularity =
        crate::strategies::bar_granularity_for_cadence(strategy.manifest.decision_cadence_minutes);
    for asset in assets {
        let bars =
            market_bars_to_ohlcv(load_bars_for_scenario(ctx, scenario, *asset, native_granularity).await?);
        market_data.insert_series(*asset, native_granularity, bars);
    }
    for (tf, support) in strategy.supported_timeframes() {
        if support == crate::strategies::TimeframeSupport::Native {
            continue;
        }
        let granularity = match tf.as_str() {
            "1m" => xvision_data::alpaca::BarGranularity::Minute1,
            "5m" => xvision_data::alpaca::BarGranularity::Minute5,
            "15m" => xvision_data::alpaca::BarGranularity::Minute15,
            "30m" => xvision_data::alpaca::BarGranularity::new(
                30,
                xvision_data::alpaca::BarGranularityUnit::Minute,
            )
            .expect("validated 30m granularity"),
            "1h" => xvision_data::alpaca::BarGranularity::Hour1,
            "2h" => {
                xvision_data::alpaca::BarGranularity::new(2, xvision_data::alpaca::BarGranularityUnit::Hour)
                    .expect("validated 2h granularity")
            }
            "4h" => xvision_data::alpaca::BarGranularity::Hour4,
            "1d" => xvision_data::alpaca::BarGranularity::Day1,
            _ => continue,
        };
        for asset in assets {
            let pair = asset.as_alpaca_pair();
            let cache_key = crate::eval::bars::compute_cache_key(
                &pair,
                granularity,
                scenario.time_window.start,
                scenario.time_window.end,
                "alpaca-historical-v1",
            );
            let bars = crate::eval::bars::load_bars(
                ctx,
                &crate::eval::bars::BarCacheArgs {
                    cache_key,
                    asset_pair: pair,
                    granularity,
                    start: scenario.time_window.start,
                    end: scenario.time_window.end,
                    data_source_tag: "alpaca-historical-v1".into(),
                },
            )
            .await?;
            market_data.insert_series(*asset, granularity, market_bars_to_ohlcv(bars));
        }
    }
    Ok(market_data)
}

// `load_ohlcv_for_scenario`, `build_paper_executor`, and
// `paper_min_notional_usd` were removed
// alongside the paper-mode-executor-deleted deletion (executor-collapse-paper-mode,
// 2026-05-22). Backtest mode never used them; the future Live wiring
// owns its own broker + min-notional surface in the
// `live-bar-source-alpaca` track. The risk-config crate's `"paper"`
// venue-id label is preserved (separate concept from RunMode); see
// `xvision_core::config::RiskVenueLimits`.

/// Build the backtest executor, fanning out bar-loading over the strategy's
/// active asset set (multi-asset B7, Task C3).
///
/// The active asset set is `active_assets(&strategy.manifest.asset_universe,
/// assets_subset)`. When `assets_subset` is `Some`, only the listed assets are
/// loaded and the executor's own `active_assets` call is kept in sync via
/// `Executor::with_asset_subset`. For the DB-resolved path each active
/// asset's bars are loaded via `load_bars_for_scenario` and injected as a
/// per-asset map (`with_asset_bars`).
///
/// Single-asset preservation: when exactly one asset is active the DB path
/// still calls `with_bars(ohlcv)` and the legacy fixture path still calls
/// `Executor::new()` — byte-identical to the pre-B7 behavior. The
/// multi-asset map (`with_asset_bars`) is only taken when 2+ assets are
/// active.
///
/// Preflight: for the DB path, missing bars / warmup for ANY active asset is
/// a hard `ApiError::Validation`, and the message names the offending asset
/// so the operator knows which `xvn bars fetch` to run. The single-asset
/// error shape is preserved.
#[allow(clippy::too_many_arguments)]
async fn build_backtest_executor(
    ctx: &ApiContext,
    scenario: &Scenario,
    from_db: bool,
    strategy: &crate::strategies::Strategy,
    assets_subset: Option<&[xvision_core::trading::AssetSymbol]>,
    obs: Option<crate::agent::observability::ObsEmitter>,
    provider_catalogs: std::collections::HashMap<String, std::sync::Arc<xvision_core::providers::Catalog>>,
    limits: Option<&crate::eval::limits::EvalLimits>,
    agent_runtime: AgentRuntime,
    cline: Option<crate::agent::dispatch_capability::ClineDispatchCtx>,
) -> ApiResult<Box<dyn RunExecutor>> {
    use crate::eval::executor::asset_set::active_assets;
    // Multi-asset (B7): resolve the active set. `subset` is `None` for full-universe
    // runs; `Some(slice)` when the CLI `--assets` flag narrows the run (Task C3).
    // `active_assets` validates that every subset entry is in the universe and
    // returns the filtered list.
    let active = active_assets(&strategy.manifest.asset_universe, assets_subset)
        .map_err(|e| ApiError::Validation(e.to_string()))?;
    let first_asset = *active
        .first()
        .ok_or_else(|| ApiError::Validation("strategy asset_universe resolved empty".into()))?;
    if from_db {
        let market_data = load_market_data_context_for_scenario(ctx, scenario, strategy, &active).await?;
        let mut asset_bars = std::collections::BTreeMap::new();
        let mut first_err: Option<String> = None;
        let native_granularity =
            crate::strategies::bar_granularity_for_cadence(strategy.manifest.decision_cadence_minutes);
        for asset in &active {
            match market_data.series(*asset, native_granularity) {
                Some(bars) if !bars.is_empty() => {
                    asset_bars.insert(*asset, bars.to_vec());
                }
                Some(_) => {
                    first_err.get_or_insert_with(|| {
                        format!("{}: no bars loaded for scenario window", asset.as_alpaca_pair())
                    });
                }
                None => {
                    first_err.get_or_insert_with(|| {
                        format!("{}: missing native timeframe context", asset.as_alpaca_pair())
                    });
                }
            }
        }
        if let Some(err) = first_err {
            return Err(missing_bars_validation(scenario, Some(err)));
        }
        if !asset_bars.is_empty() {
            // Warmup is a hard preflight error when DB-resolved: an
            // operator who set `warmup_bars > 0` expects real
            // pre-window context, not silent emptiness.
            let warmup = market_bars_to_ohlcv(
                load_warmup_for_scenario(ctx, scenario, first_asset, native_granularity).await?,
            );
            let mut bt = if asset_bars.len() == 1 && asset_bars.contains_key(&first_asset) {
                Executor::with_bars(asset_bars.remove(&first_asset).unwrap())
            } else {
                // Multi-asset: fan out over the per-asset map.
                Executor::new().with_asset_bars(asset_bars)
            };
            bt = bt
                .with_market_data(market_data)
                .with_warmup(warmup)
                .with_event_bus(ctx.event_bus.clone())
                .with_provider_catalogs(provider_catalogs)
                .with_cline_runtime(agent_runtime, cline);
            if let Some(subset) = assets_subset {
                bt = bt.with_asset_subset(subset.to_vec());
            }
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
        return Err(missing_bars_validation(
            scenario,
            Some("no bars loaded for any active asset".to_string()),
        ));
    } else if !legacy_fixture_exists(scenario) {
        return Err(missing_bars_validation(scenario, None));
    }

    let mut bt = Executor::new()
        .with_event_bus(ctx.event_bus.clone())
        .with_provider_catalogs(provider_catalogs)
        .with_cline_runtime(agent_runtime, cline);
    // Task C3: thread the subset through for the legacy path too.
    if let Some(subset) = assets_subset {
        bt = bt.with_asset_subset(subset.to_vec());
    }
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

/// Live execution venue resolved from `live_config.broker_creds_ref`.
/// `AlpacaPaper` is the original paper-trading scope; `OrderlyTestnet` executes
/// on the Orderly Network testnet, and real-money perps venues are allowed only
/// when their explicit mainnet gates and `venue_label` checks pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveVenue {
    AlpacaPaper,
    OrderlyTestnet,
    /// Orderly Network mainnet (real-money) perps execution while Alpaca
    /// supplies the live market-data stream. Gated by `broker_creds_ref =
    /// "orderly_mainnet"`; the default `ORDERLY_BASE_URL` (api-evm.orderly.org)
    /// is mainnet, so a testnet URL on this venue is rejected fail-closed.
    OrderlyMainnet,
    /// Byreal perps (executes on Hyperliquid via the perps CLI) while Alpaca
    /// supplies the live market-data stream. Mainnet is gated by `venue_label`,
    /// network consistency, real-money acknowledgement, and the `SafetyGate`.
    ByrealLive,
    /// Degen Arena — AI Pot / Hyperliquid perps via `DegenArenaSurface`. Alpaca
    /// supplies live market-data bars while Degen Arena executes on Hyperliquid.
    /// Testnet is always permitted; mainnet requires DEGEN_ALLOW_MAINNET=1.
    DegenArena,
    /// Plain native Hyperliquid perps via `HyperliquidSurface` (EIP-712 signed
    /// in Rust, no npm). Distinct from `DegenArena` — carries no Arena/AI-Pot
    /// product framing. Uses HL-native bars (no Alpaca data needed).
    /// Testnet always permitted; mainnet requires `HL_ALLOW_MAINNET=1`.
    Hyperliquid,
    /// Byreal Solana spot (curated SPL + xStocks) via `byreal-cli`. Long/Flat
    /// only (no shorting, no leverage); marks come from byreal-cli token price
    /// (poll-only, no Alpaca data). Mainnet gated by `venue_label`=Live + the
    /// SafetyGate + the `BYREAL_SPOT_NETWORK` consistency check, like ByrealLive.
    ByrealSpot,
}

/// Gate `broker_creds_ref` to the supported live venues. For
/// `"orderly_testnet"`, HARD-REQUIRE that `ORDERLY_BASE_URL` is set and
/// points at a testnet gateway — mirroring the Alpaca paper-only guard, so a
/// mainnet (real-money) Orderly config can never slip through by omission.
fn resolve_live_venue(
    broker_creds_ref: &str,
    orderly_base_url: Option<&str>,
    // byreal mainnet is gated by venue_label + SafetyGate, not by refusing to
    // resolve here, so resolve_live_venue no longer inspects the byreal network.
    _byreal_network: Option<&str>,
    degen_network: Option<&str>,
    hl_network: Option<&str>,
) -> ApiResult<LiveVenue> {
    match broker_creds_ref {
        "alpaca" => Ok(LiveVenue::AlpacaPaper),
        "orderly_testnet" => {
            let Some(url) = orderly_base_url.map(str::trim).filter(|s| !s.is_empty()) else {
                return Err(ApiError::Validation(
                    "live_config.broker_creds_ref 'orderly_testnet' requires ORDERLY_BASE_URL to be \
                     set to the Orderly testnet gateway (e.g. https://testnet-api-evm.orderly.org). \
                     Refusing to fall back to the mainnet default — real-money Orderly is out of \
                     scope for the current live scope."
                        .into(),
                ));
            };
            if !url.contains("testnet") {
                return Err(ApiError::Validation(format!(
                    "current live scope for Orderly is testnet only; ORDERLY_BASE_URL must point at \
                     a testnet gateway containing 'testnet' (got '{url}'). \
                     Real-money Orderly mainnet is out of scope for the current live scope."
                )));
            }
            Ok(LiveVenue::OrderlyTestnet)
        }
        "orderly_mainnet" => {
            // Real-money mainnet. ORDERLY_BASE_URL is OPTIONAL here: when unset
            // or blank, OrderlyLiveSurface::from_env() defaults to the
            // production gateway (https://api-evm.orderly.org). Fail-closed
            // mirror of the testnet guard — if a URL *is* provided it must NOT
            // be a testnet gateway, so a stale testnet env can never silently
            // masquerade as mainnet. We echo the offending URL (a non-secret
            // gateway hostname) to make the misconfig obvious.
            if let Some(url) = orderly_base_url.map(str::trim).filter(|s| !s.is_empty()) {
                if url.to_ascii_lowercase().contains("testnet") {
                    return Err(ApiError::Validation(format!(
                        "live_config.broker_creds_ref 'orderly_mainnet' is real-money mainnet, but \
                         ORDERLY_BASE_URL points at a testnet gateway containing 'testnet' \
                         (got '{url}'). Unset ORDERLY_BASE_URL to use the mainnet default \
                         (https://api-evm.orderly.org), or point it at the mainnet gateway."
                    )));
                }
            }
            Ok(LiveVenue::OrderlyMainnet)
        }
        "byreal" => {
            // Byreal perps execute on Hyperliquid via the perps CLI; the live
            // bar stream is still Alpaca. The testnet/mainnet split is carried
            // by the run's venue_label (Testnet vs Live), enforced by the
            // SafetyGate + the venue_label<->network consistency check (WU-2.3),
            // NOT by refusing to resolve mainnet here.
            Ok(LiveVenue::ByrealLive)
        }
        "byreal_spot" => {
            // Byreal Solana spot via byreal-cli. Like "byreal", the testnet/
            // mainnet split is carried by the run's venue_label (Testnet vs Live),
            // enforced by the SafetyGate + the BYREAL_SPOT_NETWORK consistency
            // check, NOT by refusing to resolve mainnet here.
            Ok(LiveVenue::ByrealSpot)
        }
        "degen_arena" => {
            // Gating policy for Degen Arena (AI Pot / Hyperliquid perps):
            // - Testnet is always allowed (DEGEN_HL_NETWORK contains "testnet").
            // - Mainnet requires explicit opt-in via DEGEN_ALLOW_MAINNET=1 because
            //   the AI Pot is a real-money venue. We name env vars but never
            //   interpolate their values into error responses (cred-safety policy).
            let is_testnet = degen_network
                .map(str::trim)
                .map(|n| n.to_ascii_lowercase().contains("testnet"))
                .unwrap_or(false);
            if is_testnet {
                return Ok(LiveVenue::DegenArena);
            }
            // Mainnet path: require explicit DEGEN_ALLOW_MAINNET=1 opt-in.
            let allow_mainnet = std::env::var("DEGEN_ALLOW_MAINNET")
                .ok()
                .map(|v| matches!(v.trim(), "1" | "true"))
                .unwrap_or(false);
            if !allow_mainnet {
                return Err(ApiError::Validation(
                    "mainnet Degen Arena is gated: DEGEN_HL_NETWORK is not set to testnet and \
                     DEGEN_ALLOW_MAINNET is not set to '1'. \
                     Set DEGEN_ALLOW_MAINNET=1 to enable real-money AI-Pot trading on Hyperliquid mainnet. \
                     For testnet runs set DEGEN_HL_NETWORK to a testnet value."
                        .into(),
                ));
            }
            Ok(LiveVenue::DegenArena)
        }
        "hyperliquid" => {
            // Gating policy for the plain native Hyperliquid venue:
            // - Testnet is always allowed (HL_NETWORK contains "testnet").
            // - Mainnet requires explicit opt-in via HL_ALLOW_MAINNET=1.
            // We name env vars but never interpolate their values into error
            // responses (cred-safety policy).
            let is_testnet = hl_network
                .map(str::trim)
                .map(|n| n.to_ascii_lowercase().contains("testnet"))
                .unwrap_or(false);
            if is_testnet {
                return Ok(LiveVenue::Hyperliquid);
            }
            // Mainnet path: require explicit HL_ALLOW_MAINNET=1 opt-in.
            let allow_mainnet = std::env::var("HL_ALLOW_MAINNET")
                .ok()
                .map(|v| matches!(v.trim(), "1" | "true"))
                .unwrap_or(false);
            if !allow_mainnet {
                return Err(ApiError::Validation(
                    "mainnet Hyperliquid is gated: HL_NETWORK is not set to testnet and \
                     HL_ALLOW_MAINNET is not set to '1'. \
                     Set HL_ALLOW_MAINNET=1 to enable real-money trading on Hyperliquid mainnet. \
                     For testnet runs set HL_NETWORK to a testnet value."
                        .into(),
                ));
            }
            Ok(LiveVenue::Hyperliquid)
        }
        other => Err(ApiError::Validation(format!(
            "live_config.broker_creds_ref '{other}' is not supported. \
             Supported venues: \"alpaca\" (Alpaca paper trading), \"orderly_testnet\" \
             (Orderly Network testnet execution with Alpaca market data), \"orderly_mainnet\" \
             (Orderly Network mainnet / real-money execution with Alpaca market data), \"byreal\" \
             (Byreal perps on Hyperliquid via the perps CLI with Alpaca market data; \
             network from BYREAL_NETWORK, mainnet gated by venue_label=Live), \
             \"byreal_spot\" (Byreal Solana spot — curated SPL + xStocks via byreal-cli; \
             Long/Flat only, marks from byreal-cli token price, mainnet gated by venue_label=Live), \
             \"degen_arena\" (Degen Arena / Hyperliquid perps with Alpaca market data; \
             testnet requires DEGEN_HL_NETWORK=testnet, mainnet requires DEGEN_ALLOW_MAINNET=1), \
             and \"hyperliquid\" (native Hyperliquid perps; \
             testnet requires HL_NETWORK=testnet, mainnet requires HL_ALLOW_MAINNET=1)."
        ))),
    }
}

/// Defense-in-depth build-time consistency check for the network-derived
/// real-money venues (Byreal / Hyperliquid / Degen Arena): the run's configured
/// `venue_label` (from `LiveConfig`) must match the broker label derived from
/// the venue's network env (`BYREAL_NETWORK` / `HL_NETWORK` / `DEGEN_HL_NETWORK`).
/// A mismatch means the operator configured the run for one environment (e.g.
/// Live) but pointed the network env at another (e.g. testnet), which would
/// produce a silently-wrong execution. Rejecting at build-time gives a clearer
/// error than a runtime gate denial and catches both directions: Live-label +
/// testnet-network and Testnet-label + mainnet-network. Fixed paper/testnet
/// venues are a no-op; fixed mainnet venues must use `venue_label=Live`.
fn check_venue_label_network(venue: LiveVenue, run: VenueLabel, broker: VenueLabel) -> ApiResult<()> {
    if venue == LiveVenue::OrderlyMainnet {
        if run != VenueLabel::Live || broker != VenueLabel::Live {
            return Err(ApiError::Validation(format!(
                "OrderlyMainnet run venue_label ({run:?}) must be Live; \
                 broker label resolved to {broker:?}."
            )));
        }
        return Ok(());
    }

    // Only the network-derived real-money venues carry a testnet/mainnet split;
    // Alpaca (Paper) and Orderly testnet have a fixed non-mainnet label and are a no-op.
    let env_var = match venue {
        LiveVenue::ByrealLive => "BYREAL_NETWORK",
        LiveVenue::ByrealSpot => "BYREAL_SPOT_NETWORK",
        LiveVenue::Hyperliquid => "HL_NETWORK",
        LiveVenue::DegenArena => "DEGEN_HL_NETWORK",
        LiveVenue::OrderlyMainnet => return Ok(()),
        LiveVenue::AlpacaPaper | LiveVenue::OrderlyTestnet => return Ok(()),
    };
    if run != broker {
        return Err(ApiError::Validation(format!(
            "{venue:?} run venue_label ({run:?}) must match the broker label \
             ({broker:?}) resolved from {env_var}: use a mainnet network for a Live \
             run or a testnet network for a Testnet run."
        )));
    }
    Ok(())
}

/// Map a `LiveVenue` + optional network values to the `VenueLabel`
/// that describes what the **broker** is doing (as opposed to what the *run*
/// is labelled). Used to populate the `broker_venue_label` argument of
/// `GatedBrokerSurface::new`.
///
/// Rules:
/// - `AlpacaPaper` → `Paper` (Alpaca paper trading, no real money).
/// - `OrderlyTestnet` → `Testnet` (Orderly testnet, on-chain but no real funds).
/// - `OrderlyMainnet` → `Live` (real-money Orderly perps).
/// - `ByrealLive` → from `byreal_network`: "testnet" (case-insensitive) →
///   `Testnet`; anything else (unset, empty, "mainnet", …) → `Live`. Unset/empty
///   is the production-mainnet default for the perps CLI, so we fail safe to the
///   strict `Live` label.
/// - `DegenArena` → from `degen_network` the same way: testnet → `Testnet`, else
///   → `Live` (mainnet Degen Arena is real-money Hyperliquid). This makes the
///   SafetyGate wrap Degen Arena runs too, not just Byreal.
/// - `Hyperliquid` → from `hl_network` the same way: testnet → `Testnet`, else
///   → `Live` (mainnet Hyperliquid is real-money). SafetyGate enforces Live
///   label for real-money runs.
fn broker_label_for(
    venue: LiveVenue,
    // The active byreal-family network: BYREAL_NETWORK for `ByrealLive`,
    // BYREAL_SPOT_NETWORK for `ByrealSpot` (the two are mutually exclusive per
    // call, so they share this slot — the caller passes whichever applies).
    byreal_network: Option<&str>,
    degen_network: Option<&str>,
    hl_network: Option<&str>,
) -> VenueLabel {
    // testnet substring (case-insensitive) ⇒ Testnet; otherwise ⇒ Live (fail safe).
    fn label_from_network(net: Option<&str>) -> VenueLabel {
        let is_testnet = net
            .map(str::trim)
            .map(|n| n.to_ascii_lowercase().contains("testnet"))
            .unwrap_or(false);
        if is_testnet {
            VenueLabel::Testnet
        } else {
            VenueLabel::Live
        }
    }
    match venue {
        LiveVenue::AlpacaPaper => VenueLabel::Paper,
        LiveVenue::OrderlyTestnet => VenueLabel::Testnet,
        LiveVenue::OrderlyMainnet => VenueLabel::Live,
        LiveVenue::ByrealLive => label_from_network(byreal_network),
        LiveVenue::ByrealSpot => label_from_network(byreal_network),
        LiveVenue::DegenArena => label_from_network(degen_network),
        LiveVenue::Hyperliquid => label_from_network(hl_network),
    }
}

async fn build_live_executor(
    ctx: &ApiContext,
    cfg: &LiveConfig,
    broker_override: Option<Arc<dyn BrokerSurface>>,
    obs: Option<crate::agent::observability::ObsEmitter>,
    provider_catalogs: std::collections::HashMap<String, std::sync::Arc<xvision_core::providers::Catalog>>,
    limits: Option<&crate::eval::limits::EvalLimits>,
    agent_runtime: AgentRuntime,
    cline_ctx: Option<crate::agent::dispatch_capability::ClineDispatchCtx>,
) -> ApiResult<Box<dyn RunExecutor>> {
    cfg.validate()
        .map_err(|e| ApiError::Validation(format!("invalid live_config at {}: {e:?}", e.field_path())))?;
    let orderly_creds = broker_settings::resolve_orderly_credentials(&ctx.xvn_home).await?;
    let orderly_base_url = orderly_creds
        .as_ref()
        .and_then(|c| c.base_url.clone())
        .or_else(|| std::env::var("ORDERLY_BASE_URL").ok());
    let byreal_network = std::env::var("BYREAL_NETWORK").ok();
    let byreal_spot_network = std::env::var("BYREAL_SPOT_NETWORK").ok();
    // Resolve Degen Arena creds (stored via Settings → Brokers / deploy ingest
    // win over DEGEN_HL_* env) BEFORE gating, so the testnet/mainnet gate agrees
    // with the network the creds actually carry — not a possibly-unset env var.
    let degen_creds = crate::api::settings::brokers::resolve_degen_arena_credentials(&ctx.xvn_home).await?;
    let degen_network = degen_creds.as_ref().map(|c| c.network.clone());
    // Resolve Hyperliquid creds BEFORE gating (same pattern as degen_arena):
    // stored creds carry the definitive network, not a possibly-unset HL_NETWORK env.
    let hl_creds = crate::api::settings::brokers::resolve_hyperliquid_credentials(&ctx.xvn_home).await?;
    let hl_network = hl_creds.as_ref().map(|c| c.network.clone());
    let venue = resolve_live_venue(
        &cfg.broker_creds_ref,
        orderly_base_url.as_deref(),
        byreal_network.as_deref(),
        degen_network.as_deref(),
        hl_network.as_deref(),
    )?;
    if cfg.assets.is_empty() {
        return Err(ApiError::Validation(
            "live_config.assets must contain at least one asset".into(),
        ));
    }
    // Byreal-spot needs the curated SPL/xStock set for symbol→mint resolution
    // (both for the broker surface and the price-poll mark source). Load it once
    // here so a misconfigured set fails fast with a clear error.
    let byreal_spot_assets = if venue == LiveVenue::ByrealSpot {
        let cfg_path = xvision_core::config::spot_assets_path(&ctx.xvn_home);
        Some(xvision_core::config::load_spot_assets(&cfg_path).map_err(|e| {
            ApiError::Validation(format!(
                "byreal_spot requires a curated set at {}: {e}",
                cfg_path.display()
            ))
        })?)
    } else {
        None
    };
    // Degen Arena, the plain Hyperliquid venue, and Byreal-spot all source
    // market-data bars from their own venue (HL-native candles / byreal-cli
    // token price), not Alpaca — so they need no Alpaca credentials. Every
    // other venue still uses the Alpaca bar stream.
    let uses_alpaca_data =
        venue != LiveVenue::DegenArena && venue != LiveVenue::Hyperliquid && venue != LiveVenue::ByrealSpot;
    // Alpaca credentials supply the live bar stream for every venue EXCEPT
    // the HL-native venues (degen_arena, hyperliquid). This message is only
    // surfaced when `uses_alpaca_data` is true.
    let missing_alpaca_creds = || {
        match venue {
        LiveVenue::AlpacaPaper => {
            "no Alpaca credentials configured for Live run (set Settings -> Brokers or APCA_API_KEY_ID/APCA_API_SECRET_KEY)".to_string()
        }
        LiveVenue::OrderlyTestnet => {
            "no Alpaca credentials configured for Live run: Orderly testnet runs still need Alpaca \
             credentials because Alpaca supplies the live market-data stream while Orderly executes \
             the orders. Set Settings -> Brokers or APCA_API_KEY_ID/APCA_API_SECRET_KEY."
                .to_string()
        }
        LiveVenue::OrderlyMainnet => {
            "no Alpaca credentials configured for Live run: Orderly mainnet runs still need Alpaca \
             credentials because Alpaca supplies the live market-data stream while Orderly executes \
             the orders. Set Settings -> Brokers or APCA_API_KEY_ID/APCA_API_SECRET_KEY."
                .to_string()
        }
        LiveVenue::ByrealLive => {
            "no Alpaca credentials configured for Live run: Byreal runs still need Alpaca \
             credentials because Alpaca supplies the live market-data stream while Byreal executes \
             the orders (on Hyperliquid). Set Settings -> Brokers or APCA_API_KEY_ID/APCA_API_SECRET_KEY."
                .to_string()
        }
        LiveVenue::DegenArena => {
            // Unreachable: Degen Arena uses HL-native candles (uses_alpaca_data
            // == false), so the Alpaca-creds requirement is skipped for it.
            "Degen Arena sources bars from Hyperliquid and needs no Alpaca credentials.".to_string()
        }
        LiveVenue::Hyperliquid => {
            // Unreachable: plain Hyperliquid also uses HL-native candles
            // (uses_alpaca_data == false), so the Alpaca-creds requirement is skipped.
            "Hyperliquid sources bars from Hyperliquid and needs no Alpaca credentials.".to_string()
        }
        LiveVenue::ByrealSpot => {
            // Unreachable: Byreal-spot uses byreal-cli token price for marks
            // (uses_alpaca_data == false), so the Alpaca-creds requirement is skipped.
            "Byreal spot sources marks from byreal-cli and needs no Alpaca credentials.".to_string()
        }
    }
    };
    let stored = broker_settings::load_alpaca_credentials(&ctx.xvn_home).await?;
    let (key_id, secret, trade_base_url) = if let Some(c) = stored {
        (
            c.api_key_id,
            c.api_secret_key,
            c.base_url
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "https://paper-api.alpaca.markets".into()),
        )
    } else if uses_alpaca_data {
        let key_id =
            std::env::var("APCA_API_KEY_ID").map_err(|_| ApiError::Validation(missing_alpaca_creds()))?;
        let secret = std::env::var("APCA_API_SECRET_KEY").map_err(|_| {
            ApiError::Validation(format!("{} (APCA_API_SECRET_KEY unset)", missing_alpaca_creds()))
        })?;
        let trade_base_url = std::env::var("APCA_API_BASE_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "https://paper-api.alpaca.markets".into());
        (key_id, secret, trade_base_url)
    } else {
        // HL-native venues (degen_arena, hyperliquid): bars come from Hyperliquid
        // and orders are signed with an HL agent key — no Alpaca credentials needed.
        (
            String::new(),
            String::new(),
            "https://paper-api.alpaca.markets".into(),
        )
    };
    if venue == LiveVenue::AlpacaPaper && !trade_base_url.contains("paper-api.alpaca.markets") {
        return Err(ApiError::Validation(format!(
            "Alpaca live execution is paper-only; \
             APCA_API_BASE_URL must point at https://paper-api.alpaca.markets \
             (got '{trade_base_url}'). \
             For real-money trading use a perps venue (byreal, hyperliquid, degen_arena)."
        )));
    }

    let broker: Arc<dyn BrokerSurface> = match broker_override {
        Some(b) => b,
        None => match venue {
            LiveVenue::AlpacaPaper => Arc::new(
                AlpacaPaperSurface::from_credentials(&key_id, &secret, &trade_base_url)
                    .map_err(|e| ApiError::Validation(format!("build Alpaca paper broker: {e}")))?,
            ),
            LiveVenue::OrderlyTestnet | LiveVenue::OrderlyMainnet => {
                let c = orderly_creds.ok_or_else(|| {
                    ApiError::Validation(
                        "Orderly venue selected but no credentials configured — \
                         store credentials in Settings -> Brokers or set \
                         ORDERLY_KEY / ORDERLY_SECRET / ORDERLY_ACCOUNT_ID."
                            .into(),
                    )
                })?;
                Arc::new(
                    OrderlyLiveSurface::connect(
                        xvision_execution::orderly::Credentials {
                            orderly_key: c.api_key,
                            orderly_secret: c.api_secret,
                            orderly_account_id: c.account_id,
                        },
                        c.base_url.as_deref(),
                    )
                    .map_err(|e| ApiError::Validation(format!("build Orderly broker: {e}")))?,
                )
            }
            LiveVenue::ByrealLive => Arc::new(
                ByrealLiveSurface::from_env()
                    .map_err(|e| ApiError::Validation(format!("build Byreal live broker: {e}")))?,
            ),
            LiveVenue::ByrealSpot => {
                // venue_label decides the swap mode: Live → real `--confirm`;
                // Testnet/Paper → `--dry-run` preview (no-funds forward-test).
                let mode = if cfg.venue_label == VenueLabel::Live {
                    xvision_execution::ByrealSpotMode::Live
                } else {
                    xvision_execution::ByrealSpotMode::Preview
                };
                let assets = byreal_spot_assets
                    .clone()
                    .expect("byreal_spot_assets loaded above when venue == ByrealSpot");
                Arc::new(
                    xvision_execution::ByrealSpotSurface::new(
                        xvision_execution::SubprocessByrealSpotApi::from_env(),
                        assets,
                    )
                    .with_mode(mode),
                )
            }
            LiveVenue::DegenArena => {
                let c = degen_creds.ok_or_else(|| {
                    ApiError::Validation(
                        "Degen Arena selected but no credentials configured — deploy a \
                         trade-only HL key via the /live deploy strip (POST \
                         /api/live/deploy/degen-arena) or set DEGEN_HL_API_KEY / \
                         DEGEN_HL_ACCOUNT_ADDRESS / DEGEN_HL_NETWORK."
                            .into(),
                    )
                })?;
                Arc::new(
                    DegenArenaSurface::from_credentials(&c.api_key, &c.account_address, &c.network)
                        .map_err(|e| ApiError::Validation(format!("build Degen Arena broker: {e}")))?,
                )
            }
            LiveVenue::Hyperliquid => {
                let c = hl_creds.ok_or_else(|| {
                    ApiError::Validation(
                        "Hyperliquid venue selected but no credentials configured — \
                         set HL_API_KEY / HL_ACCOUNT_ADDRESS / HL_NETWORK or store \
                         credentials under the [hyperliquid] section of brokers.toml."
                            .into(),
                    )
                })?;
                Arc::new(
                    HyperliquidSurface::from_credentials(&c.api_key, &c.account_address, &c.network)
                        .map_err(|e| ApiError::Validation(format!("build Hyperliquid broker: {e}")))?,
                )
            }
        },
    };
    // Wrap every broker (including injected overrides) in `GatedBrokerSurface`
    // so the safety gate fires on every live submit, regardless of venue or
    // whether the broker came from a broker_override. The wrap happens AFTER
    // the override/venue match so even injected test brokers are gated.
    // The byreal-family label uses BYREAL_SPOT_NETWORK for spot and
    // BYREAL_NETWORK for perps (the two venues are mutually exclusive per run).
    let byreal_family_network = if venue == LiveVenue::ByrealSpot {
        byreal_spot_network.as_deref()
    } else {
        byreal_network.as_deref()
    };
    let broker_lbl = broker_label_for(
        venue,
        byreal_family_network,
        degen_network.as_deref(),
        hl_network.as_deref(),
    );
    // Defense-in-depth build-time check — for the network-derived real-money
    // venues (Byreal / Hyperliquid / Degen Arena) the run's venue_label must
    // agree with the broker label resolved from the venue's network env. Catches
    // Live-label+testnet and Testnet-label+mainnet BEFORE the executor is built,
    // a clearer error than a runtime gate denial. No-op for Alpaca/Orderly.
    check_venue_label_network(venue, cfg.venue_label, broker_lbl)?;
    let broker: Arc<dyn BrokerSurface> = Arc::new(GatedBrokerSurface::new(
        broker,
        ctx.safety_gate.clone(),
        cfg.venue_label,
        broker_lbl,
        AuthContext::system(),
        cfg.safety_limits.clone(),
    ));
    let granularity = cfg.granularity;
    let live_client = AlpacaLiveClient::new(AlpacaLiveCredentials {
        key_id: key_id.clone(),
        secret_key: secret.clone(),
    });
    let data_base_url = std::env::var("APCA_API_DATA_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://data.alpaca.markets".into());
    let warmup_bars = cfg.warmup_bars.unwrap_or(200);

    // Multi-asset live fanout (§4 L2): build one `LiveStream` per asset in
    // the LiveConfig (subscribe + poll + warmup each), then merge them into
    // a `MultiLiveStream`. A single-asset run yields a 1-element
    // `MultiLiveStream`, which the executor consumes exactly like the L1
    // single `LiveStream` — preserving single-asset byte-identity.
    let mut sub_streams: Vec<(
        xvision_core::trading::AssetSymbol,
        crate::eval::executor::LiveStream,
    )> = Vec::with_capacity(cfg.assets.len());
    for asset_ref in &cfg.assets {
        let asset = asset_ref.venue_symbol.clone();
        let asset_sym = <xvision_core::trading::AssetSymbol as std::str::FromStr>::from_str(&asset)
            .map_err(|e| ApiError::Validation(format!("live_config asset '{asset}': {e}")))?;
        let stream = if uses_alpaca_data {
            let ws = live_client
                .subscribe_bars(&asset, granularity)
                .await
                .map_err(|e| ApiError::Validation(format!("subscribe Alpaca live bars for {asset}: {e}")))?;
            let poll = AlpacaLivePoll::new(
                production_fetcher(data_base_url.clone(), key_id.clone(), secret.clone()),
                asset.clone(),
                granularity,
            );
            let warmup_fetcher = xvision_data::alpaca::AlpacaBarsFetcher::new(
                data_base_url.clone(),
                key_id.clone(),
                secret.clone(),
            );
            crate::eval::executor::LiveStream::new_with_warmup_and_fetcher(
                ctx,
                &asset,
                granularity,
                warmup_bars,
                ws,
                poll,
                Some(&warmup_fetcher),
            )
            .await
            .map_err(|e| ApiError::Validation(format!("build LiveStream for {asset}: {e}")))?
        } else if venue == LiveVenue::ByrealSpot {
            // Byreal spot: live marks from byreal-cli token price, warmup
            // from HlBarFetcher (Hyperliquid public candles — Byreal tokens
            // trade on HL).  One synthetic bar per poll for live marks;
            // warmup gets full OHLCV candles for indicator history.
            let assets = byreal_spot_assets
                .clone()
                .expect("byreal_spot_assets loaded above when venue == ByrealSpot");
            let mark_fetcher = std::sync::Arc::new(crate::eval::executor::ByrealSpotPriceFetcher::new(
                xvision_execution::SubprocessByrealSpotApi::from_env(),
                assets,
            ));
            let poll = AlpacaLivePoll::new(mark_fetcher, asset.clone(), granularity);

            let byreal_network = std::env::var("BYREAL_SPOT_NETWORK").ok();
            let is_testnet = byreal_network
                .as_deref()
                .map(|n| n.to_ascii_lowercase().contains("testnet"))
                .unwrap_or(false);
            let hl_base = if is_testnet {
                xvision_data::hl_bars::HL_TESTNET_INFO
            } else {
                xvision_data::hl_bars::HL_MAINNET_INFO
            };
            let hl_fetcher = xvision_data::hl_bars::production_hl_fetcher(hl_base);
            let warmup = if warmup_bars == 0 {
                Vec::new()
            } else {
                let end = Utc::now();
                let start = end
                    - chrono::Duration::seconds(granularity.seconds() as i64 * (warmup_bars as i64 + 5));
                let bars = hl_fetcher
                    .fetch_window(&asset, granularity, start, end)
                    .await
                    .map_err(|e| ApiError::Validation(format!("byreal-spot hl warmup for {asset}: {e}")))?;
                let mut ohlcv = market_bars_to_ohlcv(bars);
                if ohlcv.len() > warmup_bars as usize {
                    ohlcv = ohlcv.split_off(ohlcv.len() - warmup_bars as usize);
                }
                let got = ohlcv.len() as u32;
                if got == 0 {
                    tracing::warn!(
                        target: "xvision_engine::live_source",
                        asset, granularity = %granularity, requested = warmup_bars,
                        "byreal-spot warmup: HL returned 0 bars. \
                         Agent starts cold — indicators have no history until live bars accumulate.",
                    );
                } else if got < warmup_bars / 2 {
                    tracing::warn!(
                        target: "xvision_engine::live_source",
                        asset, granularity = %granularity, got, requested = warmup_bars,
                        "byreal-spot warmup: only {got}/{warmup_bars} bars loaded",
                    );
                }
                ohlcv
            };
            crate::eval::executor::LiveStream::new_poll_only(warmup, poll)
        } else {
            // HL-native venues (degen_arena, hyperliquid): Hyperliquid-native candles
            // via HlBarFetcher, poll-only (no Alpaca websocket). Warmup is fetched up
            // front from the same source so decision history and live bars share one
            // price basis. Both venues use the same HL bar API; the network is resolved
            // from whichever credential is active.
            let active_network = if venue == LiveVenue::Hyperliquid {
                hl_network.as_deref()
            } else {
                degen_network.as_deref()
            };
            let is_testnet = active_network
                .map(|n| n.to_ascii_lowercase().contains("testnet"))
                .unwrap_or(false);
            let hl_base = if is_testnet {
                xvision_data::hl_bars::HL_TESTNET_INFO
            } else {
                xvision_data::hl_bars::HL_MAINNET_INFO
            };
            let fetcher = xvision_data::hl_bars::production_hl_fetcher(hl_base);
            let warmup = if warmup_bars == 0 {
                Vec::new()
            } else {
                let end = Utc::now();
                let start =
                    end - chrono::Duration::seconds(granularity.seconds() as i64 * (warmup_bars as i64 + 5));
                let bars = fetcher
                    .fetch_window(&asset, granularity, start, end)
                    .await
                    .map_err(|e| ApiError::Validation(format!("hl warmup for {asset}: {e}")))?;
                let mut ohlcv = market_bars_to_ohlcv(bars);
                if ohlcv.len() > warmup_bars as usize {
                    ohlcv = ohlcv.split_off(ohlcv.len() - warmup_bars as usize);
                }
                let got = ohlcv.len() as u32;
                if got == 0 {
                    tracing::warn!(
                        target: "xvision_engine::live_source",
                        asset, granularity = %granularity, requested = warmup_bars,
                        "HL warmup: fetcher returned 0 bars. \
                         Agent starts cold — indicators have no history until live bars accumulate.",
                    );
                } else if got < warmup_bars / 2 {
                    tracing::warn!(
                        target: "xvision_engine::live_source",
                        asset, granularity = %granularity, got, requested = warmup_bars,
                        "HL warmup: only {got}/{warmup_bars} bars loaded",
                    );
                }
                ohlcv
            };
            let poll = AlpacaLivePoll::new(fetcher, asset.clone(), granularity);
            crate::eval::executor::LiveStream::new_poll_only(warmup, poll)
        };
        sub_streams.push((asset_sym, stream));
    }
    let multi = crate::eval::executor::MultiLiveStream::new(sub_streams);
    let mut live = Executor::live(cfg, broker, multi, crate::eval::executor::WallClock::new(), obs)
        .map_err(|e| ApiError::Validation(format!("build Live executor: {e}")))?
        .with_event_bus(ctx.event_bus.clone())
        .with_provider_catalogs(provider_catalogs)
        .with_cline_runtime(agent_runtime, cline_ctx);
    if let Some(recorder) = ctx.memory_recorder.clone() {
        live = live.with_memory_recorder(recorder);
    }
    if let Some(l) = limits {
        live = live.with_limits(l.clone());
    }
    Ok(Box::new(live))
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
/// Sync-up-front validation: env vars (`ANTHROPIC_API_KEY` today) are read
/// before the spawn so missing-config errors return as `ApiError::Validation`
/// rather than landing in the row's `error` field. Strategy/scenario lookups
/// also happen up-front for the same reason.
///
/// Wraps `start_run_inner` with the standard audit-on-both-paths pattern so
/// validation failures (400s) are visible in `api_audit` just like successes.
/// `target` is `None` on the error path (no run row exists yet); callers can
/// correlate the failure via `args_json` (carries `agent_id` + `scenario_id`).
pub async fn start_run(ctx: &ApiContext, req: EvalRunRequest) -> ApiResult<RunDetail> {
    let started = Instant::now();
    let args_json = serde_json::to_string(&req).ok();
    let result = start_run_inner(ctx, req).await;
    let (outcome, target) = match &result {
        Ok(detail) => (Outcome::Ok, Some(detail.summary.id.clone())),
        Err(e) => (Outcome::Error(e.to_string()), None),
    };
    let _ = audit::record(
        ctx,
        "eval",
        "start",
        target.as_deref(),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn start_run_inner(ctx: &ApiContext, req: EvalRunRequest) -> ApiResult<RunDetail> {
    validate_provider_override_shape(req.provider_override.as_ref())?;
    validate_live_request_shape(&req)?;
    let strategy = api_strategy::get(ctx, &req.agent_id).await?;
    let live_config = req.live_config.clone();
    let (scenario, from_db) = if let Some(cfg) = live_config.as_ref() {
        (scenario_from_live_config(cfg), false)
    } else {
        resolve_scenario_with_source(ctx, &req.scenario_id).await?
    };

    // Build broker / dispatch / tools from env up-front so any
    // missing-config errors return synchronously rather than landing in
    // a background-task failure row the user has to dig out of the list.
    //
    // Live mode broker construction is deferred to the launch endpoint
    // (Phase 3, `live-bar-source-alpaca` track). The engine itself only
    // ships Backtest end-to-end today; Live falls through to
    // `Executor::live()` below which returns a stable not-implemented
    // error.
    let _broker: Option<Arc<dyn BrokerSurface>> = None;
    let mut agent_slots = resolve_agent_slots(ctx, &strategy).await?;
    validate_eval_trader_source(&strategy, &agent_slots)?;
    apply_provider_override(&mut agent_slots, req.provider_override.as_ref());

    let provider_names = validate_provider_preflight(ctx, &req, &strategy, &agent_slots).await?;

    // Phase 4 launch gate + launch-preflight guardrails (live eval path).
    // Refuses the launch BEFORE the executor is built/spawned when the
    // strategy is not launchable (missing REQUIRED capability) or a
    // cleanly-reachable short-circuit (unattached slot / missing prompt /
    // missing tool / provider unavailable) fires. A backtest of a strategy
    // missing a required capability never starts. Live mode runs the same
    // gate: a non-launchable strategy must not reach the executor in either
    // mode.
    assert_launchable_with_guardrails(ctx, &req.agent_id, &strategy, &agent_slots).await?;

    let (dispatch, findings_model) =
        build_eval_dispatch(ctx, &strategy, &agent_slots, req.provider_override.as_ref()).await?;
    // Resolve signal-tool config once; pass to both build_tool_registry and
    // spawn_cline_ctx (via the registry) so xvn.toml is parsed only once per
    // run start (xvision-im2r.6).
    let sig_cfg = resolve_signal_tool_config(ctx);
    let tools = Arc::new(build_tool_registry(ctx, &sig_cfg));

    // Other entry point (`run_with_deps_in_progress`) — observability
    // wiring is opt-in via the same ApiContext bus. The emitter is
    // built after `run.id` is available below; `RunStarted` is
    // published only after the `eval_runs` row exists and executor
    // preflight has succeeded, so the recorder's FK is valid and
    // preflight failures can't leave a phantom observability run
    // behind. The matching `RunFinished` is emitted by
    // `execute_in_background`.
    let mut run = Run::new_queued(req.agent_id.clone(), scenario.id.clone(), req.mode);
    run.params_override = req.params_override.clone();
    if let Some(cfg) = live_config.clone() {
        run = run.with_live_config(cfg);
    }
    apply_review_launch_options(&mut run, &req);
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
    let obs_config = effective_obs_config(ctx);
    let obs_emitter = ctx.obs_event_bus.as_ref().map(|bus| {
        // Mirror the FullDebug-aware emitter wiring above; same
        // blob root so the second eval entry point produces refs
        // the dashboard's blob-fetch route resolves to.
        let blob_store = xvision_observability::BlobStore::new(ctx.xvn_home.join("agent_runs").join("blobs"));
        crate::agent::observability::ObsEmitter::new(bus.clone(), run.id.clone())
            .with_retention(crate::agent::observability::ObsRetentionPolicy::from_config(
                &obs_config,
            ))
            .with_blob_store(blob_store)
            .with_catalogs(obs_catalogs.clone())
    });

    // Stage 1 (Cline runtime unification, Task 6): same resolution as
    // `run_inner`. When `Cline` is selected, spawn the sidecar + ctx.
    // §2-B note: this async/background entry point does NOT mint a
    // recording — it passes `None` so the spawn binds no sink and the path
    // is unchanged. Eval-side recording is wired through the synchronous
    // `run_inner` path whose finalizer can close the recording out
    // (complete/corrupt). Extending recording to this path needs a finalize
    // hook inside the spawned task (future work).
    // WU-6: runtime is always Cline.
    let (agent_runtime, agent_runtime_reason) = resolve_agent_runtime(ctx).await;
    let cline_ctx = {
        let provider_name = select_eval_provider(ctx, &strategy, &agent_slots).await?;
        let cfg_path = runtime_config_path(ctx);
        let entry = crate::api::settings::providers::resolve_provider(ctx, &cfg_path, &provider_name, None)
            .await
            .map_err(|u| {
                ApiError::Validation(format!(
                    "agent_runtime = cline: provider `{}` is not launchable (reason={}): {}",
                    u.provider,
                    u.reason.as_str(),
                    u.hint
                ))
            })?;
        let (cctx, _no_recording) = spawn_cline_ctx(ctx, entry, tools.clone(), None, req.mode).await?;
        Some(cctx)
    };

    let executor: Box<dyn RunExecutor> = match req.mode {
        RunMode::Backtest => {
            build_backtest_executor(
                ctx,
                &scenario,
                from_db,
                &strategy,
                req.assets_subset.as_deref(),
                obs_emitter.clone(),
                provider_catalogs.clone(),
                req.limits.as_ref(),
                agent_runtime,
                cline_ctx,
            )
            .await?
        }
        RunMode::Live => {
            build_live_executor(
                ctx,
                live_config
                    .as_ref()
                    .expect("validate_live_request_shape requires live_config"),
                None,
                obs_emitter.clone(),
                provider_catalogs.clone(),
                req.limits.as_ref(),
                agent_runtime,
                cline_ctx,
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

    // Seed the `agent_runs` baseline row synchronously — see the
    // corresponding comment in `run_inner` for the full rationale.
    // Failure here is fatal because every subsequent FK-bearing write
    // (supervisor_notes, observability spans, View Trace export) would
    // otherwise silently break.
    store
        .ensure_agent_run_baseline(&run.id, obs_config.retention.mode.as_db_str())
        .await
        .map_err(|e| ApiError::Internal(format!("ensure agent_runs baseline: {e}")))?;

    // Persist preflight results as supervisor_notes immediately after the run
    // row exists. Uses `info` severity for reachable providers and `warn` for
    // skip_preflight. Best-effort: a failed note write does NOT abort the run.
    write_preflight_supervisor_notes(&store, &run.id, &provider_names, req.skip_preflight).await;
    record_provider_override_note(&store, &run.id, req.provider_override.as_ref()).await;
    record_agent_runtime_note(&store, &run.id, agent_runtime, agent_runtime_reason).await;

    if let Some(em) = obs_emitter.as_ref() {
        let objective = format!(
            "eval:{mode:?}:{scenario}",
            mode = req.mode,
            scenario = scenario.id,
        );
        em.emit_run_started(objective, obs_config.retention.mode.as_db_str())
            .await;
    }

    // F-1 (eval-launch-concurrency-cap, 2026-05-19): cap how many runs
    // can be in flight against a single upstream `(provider, model)`
    // bucket. Resolved from the trader slot (the dominant token spender);
    // findings/regime slots ride along on the same permit because the
    // F-1 audit (`team/intake/2026-05-16-eval-review-and-v2a.md`) tracked
    // the burst as a single user-perceived "launch". The guard is moved
    // into the spawned background task so it lives for the full run
    // lifecycle and is dropped (releasing the permit) when the task
    // exits — including via panic.
    let (gate_provider, gate_model) = resolve_launch_gate_key(&strategy, &agent_slots, &findings_model);
    let launch_permit = ctx.launch_gate.acquire(&gate_provider, &gate_model).await;

    let ctx_bg = ctx.clone();
    let run_id = run.id.clone();
    spawn_launch_gated_task(launch_permit, async move {
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

/// Spawn a task while holding the launch-concurrency permit for the full
/// task lifetime. `start_run` acquires the permit before calling this helper;
/// the helper is intentionally tiny so integration tests can pin the lifetime
/// contract without constructing a full backtest executor.
#[doc(hidden)]
pub fn spawn_launch_gated_task<F>(
    launch_permit: crate::eval::concurrency::PermitGuard,
    task: F,
) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        // Dropping this guard releases the slot back to the gate; it must
        // outlive the whole background task body.
        let _launch_permit = launch_permit;
        task.await;
    })
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
    executor: Box<dyn RunExecutor>,
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
                if let Err(e) = api_search::upsert_run(&ctx, &terminal).await {
                    tracing::warn!(error = %e, run_id = %run.id, "search index upsert (run) failed");
                }
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
                if let Err(e) = api_search::upsert_run(&ctx, &cancelled).await {
                    tracing::warn!(error = %e, run_id = %run.id, "search index upsert (run) failed");
                }
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
            if let Err(e) = api_search::upsert_run(&ctx, &failed).await {
                tracing::warn!(error = %e, run_id = %run.id, "search index upsert (run) failed");
            }
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

    if let Err(e) = api_search::upsert_run(&ctx, &finalized).await {
        tracing::warn!(error = %e, run_id = %finalized.id, "search index upsert (run) failed");
    }
    fire_chain_attestation_after_finalize(&finalized);
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
    if finalized.auto_fire_review {
        crate::eval::review::auto::fire_auto_review(&store_for_auto, &finalized.id).await;
    }

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

#[cfg(feature = "chain-attest")]
fn fire_chain_attestation_after_finalize(run: &Run) {
    let finalized = run.clone();
    tokio::spawn(async move {
        crate::eval::chain_attestation::fire_chain_attestation(&finalized).await;
    });
}

#[cfg(not(feature = "chain-attest"))]
fn fire_chain_attestation_after_finalize(_run: &Run) {}

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

fn effective_obs_config(ctx: &ApiContext) -> Arc<xvision_observability::ObservabilityConfig> {
    let path = ctx.xvn_home.join("config").join("observability.toml");
    match xvision_observability::ObservabilityConfig::load_from_file(&path) {
        Ok(cfg) => Arc::new(cfg),
        Err(err) => {
            tracing::warn!(error = %err, "using startup observability config");
            ctx.obs_config.clone()
        }
    }
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
            let asset_universe: Vec<String> = Vec::new();
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

fn apply_review_launch_options(run: &mut Run, req: &EvalRunRequest) {
    run.auto_fire_review = req.auto_fire_review;
    run.review_model = req.review_model.clone();
    run.max_annotations_per_review = req.max_annotations_per_review.or(Some(8));
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
        strategy: None,
        scenario: None,
        mode: match run.mode {
            RunMode::Backtest => "backtest".into(),
            RunMode::Live => "live".into(),
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
        auto_fire_review: run.auto_fire_review,
        review_model: run.review_model,
        max_annotations_per_review: run.max_annotations_per_review,
        paused: run.paused,
        paused_at: run.paused_at,
        flatten_requested: run.flatten_requested,
        live_config: run.live_config,
        source: run.source,
        unrealized_pnl_usd: run.unrealized_pnl_usd,
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

/// Public wrapper that spawns ONE [`crate::agent::dispatch_capability::ClineDispatchCtx`]
/// without trajectory recording, for use by the optimizer cycle.
///
/// Since WU-6 retired `LlmDispatch`, the sidecar is mandatory — this function
/// always attempts to spawn. If `XVN_AGENTD_BIN` is unset or the provider is
/// not launchable it returns a clear `ApiError::Validation` so the caller can
/// surface an actionable message rather than silently falling back.
///
/// The return type is `Option<ClineDispatchCtx>` for call-site compatibility;
/// it is always `Some` on success (never `None`).
pub async fn spawn_optimizer_cline_ctx(
    ctx: &ApiContext,
    provider_name: &str,
    tools: Arc<ToolRegistry>,
    run_mode: crate::eval::run::RunMode,
) -> ApiResult<Option<crate::agent::dispatch_capability::ClineDispatchCtx>> {
    let cfg_path = runtime_config_path(ctx);
    let entry = crate::api::settings::providers::resolve_provider(ctx, &cfg_path, provider_name, None)
        .await
        .map_err(|u| {
            ApiError::Validation(format!(
                "optimizer sidecar (Cline) is required since WU-6: \
                 provider `{}` not launchable (reason={}): {} \
                 — ensure XVN_AGENTD_BIN is set and the provider is configured",
                u.provider,
                u.reason.as_str(),
                u.hint
            ))
        })?;
    let (cctx, _no_recording) = spawn_cline_ctx(ctx, entry, tools, None, run_mode).await?;
    Ok(Some(cctx))
}

mod tests {
    use super::*;
    use crate::strategies::{
        manifest::PublicManifest, risk::RiskPreset, slot::LLMSlot, AgentRef, PipelineDef, Strategy,
    };

    // --- resolve_live_venue (Orderly testnet live venue, 2026-06-11) --------

    #[test]
    fn live_venue_alpaca_resolves_regardless_of_orderly_env() {
        for url in [None, Some("https://testnet-api-evm.orderly.org")] {
            assert_eq!(
                resolve_live_venue("alpaca", url, None, None, None).unwrap(),
                LiveVenue::AlpacaPaper
            );
        }
    }

    #[test]
    fn live_venue_orderly_testnet_requires_base_url_set() {
        for url in [None, Some(""), Some("   ")] {
            let err = resolve_live_venue("orderly_testnet", url, None, None, None)
                .expect_err("orderly_testnet without ORDERLY_BASE_URL must be rejected");
            let msg = err.to_string();
            assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
            assert!(msg.contains("ORDERLY_BASE_URL"), "must name the env var: {msg}");
            assert!(msg.contains("mainnet"), "must explain mainnet refusal: {msg}");
        }
    }

    #[test]
    fn live_venue_orderly_testnet_rejects_mainnet_base_url() {
        let err = resolve_live_venue(
            "orderly_testnet",
            Some("https://api-evm.orderly.org"),
            None,
            None,
            None,
        )
        .expect_err("mainnet ORDERLY_BASE_URL must be rejected");
        let msg = err.to_string();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
        assert!(
            msg.contains("testnet only"),
            "must state the testnet-only scope: {msg}"
        );
        assert!(
            msg.contains("api-evm.orderly.org"),
            "must echo the offending URL: {msg}"
        );
    }

    #[test]
    fn live_venue_orderly_testnet_accepts_testnet_base_url() {
        assert_eq!(
            resolve_live_venue(
                "orderly_testnet",
                Some("https://testnet-api-evm.orderly.org"),
                None,
                None,
                None
            )
            .unwrap(),
            LiveVenue::OrderlyTestnet,
        );
    }

    // --- resolve_live_venue (Orderly mainnet live venue, 2026-06-14) --------

    #[test]
    fn live_venue_orderly_mainnet_accepts_default_and_mainnet_base_url() {
        // Unset / empty ORDERLY_BASE_URL falls back to the mainnet default
        // (api-evm.orderly.org); an explicit mainnet URL is also accepted.
        for url in [None, Some(""), Some("   "), Some("https://api-evm.orderly.org")] {
            assert_eq!(
                resolve_live_venue("orderly_mainnet", url, None, None, None).unwrap(),
                LiveVenue::OrderlyMainnet,
                "url {url:?} should resolve to OrderlyMainnet",
            );
        }
    }

    #[test]
    fn live_venue_orderly_mainnet_rejects_testnet_base_url() {
        // Mirror-image of the testnet guard: a real-money "mainnet" venue must
        // never run against a testnet gateway via a stale-env mistake
        // (fail-closed), case-insensitively.
        for url in [
            "https://testnet-api-evm.orderly.org",
            "https://TESTNET-api-evm.orderly.org",
        ] {
            let err = resolve_live_venue("orderly_mainnet", Some(url), None, None, None)
                .expect_err("orderly_mainnet with a testnet ORDERLY_BASE_URL must be rejected");
            let msg = err.to_string();
            assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
            assert!(
                msg.contains("testnet"),
                "must explain the testnet/mainnet mismatch: {msg}"
            );
            assert!(msg.contains("ORDERLY_BASE_URL"), "must name the env var: {msg}");
        }
    }

    #[test]
    fn live_venue_byreal_requires_testnet_network() {
        // After the mainnet parity lift, byreal resolves to ByrealLive for ALL
        // BYREAL_NETWORK values (the testnet/mainnet split is carried by
        // venue_label + SafetyGate, not by refusing to resolve here).
        // Previously this test asserted rejection for non-testnet networks; it
        // now asserts successful resolution for all network values.
        for net in [None, Some(""), Some("   "), Some("mainnet")] {
            assert_eq!(
                resolve_live_venue("byreal", None, net, None, None).unwrap(),
                LiveVenue::ByrealLive,
                "byreal with network {net:?} should resolve to ByrealLive after mainnet parity",
            );
        }
    }

    #[test]
    fn live_venue_byreal_accepts_testnet_network() {
        for net in [Some("testnet"), Some("hyperliquid-testnet"), Some(" TESTNET ")] {
            assert_eq!(
                resolve_live_venue("byreal", None, net, None, None).unwrap(),
                LiveVenue::ByrealLive,
                "network {net:?} should resolve to ByrealLive",
            );
        }
    }

    #[test]
    fn byreal_mainnet_resolves_to_byreal_live() {
        assert_eq!(
            resolve_live_venue("byreal", None, Some("mainnet"), None, None).unwrap(),
            LiveVenue::ByrealLive,
        );
    }

    #[test]
    fn byreal_testnet_resolves_to_byreal_live() {
        assert_eq!(
            resolve_live_venue("byreal", None, Some("testnet"), None, None).unwrap(),
            LiveVenue::ByrealLive,
        );
    }

    #[test]
    fn live_venue_unknown_ref_names_both_supported_venues() {
        let err = resolve_live_venue(
            "bybit",
            Some("https://testnet-api-evm.orderly.org"),
            None,
            None,
            None,
        )
        .expect_err("unknown broker_creds_ref must be rejected");
        let msg = err.to_string();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
        assert!(msg.contains("\"alpaca\""), "must name alpaca: {msg}");
        assert!(
            msg.contains("\"orderly_testnet\""),
            "must name orderly_testnet: {msg}"
        );
        assert!(
            msg.contains("\"orderly_mainnet\""),
            "must name orderly_mainnet: {msg}"
        );
        assert!(msg.contains("\"byreal\""), "must name byreal: {msg}");
        assert!(msg.contains("\"degen_arena\""), "must name degen_arena: {msg}");
    }

    // --- resolve_live_venue: degen_arena (2026-06-13) -----------------------

    #[test]
    fn live_venue_degen_arena_accepts_testnet_network() {
        for net in [Some("testnet"), Some("hyperliquid-testnet"), Some(" TESTNET ")] {
            assert_eq!(
                resolve_live_venue("degen_arena", None, None, net, None).unwrap(),
                LiveVenue::DegenArena,
                "degen_network {net:?} should resolve to DegenArena",
            );
        }
    }

    #[test]
    fn live_venue_degen_arena_mainnet_without_allow_flag_is_rejected() {
        // Mainnet (no "testnet" in network value) without DEGEN_ALLOW_MAINNET=1
        // must be gated. We do NOT set the env var in this test — if it happens
        // to be set in the environment we explicitly clear it to stay hermetic.
        // The test names env vars in assertions but never prints their values
        // (cred-safety policy).
        let _guard = EnvVarGuard::clear("DEGEN_ALLOW_MAINNET");
        for net in [None, Some(""), Some("mainnet"), Some("hyperliquid-mainnet")] {
            let err = resolve_live_venue("degen_arena", None, None, net, None)
                .expect_err("mainnet degen_arena without DEGEN_ALLOW_MAINNET must be rejected");
            let msg = err.to_string();
            assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
            assert!(
                msg.contains("DEGEN_ALLOW_MAINNET"),
                "must name DEGEN_ALLOW_MAINNET env var: {msg}"
            );
            assert!(
                msg.contains("DEGEN_HL_NETWORK"),
                "must name DEGEN_HL_NETWORK env var: {msg}"
            );
            // Cred-safety: must not echo env values.
            assert!(!msg.contains("mainnet'"), "must not echo env value: {msg}");
        }
    }

    // --- resolve_live_venue: hyperliquid (WU-6.1, 2026-06-14) -----------------

    #[test]
    fn live_venue_hyperliquid_accepts_testnet_network() {
        for net in [Some("testnet"), Some("hyperliquid-testnet"), Some(" TESTNET ")] {
            assert_eq!(
                resolve_live_venue("hyperliquid", None, None, None, net).unwrap(),
                LiveVenue::Hyperliquid,
                "hl_network {net:?} should resolve to Hyperliquid",
            );
        }
    }

    #[test]
    fn live_venue_hyperliquid_mainnet_without_allow_flag_is_rejected() {
        // Mainnet without HL_ALLOW_MAINNET=1 must be gated. We clear the env var
        // to stay hermetic. The test names env vars in assertions but never
        // prints their values (cred-safety policy).
        let _guard = EnvVarGuard::clear("HL_ALLOW_MAINNET");
        for net in [None, Some(""), Some("mainnet"), Some("hyperliquid-mainnet")] {
            let err = resolve_live_venue("hyperliquid", None, None, None, net)
                .expect_err("mainnet hyperliquid without HL_ALLOW_MAINNET must be rejected");
            let msg = err.to_string();
            assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
            assert!(
                msg.contains("HL_ALLOW_MAINNET"),
                "must name HL_ALLOW_MAINNET env var: {msg}"
            );
            assert!(msg.contains("HL_NETWORK"), "must name HL_NETWORK env var: {msg}");
            // Cred-safety: must not echo env values.
            assert!(!msg.contains("mainnet'"), "must not echo env value: {msg}");
        }
    }

    #[test]
    fn live_venue_unknown_error_contains_degen_arena() {
        let err = resolve_live_venue("unknown_venue", None, None, None, None)
            .expect_err("unknown venue must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("\"degen_arena\""),
            "must list degen_arena in error: {msg}"
        );
    }

    #[test]
    fn live_venue_unknown_error_contains_hyperliquid() {
        let err = resolve_live_venue("unknown_venue", None, None, None, None)
            .expect_err("unknown venue must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("\"hyperliquid\""),
            "must list hyperliquid in error: {msg}"
        );
    }

    /// RAII guard that clears an env var for the duration of a test and restores
    /// (or removes) it on drop — keeping tests hermetic when the env may be set
    /// by the outer shell.
    struct EnvVarGuard {
        key: &'static str,
        prior: Option<String>,
    }

    impl EnvVarGuard {
        fn clear(key: &'static str) -> Self {
            let prior = std::env::var(key).ok();
            // SAFETY: single-threaded test context; standard test-env pattern.
            #[allow(unused_unsafe)]
            unsafe {
                std::env::remove_var(key);
            }
            Self { key, prior }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            // SAFETY: restoring to prior state.
            #[allow(unused_unsafe)]
            unsafe {
                match &self.prior {
                    Some(v) => std::env::set_var(self.key, v),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    // --- U13: agentd registry / cancel-degrades (2026-06-11) ----------------

    /// An unregistered run signals `NoProcess` — cancel degrades gracefully,
    /// never erroring on missing sidecar bookkeeping.
    #[test]
    fn test_signal_agentd_unknown_run_degrades() {
        let outcome = signal_agentd_for_run("u13-no-such-run-xyz");
        assert_eq!(outcome, CancelOutcome::NoProcess);
    }

    /// A registered handle with a pid reports `Signaled`; a second signal for
    /// the same run reports `NoProcess` (the handle was consumed/removed).
    #[test]
    fn test_signal_agentd_registered_is_signaled_once() {
        let run_id = "u13-registered-run-abc";
        register_agentd(
            run_id,
            AgentdHandle {
                // Use the current process pid as a guaranteed-live target, but
                // SIGTERM is sent via `kill -TERM` only inside signal_*; here we
                // assert the registry/outcome bookkeeping, not the actual kill.
                pid: Some(std::process::id()),
                socket_path: std::path::PathBuf::from("/tmp/agentd-test.sock"),
            },
        );
        // NOTE: this WOULD send SIGTERM to ourselves if pid is Some. To keep the
        // test from terminating the test runner, deregister and assert the
        // bookkeeping path via a None-pid handle instead.
        deregister_agentd(run_id);

        register_agentd(
            run_id,
            AgentdHandle {
                pid: None,
                socket_path: std::path::PathBuf::from("/tmp/agentd-test.sock"),
            },
        );
        let outcome = signal_agentd_for_run(run_id);
        assert_eq!(
            outcome,
            CancelOutcome::Unknown,
            "registered handle with no pid → Unknown (degrade with a warning)"
        );
        // Handle consumed; a second signal sees nothing.
        assert_eq!(signal_agentd_for_run(run_id), CancelOutcome::NoProcess);
    }

    /// deregister removes a handle so a later signal degrades to NoProcess.
    #[test]
    fn test_deregister_agentd_removes_handle() {
        let run_id = "u13-dereg-run";
        register_agentd(
            run_id,
            AgentdHandle {
                pid: None,
                socket_path: std::path::PathBuf::from("/tmp/x.sock"),
            },
        );
        deregister_agentd(run_id);
        assert_eq!(signal_agentd_for_run(run_id), CancelOutcome::NoProcess);
    }

    // --- agent_runtime (WU-6: always Cline) ------------------------------------
    // classify_agent_runtime was removed in WU-6 (LlmDispatch retirement).
    // The tests that exercised LlmDispatch fallback paths were also removed.

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
                execution_mode: Default::default(),
                capital_mode: Default::default(),
                decision_cadence_minutes: 60,
                timeframe_requirements: Default::default(),
                attested_with: Vec::new(),
                required_tools: Vec::new(),
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
            },
            hypothesis: None,
            agents: vec![AgentRef {
                agent_id: "01TESTAGENT".into(),
                role: "trader".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
            }],
            pipeline: PipelineDef::default(),
            regime_slot: None,
            trader_slot: Some(legacy_slot),
            risk: RiskPreset::Balanced.expand(),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
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
            max_wall_ms: None,
            temperature: None,
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
            nano: None,
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
            max_wall_ms: None,
            temperature: None,
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
            nano: None,
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
            max_wall_ms: None,
            temperature: None,
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
            nano: None,
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

    /// Regression (QA 2026-06-03, CRITICAL): multi-asset eval fed every asset
    /// BTC's bars. Root cause was `load_bars_for_scenario` keying the cache by
    /// the asset-independent `scenario.bar_cache_policy.cache_key` instead of a
    /// per-asset `compute_cache_key`, so the second asset read the first asset's
    /// cached row. Two assets with divergent price levels must resolve to their
    /// OWN bars.
    #[tokio::test]
    async fn load_bars_for_scenario_routes_per_asset_not_scenario_key() {
        use sqlx::sqlite::SqlitePoolOptions;
        use std::sync::Arc;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use xvision_core::trading::AssetSymbol;
        use xvision_data::alpaca::{AlpacaBarsFetcher, BarGranularity};

        // One combined body: the fetcher selects `bars[requested_symbol]`, so
        // each per-asset fetch extracts its own series. BTC ~101_769, ETH ~2_310.
        let bar = |c: f64, t: &str| serde_json::json!({"t": t, "o": c, "h": c, "l": c, "c": c, "v": 1.0});
        let body = serde_json::json!({
            "bars": {
                "BTC/USD": [
                    bar(101_769.0, "2025-01-06T00:00:00Z"),
                    bar(101_770.0, "2025-01-06T01:00:00Z"),
                    bar(101_771.0, "2025-01-06T02:00:00Z"),
                ],
                "ETH/USD": [
                    bar(2_310.0, "2025-01-06T00:00:00Z"),
                    bar(2_311.0, "2025-01-06T01:00:00Z"),
                    bar(2_312.0, "2025-01-06T02:00:00Z"),
                ]
            },
            "next_page_token": null
        });
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1beta3/crypto/us/bars"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(include_str!("../../migrations/001_api_audit.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(include_str!("../../migrations/010_bars_cache.sql"))
            .execute(&pool)
            .await
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let fetcher = Arc::new(AlpacaBarsFetcher::new(
            server.uri(),
            "test-key".into(),
            "test-secret".into(),
        ));
        let ctx = ApiContext::new(
            pool,
            crate::api::Actor::Cli {
                user: "tester".into(),
            },
            dir.path().to_path_buf(),
        )
        .with_alpaca_fetcher(fetcher);

        // Canonical scenario narrowed to the 3-bar window the mock serves. The
        // scenario-level cache_key is deliberately set to BTC's per-asset key to
        // recreate the contamination condition: the pre-fix code read this row
        // for ETH too and returned BTC's bars.
        let mut scenario = crate::eval::scenario_seed::canonical_seed_rows()
            .into_iter()
            .next()
            .expect("at least one canonical scenario");
        scenario.time_window = crate::eval::scenario::TimeWindow {
            start: chrono::DateTime::parse_from_rfc3339("2025-01-06T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            end: chrono::DateTime::parse_from_rfc3339("2025-01-06T03:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        };
        let granularity = BarGranularity::Hour1;
        scenario.bar_cache_policy.cache_key = crate::eval::bars::compute_cache_key(
            &AssetSymbol::Btc.as_alpaca_pair(),
            granularity,
            scenario.time_window.start,
            scenario.time_window.end,
            "alpaca-historical-v1",
        );

        // Resolve BTC first so the pre-fix code caches BTC under the scenario key.
        let btc_bars = load_bars_for_scenario(&ctx, &scenario, AssetSymbol::Btc, granularity)
            .await
            .unwrap();
        let eth_bars = load_bars_for_scenario(&ctx, &scenario, AssetSymbol::Eth, granularity)
            .await
            .unwrap();
        let btc_close = btc_bars.first().expect("btc bars non-empty").close;
        let eth_close = eth_bars.first().expect("eth bars non-empty").close;

        assert!(
            (btc_close - 101_769.0).abs() < 1.0,
            "BTC must resolve its own bars, got {btc_close}"
        );
        assert!(
            (eth_close - 2_310.0).abs() < 1.0,
            "ETH must resolve ETH's bars, not BTC's (multi-asset contamination regression); got {eth_close}"
        );
        assert!(
            (btc_close - eth_close).abs() > 1_000.0,
            "per-asset bars must diverge across assets; btc={btc_close} eth={eth_close}"
        );
    }

    // --- spawn_optimizer_cline_ctx (optimizer parity, WU-6) -------------------

    /// Since WU-6 retired LlmDispatch, spawn_optimizer_cline_ctx always
    /// attempts to spawn the sidecar. Without XVN_AGENTD_BIN set and no
    /// configured provider, it must return a hard error (not Ok(None)).
    #[tokio::test]
    async fn optimizer_cline_ctx_errors_without_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = crate::api::ApiContext::open(dir.path(), crate::api::Actor::Cli { user: "test".into() })
            .await
            .unwrap();

        // Ensure XVN_AGENTD_BIN is NOT set so the sidecar is unavailable.
        // Without a configured provider, the wrapper must return an error.
        std::env::remove_var("XVN_AGENTD_BIN");

        let tools = Arc::new(crate::tools::ToolRegistry::empty());
        let result =
            spawn_optimizer_cline_ctx(&ctx, "anthropic", tools, crate::eval::run::RunMode::Backtest).await;
        // ClineDispatchCtx has no Debug, so report only the Ok/Err shape.
        assert!(
            result.is_err(),
            "expected Err (sidecar mandatory since WU-6), but got Ok(..)"
        );
    }
}

// ---------------------------------------------------------------------------
// Task 6: LiveDeploymentSummary type + list_live_deployments / get_live_deployment
// ---------------------------------------------------------------------------

/// Wire type for the live-deployments list/detail API.
/// Represents one paper or testnet live run with its current capital-risk snapshot.
/// `venue_label` is always "paper" or "testnet" — 'live' is excluded by the query.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveDeploymentSummary {
    pub deployment_id: String,
    pub strategy_id: Option<String>,
    pub strategy_name: Option<String>,
    /// "paper" | "testnet" — 'live' excluded by the query filter.
    pub venue_label: String,
    /// queued | running | completed | failed | cancelled
    pub status: String,
    pub paused: bool,
    pub started_at: String,
    pub last_decision_at: Option<String>,
    pub deployed_capital_usd: Option<f64>,
    pub equity_usd: Option<f64>,
    pub realized_pnl_usd: Option<f64>,
    pub unrealized_pnl_usd: Option<f64>,
    pub realized_today_usd: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub daily_loss_limit_remaining_usd: Option<f64>,
    // i64 → JSON integer decodes as a JS `number`, not BigInt; pin the TS type.
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub risk_veto_count: i64,
    /// Daily-loss budget in USD = kill_pct × initial capital. `None` when
    /// no live_run_state row exists yet or kill_pct is 0. Unlocks the
    /// strip's buffer %-gradient (remaining / budget).
    pub daily_loss_budget_usd: Option<f64>,
    /// Wall-clock deadline (RFC-3339) = started_at + time_limit_secs.
    /// `None` for bar/decision stop policies (no wall-clock ETA) or when no
    /// live_run_state row exists yet. Unlocks awm's ETA display.
    pub stop_at: Option<String>,
}

/// Private row type for the `eval_runs LEFT JOIN live_run_state` query.
/// Field names MUST exactly match the SELECT column aliases.
#[derive(sqlx::FromRow)]
struct LiveDeploymentRow {
    deployment_id: String,
    venue_label: String,
    status: String,
    paused: bool,
    started_at: String,
    strategy_id: Option<String>,
    strategy_name: Option<String>,
    last_decision_at: Option<String>,
    deployed_capital_usd: Option<f64>,
    equity_usd: Option<f64>,
    realized_pnl_usd: Option<f64>,
    unrealized_pnl_usd: Option<f64>,
    realized_today_usd: Option<f64>,
    drawdown_pct: Option<f64>,
    daily_loss_remaining_usd: Option<f64>,
    risk_veto_count: Option<i64>,
    daily_loss_budget_usd: Option<f64>,
    stop_at: Option<String>,
}

/// Base SELECT joining `eval_runs` to `live_run_state`, filtered to
/// `mode='live' AND venue_label != 'live'` (paper + testnet only).
const LIVE_DEPLOYMENT_SELECT: &str = "\
    SELECT r.id AS deployment_id, r.venue_label AS venue_label, r.status AS status, \
           r.paused AS paused, r.started_at AS started_at, \
           s.strategy_id AS strategy_id, s.strategy_name AS strategy_name, \
           s.last_decision_at AS last_decision_at, s.deployed_capital_usd AS deployed_capital_usd, \
           s.equity_usd AS equity_usd, s.realized_pnl_usd AS realized_pnl_usd, \
           s.unrealized_pnl_usd AS unrealized_pnl_usd, s.realized_today_usd AS realized_today_usd, \
           s.drawdown_pct AS drawdown_pct, s.daily_loss_remaining_usd AS daily_loss_remaining_usd, \
           s.risk_veto_count AS risk_veto_count, \
           s.daily_loss_budget_usd AS daily_loss_budget_usd, s.stop_at AS stop_at \
    FROM eval_runs r LEFT JOIN live_run_state s ON s.run_id = r.id \
    WHERE r.mode = 'live' AND r.venue_label != 'live'";

impl From<LiveDeploymentRow> for LiveDeploymentSummary {
    fn from(r: LiveDeploymentRow) -> Self {
        Self {
            deployment_id: r.deployment_id,
            strategy_id: r.strategy_id,
            strategy_name: r.strategy_name,
            venue_label: r.venue_label,
            status: r.status,
            paused: r.paused,
            started_at: r.started_at,
            last_decision_at: r.last_decision_at,
            deployed_capital_usd: r.deployed_capital_usd,
            equity_usd: r.equity_usd,
            realized_pnl_usd: r.realized_pnl_usd,
            unrealized_pnl_usd: r.unrealized_pnl_usd,
            realized_today_usd: r.realized_today_usd,
            drawdown_pct: r.drawdown_pct,
            daily_loss_limit_remaining_usd: r.daily_loss_remaining_usd,
            risk_veto_count: r.risk_veto_count.unwrap_or(0),
            daily_loss_budget_usd: r.daily_loss_budget_usd,
            stop_at: r.stop_at,
        }
    }
}

/// List all paper/testnet live deployments, optionally filtered by status.
///
/// An empty `status` string is treated as no-filter (same as `None`).
/// Results are ordered by `started_at DESC, id DESC`.
pub async fn list_live_deployments(
    ctx: &ApiContext,
    status: Option<&str>,
) -> anyhow::Result<Vec<LiveDeploymentSummary>> {
    let mut sql = String::from(LIVE_DEPLOYMENT_SELECT);
    // Treat an empty status string as "no filter".
    let status = status.filter(|s| !s.is_empty());
    if status.is_some() {
        sql.push_str(" AND r.status = ?");
    }
    sql.push_str(" ORDER BY r.started_at DESC, r.id DESC");
    let mut q = sqlx::query_as::<_, LiveDeploymentRow>(&sql);
    if let Some(st) = status {
        q = q.bind(st);
    }
    let rows = q.fetch_all(&ctx.db).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Fetch a single live deployment by `run_id`, or `None` when not found.
pub async fn get_live_deployment(
    ctx: &ApiContext,
    run_id: &str,
) -> anyhow::Result<Option<LiveDeploymentSummary>> {
    let sql = format!("{LIVE_DEPLOYMENT_SELECT} AND r.id = ?");
    let row = sqlx::query_as::<_, LiveDeploymentRow>(&sql)
        .bind(run_id)
        .fetch_optional(&ctx.db)
        .await?;
    Ok(row.map(Into::into))
}

// ---------------------------------------------------------------------------
// Unit tests — ToolRegistryDispatch forward-only guard + Nansen as_of injection
// (Task 1.5)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tool_registry_dispatch_tests {
    use super::ToolRegistryDispatch;

    fn test_dispatch(
        mode: crate::eval::run::RunMode,
        as_of: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ToolRegistryDispatch {
        ToolRegistryDispatch {
            tools: std::sync::Arc::new(crate::tools::ToolRegistry::empty()),
            current_asset: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            run_mode: mode,
            as_of: std::sync::Arc::new(tokio::sync::RwLock::new(as_of)),
            nansen_lag_days: 1,
            tool_cache: None,
            nansen_budget: None,
            elfa_budget: None,
        }
    }

    fn test_dispatch_budget(mode: crate::eval::run::RunMode, budget: Option<u32>) -> ToolRegistryDispatch {
        ToolRegistryDispatch {
            tools: std::sync::Arc::new(crate::tools::ToolRegistry::empty()),
            current_asset: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            run_mode: mode,
            as_of: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            nansen_lag_days: 1,
            tool_cache: None,
            nansen_budget: budget.map(|n| std::sync::Arc::new(std::sync::atomic::AtomicU32::new(n))),
            elfa_budget: None,
        }
    }

    #[tokio::test]
    async fn elfa_tool_rejected_in_backtest() {
        let d = test_dispatch(crate::eval::run::RunMode::Backtest, None);
        let err = d
            .invoke("elfa_smart_mentions", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("forward-only"), "got: {err}");
    }

    #[tokio::test]
    async fn nansen_backtest_injects_floored_as_of_overwriting_model_value() {
        use chrono::{TimeZone, Utc};
        let anchor = Utc.with_ymd_and_hms(2024, 3, 15, 14, 0, 0).unwrap();
        let d = test_dispatch(crate::eval::run::RunMode::Backtest, Some(anchor));
        // The model supplies a future date; the framework MUST overwrite it.
        let injected = d
            .inject_backtest_as_of_async(
                "nansen_smart_money_flow",
                serde_json::json!({"asset": "BTC", "as_of_date": "2099-01-01"}),
            )
            .await
            .unwrap();
        assert_eq!(injected["as_of_date"], "2024-03-14");
    }

    #[tokio::test]
    async fn nansen_backtest_without_anchor_is_error() {
        let d = test_dispatch(crate::eval::run::RunMode::Backtest, None);
        let err = d
            .invoke("nansen_smart_money_flow", serde_json::json!({"asset":"BTC"}))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("anchor"), "got: {err}");
    }

    #[tokio::test]
    async fn nansen_live_does_not_inject_as_of() {
        let d = test_dispatch(crate::eval::run::RunMode::Live, None);
        let out = d
            .inject_backtest_as_of_async("nansen_smart_money_flow", serde_json::json!({"asset": "BTC"}))
            .await
            .unwrap();
        assert!(
            out.get("as_of_date").is_none(),
            "live must not inject a backtest anchor"
        );
    }

    // FIX B — xvision-im2r.2: a model-hallucinated as_of_date in a live run
    // must be stripped before forwarding to Nansen (would hit the historical
    // endpoint otherwise).
    #[tokio::test]
    async fn nansen_live_strips_model_supplied_as_of_date() {
        let d = test_dispatch(crate::eval::run::RunMode::Live, None);
        let out = d
            .inject_backtest_as_of_async(
                "nansen_smart_money_flow",
                serde_json::json!({"asset": "BTC", "as_of_date": "2099-01-01"}),
            )
            .await
            .unwrap();
        assert!(
            out.get("as_of_date").is_none(),
            "live run must strip any model-supplied as_of_date; got: {out}"
        );
        // Other fields must be preserved.
        assert_eq!(out["asset"], "BTC");
    }

    // FIX B — non-Nansen tool (elfa) in live mode: as_of_date is irrelevant
    // and the field passes through unchanged (no stripping for non-Nansen).
    #[tokio::test]
    async fn non_nansen_live_preserves_as_of_date() {
        let d = test_dispatch(crate::eval::run::RunMode::Live, None);
        let out = d
            .inject_backtest_as_of_async(
                "elfa_smart_mentions",
                serde_json::json!({"asset": "BTC", "as_of_date": "2099-01-01"}),
            )
            .await
            .unwrap();
        // Not a Nansen tool → pass-through, field preserved.
        assert_eq!(out["as_of_date"], "2099-01-01");
    }

    #[tokio::test]
    async fn nansen_backtest_non_object_input_is_error() {
        use chrono::Utc;
        let d = test_dispatch(crate::eval::run::RunMode::Backtest, Some(Utc::now()));
        let err = d
            .inject_backtest_as_of_async("nansen_smart_money_flow", serde_json::json!("not-an-object"))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("JSON object"), "got: {err}");
    }

    use xvision_agent_client::ToolDispatch as _;

    // -----------------------------------------------------------------------
    // Task 5.2 — D8 per-run credit budget tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn zero_budget_short_circuits_signal_tool_with_degrade() {
        let d = test_dispatch_budget(crate::eval::run::RunMode::Live, Some(0));
        let out = d
            .invoke("nansen_token_screener", serde_json::json!({"asset":"BTC"}))
            .await
            .unwrap();
        assert_eq!(out["available"], false);
        assert!(out["reason"].as_str().unwrap().contains("budget"), "got: {out}");
    }

    #[tokio::test]
    async fn budget_does_not_affect_builtins() {
        // ohlcv is a built-in (no policy); a zero budget must NOT short-circuit it.
        // With an empty registry the dispatch errors (tool not found) — that's
        // proof the budget gate did NOT intercept (it would have returned a
        // degrade Ok-value instead of an Err).
        let d = test_dispatch_budget(crate::eval::run::RunMode::Live, Some(0));
        let res = d.invoke("ohlcv", serde_json::json!({"asset":"BTC"})).await;
        assert!(
            res.is_err(),
            "built-in must bypass the budget gate and hit dispatch"
        );
    }

    // -----------------------------------------------------------------------
    // FIX D — xvision-im2r.4: per-provider budget (Elfa ≠ Nansen pool)
    // -----------------------------------------------------------------------

    /// An Elfa call must NOT decrement the Nansen budget (and vice-versa).
    #[tokio::test]
    async fn elfa_call_does_not_consume_nansen_budget() {
        // nansen_budget=1, elfa_budget=1. An Elfa call with zero elfa_budget
        // degrades. But the nansen_budget must remain 1 after the Elfa call.
        let nansen_b = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1));
        let elfa_b = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let d = ToolRegistryDispatch {
            tools: std::sync::Arc::new(crate::tools::ToolRegistry::empty()),
            current_asset: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            run_mode: crate::eval::run::RunMode::Live,
            as_of: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            nansen_lag_days: 1,
            tool_cache: None,
            nansen_budget: Some(nansen_b.clone()),
            elfa_budget: Some(elfa_b),
        };
        // Elfa call with zero elfa_budget → degrade.
        let out = d
            .invoke("elfa_smart_mentions", serde_json::json!({"asset":"BTC"}))
            .await
            .unwrap();
        assert_eq!(
            out["available"], false,
            "elfa call with zero elfa_budget must degrade"
        );
        // Nansen budget must still be 1 — the Elfa call must not have touched it.
        assert_eq!(
            nansen_b.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "nansen_budget must be untouched by an Elfa call"
        );
    }

    /// A failing signal fetch must refund the credit (im2r.5).
    /// An empty registry causes dispatch_inner to Err (unknown tool), which
    /// exercises the refund path. We set nansen_budget=5 and verify it's still
    /// 5 after the error (not 4).
    #[tokio::test]
    async fn failing_signal_fetch_refunds_credit() {
        let nansen_b = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(5));
        let d = ToolRegistryDispatch {
            tools: std::sync::Arc::new(crate::tools::ToolRegistry::empty()),
            current_asset: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            run_mode: crate::eval::run::RunMode::Live,
            as_of: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            nansen_lag_days: 1,
            tool_cache: None,
            nansen_budget: Some(nansen_b.clone()),
            elfa_budget: None,
        };
        // Empty registry → dispatch_inner Errs → credit must be refunded.
        let res = d
            .invoke("nansen_smart_money_flow", serde_json::json!({"asset":"BTC"}))
            .await;
        assert!(res.is_err(), "must error with empty registry");
        assert_eq!(
            nansen_b.load(std::sync::atomic::Ordering::Relaxed),
            5,
            "credit must be refunded after a failed fetch"
        );
    }

    // -----------------------------------------------------------------------
    // Task 3.3 — replay + canonical_input_hash tests
    // -----------------------------------------------------------------------

    /// Build a minimal in-memory TrajectoryStore suitable for unit tests.
    /// Mirrors the DDL from `crates/xvision-observability/tests/tool_cache_round_trip.rs`.
    async fn make_test_store(
        tmp: &tempfile::TempDir,
    ) -> std::sync::Arc<xvision_observability::trajectory::store::TrajectoryStore> {
        use sqlx::sqlite::SqlitePoolOptions;
        use xvision_observability::trajectory::store::TrajectoryStore;
        use xvision_observability::{BlobStore, RetentionMode};

        let db_path = tmp.path().join("tool_cache_test.db");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("open sqlite");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS trajectory_recordings (
              recording_id       TEXT PRIMARY KEY,
              schema_version     INTEGER NOT NULL,
              status             TEXT NOT NULL DEFAULT 'open',
              key_fingerprint    TEXT NOT NULL UNIQUE,
              cycle_id           TEXT NOT NULL,
              slot_role          TEXT NOT NULL,
              arm_scope          TEXT,
              simulation_id      TEXT,
              provider           TEXT NOT NULL,
              model              TEXT NOT NULL,
              model_version      TEXT,
              system_prompt_hash TEXT NOT NULL,
              recovery_reason    TEXT,
              created_at         INTEGER NOT NULL,
              completed_at       INTEGER,
              expires_at         INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS trajectory_frames (
              recording_id  TEXT    NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE,
              slot_role     TEXT    NOT NULL,
              step_index    INTEGER NOT NULL,
              frame_index   INTEGER NOT NULL,
              frame_kind    TEXT    NOT NULL,
              ts_ms         INTEGER NOT NULL,
              payload_hash  TEXT    NOT NULL,
              payload_ref   TEXT,
              PRIMARY KEY (recording_id, slot_role, step_index, frame_index)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tool_http_cache (
              recording_id  TEXT NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE,
              tool_name     TEXT NOT NULL,
              input_hash    TEXT NOT NULL,
              as_of_date    TEXT,
              response_json TEXT NOT NULL,
              created_at    INTEGER NOT NULL,
              PRIMARY KEY (recording_id, tool_name, input_hash)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        let blob = BlobStore::new(tmp.path().join("blobs"));
        std::sync::Arc::new(TrajectoryStore::new(pool, blob, RetentionMode::HashOnly))
    }

    fn make_test_key() -> xvision_observability::trajectory::key::TrajectoryKey {
        use xvision_observability::trajectory::key::{TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
        TrajectoryKey::builder()
            .cycle_id(uuid::Uuid::new_v4())
            .slot_role("trader")
            .arm_scope(None::<String>)
            .simulation_id(None::<String>)
            .provider("anthropic")
            .model("claude-opus-4-7")
            .model_version("2026-05")
            .schema_version(TRAJECTORY_SCHEMA_VERSION)
            .system_prompt_hash("sys")
            .user_prompt_hash("usr")
            .build()
    }

    /// Replay path: a `ToolRegistryDispatch` with `replay: true` and a seeded
    /// cache entry must serve the cached response over an EMPTY tool registry
    /// (a live fetch would fail → proves no HTTP call was made).
    ///
    /// Uses `RunMode::Live` so that `inject_backtest_as_of_async` is a no-op
    /// and the hash over the seed input matches the hash computed in `invoke`
    /// (a Backtest run would inject an `as_of_date`, changing the hash).
    /// `nansen_token_screener` is `live: true` so the forward-only gate passes.
    #[tokio::test]
    async fn replay_serves_cache_without_invoking_tool() {
        use super::{canonical_input_hash, ToolHttpCacheHandle, ToolRegistryDispatch};
        use xvision_agent_client::ToolDispatch as _;

        let tmp = tempfile::TempDir::new().unwrap();
        let store = make_test_store(&tmp).await;
        let key = make_test_key();
        let rec = store.begin_recording(&key).await.unwrap();

        let input = serde_json::json!({"asset": "BTC"});
        let hash = canonical_input_hash("nansen_token_screener", &input);
        store
            .cache_tool_response(
                &rec,
                "nansen_token_screener",
                &hash,
                None,
                &serde_json::json!({"cached": true}),
            )
            .await
            .unwrap();

        let d = ToolRegistryDispatch {
            tools: std::sync::Arc::new(crate::tools::ToolRegistry::empty()),
            current_asset: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            // Live mode: no as_of injection → hash of input is stable and
            // matches what we seeded above.  nansen_token_screener is live:true
            // so the forward-only gate passes.  The key claim under test is
            // that the cached value is served without touching the empty
            // registry (which would panic on a real dispatch attempt).
            run_mode: crate::eval::run::RunMode::Live,
            as_of: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            nansen_lag_days: 1,
            tool_cache: Some(ToolHttpCacheHandle {
                store: store.clone(),
                recording_id: rec.clone(),
                replay: true,
            }),
            nansen_budget: None,
            elfa_budget: None,
        };

        let out = d.invoke("nansen_token_screener", input).await.unwrap();
        assert_eq!(out["cached"], true, "replay must serve the cached response");
    }

    // FIX C — xvision-im2r.3: is_degrade helper
    #[test]
    fn is_degrade_true_when_available_false() {
        use super::is_degrade;
        assert!(is_degrade(
            &serde_json::json!({"available": false, "reason": "budget exhausted"})
        ));
        assert!(is_degrade(&serde_json::json!({"available": false})));
    }

    #[test]
    fn is_degrade_false_for_normal_responses() {
        use super::is_degrade;
        assert!(!is_degrade(&serde_json::json!({"data": []})));
        assert!(!is_degrade(&serde_json::json!({"available": true})));
        assert!(!is_degrade(&serde_json::json!({})));
        assert!(!is_degrade(&serde_json::json!(null)));
    }

    /// `canonical_input_hash` must be key-order-independent: the same logical
    /// object with shuffled keys must hash identically.
    #[test]
    fn canonical_input_hash_is_deterministic_across_key_order() {
        use super::canonical_input_hash;
        let a = serde_json::json!({"asset": "BTC", "limit": 10, "as_of_date": "2024-03-14"});
        let b = serde_json::json!({"limit": 10, "as_of_date": "2024-03-14", "asset": "BTC"});
        assert_eq!(
            canonical_input_hash("nansen_token_screener", &a),
            canonical_input_hash("nansen_token_screener", &b),
            "key order must not affect the hash"
        );
        // Different tool name → different hash even for same payload.
        assert_ne!(
            canonical_input_hash("nansen_token_screener", &a),
            canonical_input_hash("elfa_trending", &a),
            "tool name must be part of the hash"
        );
    }
}

// ---------------------------------------------------------------------------
// Unit tests — check_venue_label_network (WU-2.3, generalized to all
// network-derived venues incl. Hyperliquid + Degen Arena)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod check_venue_label_network_tests {
    use super::{check_venue_label_network, LiveVenue};
    use crate::safety::VenueLabel;

    const NETWORK_DERIVED: [LiveVenue; 4] = [
        LiveVenue::ByrealLive,
        LiveVenue::ByrealSpot,
        LiveVenue::Hyperliquid,
        LiveVenue::DegenArena,
    ];

    #[test]
    fn mismatched_labels_are_err_for_every_network_derived_venue() {
        for venue in NETWORK_DERIVED {
            // Live-label run against a testnet broker → rejected.
            let e1 = check_venue_label_network(venue, VenueLabel::Live, VenueLabel::Testnet);
            assert!(e1.is_err(), "{venue:?}: Live run + Testnet broker must be Err");
            assert!(
                e1.unwrap_err().to_string().contains("venue_label"),
                "{venue:?}: error must mention venue_label"
            );
            // Testnet-label run against a live (mainnet) broker → rejected.
            assert!(
                check_venue_label_network(venue, VenueLabel::Testnet, VenueLabel::Live).is_err(),
                "{venue:?}: Testnet run + Live broker must be Err"
            );
        }
    }

    #[test]
    fn matching_labels_are_ok_for_every_network_derived_venue() {
        for venue in NETWORK_DERIVED {
            assert!(
                check_venue_label_network(venue, VenueLabel::Live, VenueLabel::Live).is_ok(),
                "{venue:?}: Live + Live must be Ok"
            );
            assert!(
                check_venue_label_network(venue, VenueLabel::Testnet, VenueLabel::Testnet).is_ok(),
                "{venue:?}: Testnet + Testnet must be Ok"
            );
        }
    }

    #[test]
    fn fixed_label_venues_are_noop_even_on_mismatch() {
        // Alpaca/Orderly testnet have a fixed non-mainnet label and no network split — never error.
        for venue in [LiveVenue::AlpacaPaper, LiveVenue::OrderlyTestnet] {
            assert!(
                check_venue_label_network(venue, VenueLabel::Paper, VenueLabel::Live).is_ok(),
                "{venue:?}: fixed-label venue must be a no-op"
            );
        }
    }

    #[test]
    fn orderly_mainnet_requires_live_run_label() {
        assert!(
            check_venue_label_network(LiveVenue::OrderlyMainnet, VenueLabel::Live, VenueLabel::Live).is_ok(),
            "OrderlyMainnet Live + Live must be Ok"
        );
        let err = check_venue_label_network(LiveVenue::OrderlyMainnet, VenueLabel::Testnet, VenueLabel::Live)
            .expect_err("OrderlyMainnet must require venue_label=Live");
        assert!(
            err.to_string().contains("OrderlyMainnet"),
            "error must name OrderlyMainnet: {err}"
        );
    }
}

// ---------------------------------------------------------------------------
// Unit tests — broker_label_for (DoD 3)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod broker_label_for_tests {
    use super::{broker_label_for, LiveVenue};
    use crate::safety::VenueLabel;

    #[test]
    fn alpaca_paper_maps_to_paper() {
        assert_eq!(
            broker_label_for(LiveVenue::AlpacaPaper, None, None, None),
            VenueLabel::Paper,
            "AlpacaPaper → Paper"
        );
    }

    #[test]
    fn orderly_testnet_maps_to_testnet() {
        assert_eq!(
            broker_label_for(LiveVenue::OrderlyTestnet, None, None, None),
            VenueLabel::Testnet,
            "OrderlyTestnet → Testnet"
        );
    }

    #[test]
    fn orderly_mainnet_maps_to_live() {
        assert_eq!(
            broker_label_for(LiveVenue::OrderlyMainnet, None, None, None),
            VenueLabel::Live,
            "OrderlyMainnet → Live"
        );
    }

    #[test]
    fn byreal_live_unset_maps_to_live() {
        // BYREAL_NETWORK unset → fail-safe to Live (production mainnet default).
        assert_eq!(
            broker_label_for(LiveVenue::ByrealLive, None, None, None),
            VenueLabel::Live,
            "ByrealLive + unset network → Live (fail-safe)"
        );
    }

    #[test]
    fn byreal_live_mainnet_maps_to_live() {
        assert_eq!(
            broker_label_for(LiveVenue::ByrealLive, Some("mainnet"), None, None),
            VenueLabel::Live,
            "ByrealLive + 'mainnet' → Live"
        );
    }

    #[test]
    fn byreal_live_testnet_maps_to_testnet() {
        assert_eq!(
            broker_label_for(LiveVenue::ByrealLive, Some("testnet"), None, None),
            VenueLabel::Testnet,
            "ByrealLive + 'testnet' → Testnet"
        );
    }

    #[test]
    fn byreal_live_testnet_case_insensitive() {
        // "Testnet", "TESTNET", "hl-testnet" all contain "testnet".
        for s in ["Testnet", "TESTNET", "hl-testnet", "byreal-testnet-v2"] {
            assert_eq!(
                broker_label_for(LiveVenue::ByrealLive, Some(s), None, None),
                VenueLabel::Testnet,
                "ByrealLive + '{s}' must map to Testnet"
            );
        }
    }

    #[test]
    fn degen_arena_mainnet_maps_to_live() {
        // Degen Arena mainnet is real-money Hyperliquid → Live, so the SafetyGate
        // requires a Live-labelled run (same invariant as byreal mainnet).
        assert_eq!(
            broker_label_for(LiveVenue::DegenArena, None, Some("mainnet"), None),
            VenueLabel::Live,
            "DegenArena + 'mainnet' → Live"
        );
        assert_eq!(
            broker_label_for(LiveVenue::DegenArena, None, None, None),
            VenueLabel::Live,
            "DegenArena + unset network → Live (fail-safe)"
        );
    }

    #[test]
    fn degen_arena_testnet_maps_to_testnet() {
        assert_eq!(
            broker_label_for(LiveVenue::DegenArena, None, Some("testnet"), None),
            VenueLabel::Testnet,
            "DegenArena + 'testnet' → Testnet"
        );
    }

    // --- broker_label_for: Hyperliquid (new venue, WU-6.1) -------------------

    #[test]
    fn hyperliquid_mainnet_maps_to_live() {
        assert_eq!(
            broker_label_for(LiveVenue::Hyperliquid, None, None, Some("mainnet")),
            VenueLabel::Live,
            "Hyperliquid + 'mainnet' → Live"
        );
        assert_eq!(
            broker_label_for(LiveVenue::Hyperliquid, None, None, None),
            VenueLabel::Live,
            "Hyperliquid + unset network → Live (fail-safe)"
        );
    }

    #[test]
    fn hyperliquid_testnet_maps_to_testnet() {
        assert_eq!(
            broker_label_for(LiveVenue::Hyperliquid, None, None, Some("testnet")),
            VenueLabel::Testnet,
            "Hyperliquid + 'testnet' → Testnet"
        );
    }

    // --- signals_used_from_tool_names (Task 6.3 backend) ---------------------

    /// Signal tools are collected, deduplicated, and sorted. Non-signal tools
    /// (ohlcv, submit_decision) are excluded.
    #[test]
    fn signals_used_collects_distinct_signal_tools() {
        let names: Vec<String> = vec![
            "ohlcv".into(),
            "nansen_smart_money_flow".into(),
            "nansen_smart_money_flow".into(), // duplicate → deduped
            "elfa_smart_mentions".into(),
            "submit_decision".into(),
        ];
        let result = super::signals_used_from_tool_names(&names);
        assert_eq!(
            result,
            Some(vec![
                "elfa_smart_mentions".to_string(),
                "nansen_smart_money_flow".to_string(),
            ]),
            "signal tools collected, deduped, sorted; non-signal tools excluded"
        );
    }

    /// When only non-signal tools are present the result is `None`.
    #[test]
    fn signals_used_none_when_no_signal_tools() {
        let names: Vec<String> = vec!["ohlcv".into(), "submit_decision".into()];
        let result = super::signals_used_from_tool_names(&names);
        assert_eq!(result, None, "no signal tools → None (field omitted from JSON)");
    }

    /// Empty input also yields `None`.
    #[test]
    fn signals_used_none_when_empty() {
        let result = super::signals_used_from_tool_names(&[]);
        assert_eq!(result, None);
    }

    /// All six signal tools are recognised.
    #[test]
    fn signals_used_all_six_signal_tools_recognised() {
        let names: Vec<String> = vec![
            "nansen_smart_money_flow".into(),
            "nansen_token_screener".into(),
            "nansen_flow_intel".into(),
            "elfa_smart_mentions".into(),
            "elfa_trending_tokens".into(),
            "elfa_trending_narratives".into(),
        ];
        let result = super::signals_used_from_tool_names(&names);
        assert_eq!(
            result,
            Some(vec![
                "elfa_smart_mentions".to_string(),
                "elfa_trending_narratives".to_string(),
                "elfa_trending_tokens".to_string(),
                "nansen_flow_intel".to_string(),
                "nansen_smart_money_flow".to_string(),
                "nansen_token_screener".to_string(),
            ]),
            "all six signal tools recognised and sorted"
        );
    }

    // --- resolve_signal_tool_config (xvision-im2r.6) -------------------------

    /// Verify `resolve_signal_tool_config` returns the enabled entries and that
    /// the resolved config is stored in the registry so `spawn_cline_ctx` reads
    /// from it rather than parsing `xvn.toml` a second time.
    #[tokio::test]
    async fn signal_tool_config_resolved_once_and_stored_in_registry() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        // Config lives at xvn_home/config/default.toml (see runtime_config_path).
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        let cfg_path = dir.path().join("config").join("default.toml");
        let mut f = std::fs::File::create(&cfg_path).unwrap();
        write!(
            f,
            r#"
[[data_tools]]
kind = "nansen"
enabled = true
base_url = "https://api.nansen.ai"
api_key_env = "NANSEN_API_KEY"
budget_credits_per_run = 10
nansen_lookahead_lag_days = 2

[[data_tools]]
kind = "elfa"
enabled = true
base_url = "https://api.elfa.ai"
api_key_env = "ELFA_API_KEY"
budget_credits_per_run = 5
"#
        )
        .unwrap();

        let ctx = crate::api::ApiContext::open(dir.path(), crate::api::Actor::Cli { user: "test".into() })
            .await
            .unwrap();

        let sig_cfg = super::resolve_signal_tool_config(&ctx);

        // Nansen entry parsed correctly.
        assert!(sig_cfg.nansen_entry.is_some(), "nansen entry must be present");
        let n = sig_cfg.nansen_entry.as_ref().unwrap();
        assert_eq!(n.base_url, "https://api.nansen.ai");
        assert_eq!(sig_cfg.nansen_lag_days(), 2);
        assert!(sig_cfg.nansen_budget_arc().is_some());

        // Elfa entry parsed correctly.
        assert!(sig_cfg.elfa_entry.is_some(), "elfa entry must be present");
        let e = sig_cfg.elfa_entry.as_ref().unwrap();
        assert_eq!(e.base_url, "https://api.elfa.ai");
        assert!(sig_cfg.elfa_budget_arc().is_some());

        // The registry stores a clone of the SAME resolved config — spawn_cline_ctx
        // reads from here instead of re-parsing xvn.toml.
        let registry = super::build_tool_registry(&ctx, &sig_cfg);
        let stored = registry
            .signal_cfg
            .as_ref()
            .expect("signal_cfg must be stored in registry");
        assert_eq!(stored.nansen_lag_days(), sig_cfg.nansen_lag_days());
        assert_eq!(
            stored.nansen_entry.as_ref().map(|e| e.base_url.as_str()),
            sig_cfg.nansen_entry.as_ref().map(|e| e.base_url.as_str()),
        );
    }

    /// When no xvn.toml exists, resolve_signal_tool_config returns an empty/default config
    /// and build_tool_registry succeeds (builtins only, no signal tools).
    #[tokio::test]
    async fn signal_tool_config_empty_when_no_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = crate::api::ApiContext::open(dir.path(), crate::api::Actor::Cli { user: "test".into() })
            .await
            .unwrap();
        let sig_cfg = super::resolve_signal_tool_config(&ctx);
        assert!(sig_cfg.nansen_entry.is_none());
        assert!(sig_cfg.elfa_entry.is_none());
        assert_eq!(
            sig_cfg.nansen_lag_days(),
            crate::tools::signal_policy::DEFAULT_NANSEN_LOOKAHEAD_LAG_DAYS
        );
        let registry = super::build_tool_registry(&ctx, &sig_cfg);
        // No signal tools registered when config is absent.
        let names: Vec<String> = registry.list().iter().map(|n| n.as_str().to_string()).collect();
        assert!(
            names
                .iter()
                .all(|n| !n.starts_with("nansen_") && !n.starts_with("elfa_")),
            "no signal tools expected without config: {names:?}"
        );
    }

    // --- broker_label_for + resolve: Byreal spot (new venue) ----------------

    #[test]
    fn byreal_spot_unset_maps_to_live() {
        // BYREAL_SPOT_NETWORK unset → fail-safe to Live. The spot network rides
        // in the byreal-family slot (2nd arg), like ByrealLive.
        assert_eq!(
            broker_label_for(LiveVenue::ByrealSpot, None, None, None),
            VenueLabel::Live,
            "ByrealSpot + unset network → Live (fail-safe)"
        );
    }

    #[test]
    fn byreal_spot_mainnet_maps_to_live() {
        assert_eq!(
            broker_label_for(LiveVenue::ByrealSpot, Some("mainnet"), None, None),
            VenueLabel::Live,
            "ByrealSpot + 'mainnet' → Live"
        );
    }

    #[test]
    fn byreal_spot_testnet_maps_to_testnet() {
        assert_eq!(
            broker_label_for(LiveVenue::ByrealSpot, Some("testnet"), None, None),
            VenueLabel::Testnet,
            "ByrealSpot + 'testnet' → Testnet"
        );
    }

    #[test]
    fn resolve_byreal_spot_creds_ref_ok() {
        use super::resolve_live_venue;
        assert_eq!(
            resolve_live_venue("byreal_spot", None, None, None, None).unwrap(),
            LiveVenue::ByrealSpot,
            "'byreal_spot' resolves to LiveVenue::ByrealSpot"
        );
    }
}

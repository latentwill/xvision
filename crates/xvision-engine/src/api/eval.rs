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

use crate::agent::llm::{AnthropicDispatch, LlmDispatch};
use crate::api::audit::{self, Outcome};
use crate::api::settings::brokers as api_brokers;
use crate::api::{search as api_search, strategy as api_strategy, ApiContext, ApiError, ApiResult};
use crate::eval::attestation::{self, EvalAttestation};
use crate::eval::compare::{compare_runs, ComparisonReport};
use crate::eval::executor::{BacktestExecutor, Executor, PaperExecutor};
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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CompareRunsRequest {
    /// Two-or-more run ids to fold into a single `ComparisonReport`.
    pub run_ids: Vec<String>,
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
pub async fn compare(
    ctx: &ApiContext,
    req: CompareRunsRequest,
) -> ApiResult<ComparisonReport> {
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

async fn compare_inner(
    ctx: &ApiContext,
    req: &CompareRunsRequest,
) -> ApiResult<ComparisonReport> {
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
    compare_runs(&req.run_ids, &store).await.map_err(|e| {
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
    /// Run mode. `Paper` drives an `AlpacaPaperSurface` against real Alpaca
    /// paper credentials; `Backtest` replays the scenario's parquet fixture
    /// in-process without any broker.
    pub mode: RunMode,
    /// Optional per-run override of bundle.mechanical_params. Persisted as
    /// `eval_runs.params_override_json`.
    pub params_override: Option<serde_json::Value>,
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
    // Early NotFound surfaces without env-var noise.
    let _bundle = api_strategy::get(ctx, &req.agent_id).await?;
    if !canonical_scenarios().iter().any(|s| s.id == req.scenario_id) {
        return Err(ApiError::NotFound(format!(
            "scenario '{}'",
            req.scenario_id
        )));
    }

    let broker: Option<Arc<dyn BrokerSurface>> = match req.mode {
        RunMode::Paper => Some(build_alpaca_paper_broker(ctx).await?),
        RunMode::Backtest => None,
    };
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        ApiError::Validation("ANTHROPIC_API_KEY env var is required".into())
    })?;
    let dispatch_arc: Arc<dyn LlmDispatch> = Arc::new(AnthropicDispatch::new(api_key));
    let tools_arc = Arc::new(ToolRegistry::default_with_builtins());
    run_with_deps(ctx, req, broker, dispatch_arc, tools_arc).await
}

/// Build an Alpaca paper broker, preferring credentials stored via the
/// settings UI (`$XVN_HOME/secrets/brokers.toml`) over `APCA_*` env
/// vars. Env-var fallback keeps CI scripts working without migration.
/// Returns `ApiError::Validation` with a user-actionable message if
/// neither source has credentials — the dashboard wires this into
/// "Configure Alpaca → Settings" copy.
async fn build_alpaca_paper_broker(
    ctx: &ApiContext,
) -> ApiResult<Arc<dyn BrokerSurface>> {
    const DEFAULT_PAPER_URL: &str = "https://paper-api.alpaca.markets";
    if let Some(creds) = api_brokers::load_alpaca_credentials(&ctx.xvn_home).await? {
        let base = creds
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_PAPER_URL);
        return AlpacaPaperSurface::from_credentials(
            &creds.api_key_id,
            &creds.api_secret_key,
            base,
        )
        .map(|s| Arc::new(s) as Arc<dyn BrokerSurface>)
        .map_err(|e| {
            ApiError::Internal(format!("alpaca paper from stored creds: {e}"))
        });
    }
    // Env-var fallback.
    match AlpacaPaperSurface::from_env() {
        Ok(s) => Ok(Arc::new(s)),
        Err(e) => {
            let msg = e.to_string();
            // Missing env vars is operator-actionable; bubble the
            // "where to set" hint into the validation message.
            if msg.contains("APCA_API_KEY_ID") || msg.contains("APCA_API_SECRET_KEY") {
                Err(ApiError::Validation(format!(
                    "Alpaca paper credentials not configured. Set them in Settings → Brokers, or export APCA_API_KEY_ID + APCA_API_SECRET_KEY before running."
                )))
            } else {
                Err(ApiError::Internal(format!("alpaca paper from env: {e}")))
            }
        }
    }
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
    broker: Option<Arc<dyn BrokerSurface>>,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
) -> ApiResult<Run> {
    // 1. Look up the strategy bundle. Propagates ApiError::NotFound cleanly.
    let bundle = api_strategy::get(ctx, &req.agent_id).await?;

    // 2. Look up the scenario from the canonical set.
    let scenario: Scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == req.scenario_id)
        .ok_or_else(|| ApiError::NotFound(format!("scenario '{}'", req.scenario_id)))?;

    // 3. Pick the executor for this run mode.
    let executor: Box<dyn Executor> = match req.mode {
        RunMode::Paper => {
            let b = broker.ok_or_else(|| {
                ApiError::Validation("paper mode requires a broker".into())
            })?;
            Box::new(PaperExecutor::new(b))
        }
        RunMode::Backtest => Box::new(BacktestExecutor::new()),
    };

    // 4. Build a fresh Run, persist, then drive the executor.
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

    // Clone the dispatch Arc so we can reuse it for the post-finalize
    // findings extraction below without re-paying client setup.
    let dispatch_for_postprocess = dispatch.clone();

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
        // Index the failed run so it shows up in ⌘K with its current status
        // — operators frequently want to find a recently-failed run by id
        // prefix without leaving the palette.
        if let Ok(failed) = store.get(&run.id).await {
            api_search::upsert_run(ctx, &failed).await;
        }
        return Err(ApiError::Internal(format!("executor: {err_msg}")));
    }

    // Re-read from the store so the returned Run reflects the canonical
    // post-finalize state — completed_at + metrics_json are set inside
    // RunStore::finalize and we want callers to see them.
    let finalized = store
        .get(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("re-read finalized run: {e}")))?;
    api_search::upsert_run(ctx, &finalized).await;

    // Postprocess: drive the findings extractor against the finalized run,
    // persist + index any findings. Best-effort — extractor failures
    // (LLM timeout, parse error) log + audit but never fail the run.
    // Reuses the same dispatch instance so we don't re-pay client setup.
    crate::eval::postprocess::extract_and_record(
        ctx,
        &finalized.id,
        dispatch_for_postprocess,
        crate::eval::postprocess::DEFAULT_FINDINGS_MODEL,
    )
    .await;

    Ok(finalized)
}

pub async fn scenarios(ctx: &ApiContext) -> ApiResult<Vec<ScenarioSummary>> {
    let started = Instant::now();
    let summaries: Vec<ScenarioSummary> = canonical_scenarios()
        .into_iter()
        .map(|s| {
            let asset_universe: Vec<String> =
                s.asset.iter().map(|a| a.venue_symbol.clone()).collect();
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
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == run.scenario_id)
        .ok_or_else(|| {
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
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow::anyhow!("create {}: {e}", dir.display()))?;
    let mut rng = rand_core::OsRng;
    let key = SigningKey::generate(&mut rng);
    let bytes = key.to_bytes();
    std::fs::write(&path, bytes)
        .map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(key)
}

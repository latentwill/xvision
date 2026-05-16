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
use crate::eval::compare::{compare_runs, ComparisonReport};
use crate::eval::executor::{BacktestExecutor, Executor, PaperExecutor};
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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRunsRequest {
    pub agent_id: Option<String>,
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
    pub actual_input_tokens: Option<u64>,
    pub actual_output_tokens: Option<u64>,
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
    };
    store
        .list(filter)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

    Ok(RunDetail {
        summary: summarise(run),
        decisions,
        equity_curve,
    })
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
    let dispatch_arc = build_eval_dispatch(ctx, &strategy, &agent_slots).await?;
    let tools_arc = Arc::new(ToolRegistry::default_with_builtins());
    run_with_deps(ctx, req, broker, dispatch_arc, tools_arc).await
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
                Err(ApiError::Validation(format!(
                    "Alpaca paper credentials not configured. Set them in Settings → Brokers, or export APCA_API_KEY_ID + APCA_API_SECRET_KEY before running."
                )))
            } else {
                Err(ApiError::Internal(format!("alpaca paper from env: {e}")))
            }
        }
    }
}

async fn build_eval_dispatch(
    ctx: &ApiContext,
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
) -> ApiResult<Arc<dyn LlmDispatch>> {
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
    dispatch_from_provider(entry).await
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

fn validate_eval_trader_source(
    strategy: &crate::strategies::Strategy,
    agent_slots: &[ResolvedAgentSlot],
) -> ApiResult<()> {
    if agent_slots.is_empty() {
        if strategy.trader_slot.is_some() {
            return Ok(());
        }
        return Err(ApiError::Validation(format!(
            "eval requires a trader output source for strategy `{}`. Add a legacy trader slot or attach an agent with role `trader`.",
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
        "eval requires an attached agent with role `trader` when strategy `{}` uses attached agents. Attached roles: [{}]. Attach a trader agent, or remove attached agents to use the legacy trader slot.",
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
                let legacy = slot.model_requirement.trim();
                let enabled = if entry.enabled_models.is_empty() {
                    "No models are enabled for this provider.".to_string()
                } else {
                    format!("Enabled models: {}", entry.enabled_models.join(", "))
                };
                ApiError::Validation(format!(
                    "provider `{}` is selected for strategy role `{}`, but no explicit model is configured. Legacy model_requirement `{legacy}` is not used as a provider model id. {enabled}",
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
            max_tokens: slot.resolve_max_tokens(),
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

    // 3. Pick the executor for this run mode. For backtest mode, when the
    //    scenario came from the DB we try to source bars through the
    //    cache wrapper (`eval::bars::load_bars`); on miss / fetch error
    //    we fall back to the legacy `data/probes/<cache_key>.parquet`
    //    loader so existing test fixtures keep working.
    let executor: Box<dyn Executor> = match req.mode {
        RunMode::Paper => {
            let b = broker.ok_or_else(|| ApiError::Validation("paper mode requires a broker".into()))?;
            build_paper_executor(ctx, &scenario, from_db, b).await?
        }
        RunMode::Backtest => build_backtest_executor(ctx, &scenario, from_db).await?,
    };

    // 4. Build a fresh Run, persist, then drive the executor.
    let mut run = Run::new_queued(req.agent_id.clone(), scenario.id.clone(), req.mode);
    run.params_override = req.params_override.clone();

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
        // see why this run is not Completed.
        let err_msg = e.to_string();
        let _ = store.fail_active(&run.id, &err_msg).await;
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
    Ok(Box::new(
        PaperExecutor::with_bars(broker, bars)
            .with_warmup(warmup)
            .with_event_bus(ctx.event_bus.clone()),
    ))
}

async fn build_backtest_executor(
    ctx: &ApiContext,
    scenario: &Scenario,
    from_db: bool,
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
                return Ok(Box::new(
                    BacktestExecutor::with_bars(ohlcv)
                        .with_warmup(warmup)
                        .with_event_bus(ctx.event_bus.clone()),
                ));
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

    Ok(Box::new(
        BacktestExecutor::new().with_event_bus(ctx.event_bus.clone()),
    ))
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
    let dispatch = build_eval_dispatch(ctx, &strategy, &agent_slots).await?;
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let executor: Box<dyn Executor> = match req.mode {
        RunMode::Paper => {
            let b = broker.expect("paper mode broker built above");
            build_paper_executor(ctx, &scenario, from_db, b).await?
        }
        RunMode::Backtest => build_backtest_executor(ctx, &scenario, from_db).await?,
    };

    let mut run = Run::new_queued(req.agent_id.clone(), scenario.id.clone(), req.mode);
    run.params_override = req.params_override.clone();
    let store = RunStore::new(ctx.db.clone());
    store
        .create(&run)
        .await
        .map_err(|e| ApiError::Internal(format!("create run: {e}")))?;

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

    let ctx_bg = ctx.clone();
    let run_id = run.id.clone();
    tokio::spawn(async move {
        execute_in_background(
            ctx_bg,
            run,
            strategy,
            scenario,
            agent_slots,
            executor,
            dispatch,
            tools,
        )
        .await;
    });

    get_run(ctx, &run_id).await
}

/// Background-task body: transition Queued → Running, drive the
/// executor, and on completion/failure persist the canonical state.
/// Detached — failures here can't propagate to the spawning request, so
/// every error path writes to the run row's `error` field and logs at
/// the `xvision::eval` target.
async fn execute_in_background(
    ctx: ApiContext,
    mut run: Run,
    strategy: crate::strategies::Strategy,
    scenario: Scenario,
    agent_slots: Vec<ResolvedAgentSlot>,
    executor: Box<dyn Executor>,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
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
            return;
        }
        Err(e) => {
            tracing::error!(
                target: "xvision::eval",
                run_id = %run.id,
                error = %e,
                "failed to transition Queued → Running",
            );
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
            return;
        }
        tracing::error!(
            target: "xvision::eval",
            run_id = %run.id,
            error = %e,
            error_chain = %err_msg,
            "executor failed",
        );
        let _ = store.fail_active(&run.id, &err_msg).await;
        if let Ok(failed) = store.get(&run.id).await {
            api_search::upsert_run(&ctx, &failed).await;
        }
        return;
    }

    let finalized = match store.get(&run.id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(
                target: "xvision::eval",
                run_id = %run.id,
                error = %e,
                "failed to re-read finalized run",
            );
            return;
        }
    };
    api_search::upsert_run(&ctx, &finalized).await;

    // Best-effort findings extraction — failures audit but don't reopen
    // the run.
    crate::eval::postprocess::extract_and_record(
        &ctx,
        &finalized.id,
        dispatch_for_postprocess,
        crate::eval::postprocess::DEFAULT_FINDINGS_MODEL,
    )
    .await;
}

/// Sweep any `Queued` or `Running` rows from a previous process and
/// transition them to `Failed`. Background tasks die with the dashboard
/// process so a clean restart should fail orphans out before serving
/// traffic — otherwise the runs list shows phantom "Running" rows.
pub async fn fail_orphan_runs(ctx: &ApiContext) -> ApiResult<u64> {
    let store = RunStore::new(ctx.db.clone());
    store
        .fail_active_runs("daemon restarted before run completed")
        .await
        .map_err(|e| ApiError::Internal(format!("fail orphan runs: {e}")))
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
    let (sharpe, max_dd, total_return) = match &run.metrics {
        Some(m) => (Some(m.sharpe), Some(m.max_drawdown_pct), Some(m.total_return_pct)),
        None => (None, None, None),
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

mod tests {
    use super::*;
    use crate::strategies::{
        manifest::PublicManifest, risk::RiskPreset, slot::LLMSlot, AgentRef, PipelineDef, Strategy,
    };

    fn provider(enabled_models: Vec<&str>) -> ProviderEntry {
        ProviderEntry {
            name: "openrouter".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://openrouter.ai/api/v1".into(),
            api_key_env: "OPENROUTER_API_KEY".into(),
            enabled_models: enabled_models.into_iter().map(str::to_string).collect(),
        }
    }

    fn slot(provider: Option<&str>, model: Option<&str>, model_requirement: &str) -> LLMSlot {
        LLMSlot {
            role: "trader".into(),
            prompt: "Trade.".into(),
            model_requirement: model_requirement.into(),
            allowed_tools: Vec::new(),
            provider: provider.map(str::to_string),
            model: model.map(str::to_string),
        }
    }

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
                required_models: Vec::new(),
                required_tools: Vec::new(),
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
            },
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
            max_tokens: 4096,
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

    #[test]
    fn eval_trader_source_accepts_legacy_trader_slot_without_agents() {
        let legacy_slot = slot(Some("openrouter"), None, "anthropic.claude-sonnet-4.6");
        let mut strategy = strategy_with_legacy_slot(legacy_slot);
        strategy.agents.clear();

        validate_eval_trader_source(&strategy, &[]).unwrap();
    }

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
            max_tokens: 4096,
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
        assert!(
            msg.contains("legacy trader slot"),
            "expected legacy slot remediation in error, got {msg}"
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
            max_tokens: 4096,
        }];

        validate_eval_trader_source(&strategy, &agent_slots).unwrap();
    }
}

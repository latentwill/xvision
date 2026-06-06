//! POST /api/autooptimizer/run-cycle — launch an optimizer run.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use xvision_core::config::ProviderKind;
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, OpenaiCompatDispatch};
use xvision_engine::api::memory;
use xvision_engine::autooptimizer::{
    blob_store::BlobStore,
    config::AutoOptimizerConfig,
    content_hash::ContentHash,
    cycle::{run_cycle, CycleConfig},
    cycle_runs::persist_cycle_cost,
    dspy_bridge::LiveDspyBridge,
    dspy_flywheel::DspyContext,
    eval_adapter::{BudgetCappedPaperTester, CachedBacktestPaperTester, PaperTestRunner},
    gate::GateVerdict,
    judge::Judge,
    lineage::{LineageNode, LineageStatus, LineageStore},
    metering_dispatch::{CostMeteringDispatch, CycleMeter},
    mutator::Mutator,
    parent_policy::ParentPolicy,
    preflight::preflight_trader_provider,
    scenario_synthesis::{synthesize_baseline_untouched_scenario, synthesize_optimizer_day_scenario},
};
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct StartCycleBody {
    pub strategy_id: Option<String>,
    pub mutator_provider: Option<String>,
    pub mutator_model: Option<String>,
    pub judge_provider: Option<String>,
    pub judge_model: Option<String>,
    /// F28: token budget in USD for this cycle. Once the metered paper-test cost
    /// reaches this ceiling the cycle stops before launching another backtest.
    /// Omit (or null) for no cap. Mirrors the CLI `--budget`.
    pub budget_usd: Option<f64>,
    /// F28: per-run evaluation window overrides (YYYY-MM-DD), mirroring the CLI
    /// `--day-*/--baseline-*` flags. Without these a UI launch ran the full
    /// ~20-month default window (~16k bars/candidate). Narrow them to bound
    /// bar-fetch cost/latency.
    pub day_start: Option<chrono::NaiveDate>,
    pub day_end: Option<chrono::NaiveDate>,
    pub baseline_start: Option<chrono::NaiveDate>,
    pub baseline_end: Option<chrono::NaiveDate>,
}

#[derive(Serialize)]
pub struct StartCycleResponse {
    pub started: bool,
    pub message: String,
}

pub async fn start_cycle(
    State(state): State<AppState>,
    Json(body): Json<StartCycleBody>,
) -> Result<(StatusCode, Json<StartCycleResponse>), DashboardError> {
    let mut cfg = load_optimizer_config()?;
    // F28: apply per-run evaluation-window overrides (bound bar-fetch cost), then
    // re-validate so an inverted/overlapping window fails fast with a clear
    // message instead of deep in scenario synthesis. Mirrors the CLI run-cycle.
    if let Some(d) = body.day_start {
        cfg.day_window.start = d;
    }
    if let Some(d) = body.day_end {
        cfg.day_window.end = d;
    }
    if let Some(d) = body.baseline_start {
        cfg.baseline_untouched_window.start = d;
    }
    if let Some(d) = body.baseline_end {
        cfg.baseline_untouched_window.end = d;
    }
    cfg.validate().map_err(|e| DashboardError::Validation {
        field: "window".into(),
        msg: format!("invalid optimizer window after overrides: {e}"),
    })?;
    // F28: validate the budget ceiling up-front (a non-positive/NaN cap is a
    // client error, not a silently-ignored one).
    if let Some(b) = body.budget_usd {
        if !b.is_finite() || b <= 0.0 {
            return Err(DashboardError::Validation {
                field: "budget_usd".into(),
                msg: "budget_usd must be a finite positive USD value".into(),
            });
        }
    }
    let budget_cap = body.budget_usd.unwrap_or(f64::INFINITY);
    let mutator_provider = body
        .mutator_provider
        .unwrap_or_else(|| cfg.mutator.provider.clone());
    let mutator_model = body.mutator_model.unwrap_or_else(|| cfg.mutator.model.clone());
    let judge_provider = body.judge_provider.unwrap_or_else(|| mutator_provider.clone());
    let judge_model = body.judge_model.unwrap_or_else(|| cfg.mutator.model.clone());
    let raw_mutator_dispatch = build_autooptimizer_dispatch(&mutator_provider, &state.xvn_home).await?;
    let raw_judge_dispatch = if judge_provider == mutator_provider {
        Arc::clone(&raw_mutator_dispatch)
    } else {
        build_autooptimizer_dispatch(&judge_provider, &state.xvn_home).await?
    };

    // F11/F23/F26: one shared meter for the whole cycle — tokens, realized cost,
    // and unpriced-call count. The metering dispatch wraps EVERY LLM call site
    // (experiment writer + judge + the paper-test backtest decisions, which route
    // through the metered mutator dispatch), so the per-cycle cost the panel reads
    // is real, not the old `$0.00`. Cost is metered at the dispatch boundary, the
    // same way the CLI `run-cycle` does it.
    let meter: Arc<Mutex<CycleMeter>> = Arc::new(Mutex::new(CycleMeter::default()));
    let mutator_catalogs = load_metering_catalogs(&state.xvn_home, &mutator_provider).await;
    let metered_mutator: Arc<dyn LlmDispatch + Send + Sync> = Arc::new(CostMeteringDispatch::new(
        Arc::clone(&raw_mutator_dispatch),
        mutator_catalogs,
        Arc::clone(&meter),
    ));
    let metered_judge: Arc<dyn LlmDispatch + Send + Sync> = if judge_provider == mutator_provider {
        Arc::clone(&metered_mutator)
    } else {
        let judge_catalogs = load_metering_catalogs(&state.xvn_home, &judge_provider).await;
        Arc::new(CostMeteringDispatch::new(
            Arc::clone(&raw_judge_dispatch),
            judge_catalogs,
            Arc::clone(&meter),
        ))
    };

    let day_scenario = build_day_scenario(&cfg)?;
    let baseline_scenario =
        synthesize_baseline_untouched_scenario(&day_scenario, &cfg.baseline_untouched_window)?;
    // The paper-test backtest dispatches every trader decision through the
    // metered mutator dispatch (the cycle provider), so the strategy's trader
    // must route to that same provider.
    let cycle_provider = mutator_provider.clone();
    let (mutator, judge) = build_mutator_and_judge(
        &cfg,
        mutator_provider,
        mutator_model,
        Arc::clone(&metered_mutator),
        judge_provider,
        judge_model,
        metered_judge,
    );
    let pool = state.pool.clone();
    let lineage_store = LineageStore::new(pool.clone());
    let strategy_blob_store = BlobStore::new(state.xvn_home.join("lineage").join("blobs"));
    let strategy_id = body
        .strategy_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| DashboardError::Validation {
            field: "strategy_id".into(),
            msg: "strategy_id is required for dashboard run-cycle launches".into(),
        })?;
    let (bundle_hash, strategy) =
        load_strategy_parent(strategy_id, &state.xvn_home, &lineage_store, &strategy_blob_store).await?;
    // F22/F26: fail fast with guidance instead of a confusing cross-provider 400
    // when the strategy's trader would route to a provider other than the cycle's.
    // Shared with the CLI via `autooptimizer::preflight` — no parallel guard.
    preflight_trader_provider(&pool, &strategy, strategy_id, &cycle_provider, false)
        .await
        .map_err(|e| DashboardError::Validation {
            field: "strategy_id".into(),
            msg: e.message,
        })?;
    let mut parent_strategies = HashMap::new();
    parent_strategies.insert(bundle_hash.to_hex(), strategy);
    let explicit_parent_hashes = vec![bundle_hash];
    let cycle_config = build_cycle_config(
        &cfg,
        &judge,
        day_scenario,
        baseline_scenario,
        parent_strategies,
        explicit_parent_hashes,
    );
    let tx = state.autooptimizer_tx.clone();
    // F13: write candidate strategy blobs to the same `lineage/blobs` root the
    // `/blob/:hash` endpoint reads, so cycle children are retrievable.
    let cycle_blob_store = BlobStore::new(state.xvn_home.join("lineage").join("blobs"));
    // F26: build a fully-wired ApiContext (event bus + observability) so the
    // dashboard's optimizer cycle drives REAL backtests through the same shared
    // `Executor`/`run_pipeline` path as the CLI, `eval run`, the chat rail, and
    // live — not the deterministic stub that always tied and rejected everything.
    let api_ctx = state.api_context();
    let backtest_dispatch = Arc::clone(&metered_mutator);
    // F35: generate the cycle id up-front so a background ticker can persist the
    // running cost/tokens INCREMENTALLY under it. Previously the cost was written
    // once at cycle end, so a killed/crashed UI cycle (the runaway-token case)
    // recorded $0.00 — the operator's "cost shows $0.00" report. With an upfront
    // id + ticker, partial spend survives an interrupt.
    let cycle_id = ulid::Ulid::new().to_string();
    // F34: serialize cycles per workspace (cross-process via the shared DB lock).
    // Refuse to launch if a CLI or dashboard cycle is already running, instead of
    // starving each other.
    match xvision_engine::autooptimizer::run_lock::try_acquire(
        &state.pool,
        &cycle_id,
        "dashboard",
        Utc::now(),
    )
    .await
    .map_err(DashboardError::Internal)?
    {
        xvision_engine::autooptimizer::run_lock::Acquire::Acquired => {}
        xvision_engine::autooptimizer::run_lock::Acquire::Busy {
            cycle_id: holder_cycle,
            holder,
            acquired_at,
        } => {
            return Err(DashboardError::Conflict(format!(
                "an optimizer cycle is already running on this workspace (cycle {holder_cycle}, \
                 holder {holder}, since {acquired_at}). Wait for it to finish or cancel it before \
                 starting another."
            )));
        }
    }
    // F28: register a cooperative cancel flag for this cycle so
    // `POST /cycles/:id/cancel` can stop it. Deregistered when the cycle ends.
    let cancel_flag = state.autooptimizer_register_cancel(&cycle_id);
    let state_for_dereg = state.clone();
    // Cortex memory: capture the recorder Arc (gated, config-backed default
    // ON; env override wins) before the spawn so the cycle can recall/record
    // distilled findings. `None` when disabled — the cycle then behaves as
    // before. Cloned into an owned binding so `.as_deref()` borrows from a
    // value that outlives the `run_cycle` call inside the task.
    let cycle_memory = if state.optimizer_memory_enabled() {
        state.memory_recorder.clone()
    } else {
        None
    };
    // DSPy in-loop bridge: when `dspy_enabled = true`, open the memory store
    // and build a `DspyContext` with a `LiveDspyBridge` before the spawn so
    // the owned context can be moved into the task.  Mirrors the `cycle_memory`
    // pattern above.  The bridge reuses `metered_mutator` (the
    // `CostMeteringDispatch`-wrapped mutator dispatch) so DSPy reflection is
    // priced through the shared per-cycle meter.
    let cycle_dspy_ctx: Option<DspyContext> = if cfg.dspy_enabled {
        match memory::open_default_store().await {
            Ok(store) => Some(DspyContext {
                store,
                bridge: std::sync::Arc::new(LiveDspyBridge {
                    dispatch: std::sync::Arc::clone(&metered_mutator),
                    model: cfg.mutator.model.clone(),
                }),
                namespace: "autooptimizer:dspy".to_string(),
            }),
            Err(e) => {
                tracing::warn!(error = %e, "dspy_enabled but could not open memory store; skipping DSPy context");
                None
            }
        }
    } else {
        None
    };
    tokio::spawn(async move {
        // The production paper tester: real cached-backtest Executor, metered at
        // the dispatch boundary, with the shared per-cycle meter feeding both the
        // (currently unbounded) budget gate and the persisted cost summary.
        let cached = CachedBacktestPaperTester::new(
            api_ctx,
            backtest_dispatch,
            Arc::new(ToolRegistry::default_with_builtins()),
        );
        // F28: enforce the operator-set budget ceiling. Once the metered
        // paper-test cost reaches `budget_cap`, the cycle stops before launching
        // another backtest (no cap → f64::INFINITY). This is the guard against the
        // runaway token spew an unbounded UI cycle could produce.
        let paper_tester: Box<dyn PaperTestRunner> = Box::new(
            BudgetCappedPaperTester::new_with_handle(Box::new(cached), budget_cap, Arc::clone(&meter)),
        );

        // F35: incremental cost/token persistence. Every 10s, snapshot the shared
        // meter into `cycle_cost` under the known cycle id so the panel shows
        // climbing spend during the run and a killed cycle keeps its partial cost.
        let ticker_pool = pool.clone();
        let ticker_meter = Arc::clone(&meter);
        let ticker_cycle_id = cycle_id.clone();
        let ticker = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let totals = *ticker_meter.lock().expect("meter mutex poisoned");
                let _ = persist_cycle_cost(
                    &ticker_pool,
                    &ticker_cycle_id,
                    &totals,
                    &Utc::now().to_rfc3339(),
                )
                .await;
            }
        });

        let result = run_cycle(
            &pool,
            &cycle_blob_store,
            &cfg,
            &cycle_config,
            &ParentPolicy::RoundRobin,
            &mutator,
            &judge,
            paper_tester.as_ref(),
            move |ev| {
                let _ = tx.send(ev);
            },
            // DSPy in-loop: `Some` when `dspy_enabled = true` and the store
            // opened successfully; `None` otherwise (operator opt-in default off).
            cycle_dspy_ctx.as_ref(),
            // Cortex memory: optimizer recall/record (default ON, config-backed).
            cycle_memory.as_deref(),
            Some(cycle_id.clone()),
            Some(cancel_flag),
        )
        .await;
        // Stop the ticker; we persist the final totals below.
        ticker.abort();
        // F28: drop the cancel flag now the cycle is finished.
        state_for_dereg.autooptimizer_deregister_cancel(&cycle_id);
        // F34: release the workspace cycle lock so the next cycle can run.
        let _ = xvision_engine::autooptimizer::run_lock::release(&pool, &cycle_id).await;
        // F23/F26/F35: persist the FINAL per-cycle tokens + cost so the panel and
        // `GET /api/autooptimizer/cycles/:id` show real spend after the run.
        // Best-effort — a failure here must not mask the completed cycle. Keyed by
        // the upfront `cycle_id` so this lands even if `run_cycle` errored.
        let totals = *meter.lock().expect("meter mutex poisoned");
        if let Err(e) =
            persist_cycle_cost(&pool, &cycle_id, &totals, &Utc::now().to_rfc3339()).await
        {
            tracing::warn!(error = %e, "persist optimizer cycle cost failed");
        }
        if let Err(e) = result {
            tracing::warn!(error = %e, "optimizer cycle failed");
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(StartCycleResponse {
            started: true,
            message: "Optimizer run started. Watch the Live tab for progress.".into(),
        }),
    ))
}

/// F28: `POST /api/autooptimizer/cycles/:cycle_id/cancel` — request cancellation
/// of an in-flight optimizer cycle. Sets the cooperative cancel flag so the cycle
/// stops launching further candidates (the current backtest, if any, finishes —
/// bounded — then the cycle returns). 404 if no cycle with that id is running.
pub async fn cancel_cycle(
    State(state): State<AppState>,
    Path(cycle_id): Path<String>,
) -> Result<(StatusCode, Json<StartCycleResponse>), DashboardError> {
    if state.autooptimizer_request_cancel(&cycle_id) {
        Ok((
            StatusCode::ACCEPTED,
            Json(StartCycleResponse {
                started: false,
                message: format!(
                    "Cancellation requested for cycle {cycle_id}; it will stop before the next candidate."
                ),
            }),
        ))
    } else {
        Err(DashboardError::NotFound(format!(
            "no in-flight optimizer cycle '{cycle_id}'"
        )))
    }
}

fn load_optimizer_config() -> Result<AutoOptimizerConfig, DashboardError> {
    let cfg = match AutoOptimizerConfig::default_path() {
        Ok(path) if path.exists() => AutoOptimizerConfig::load(&path)?,
        _ => AutoOptimizerConfig::default(),
    };
    Ok(cfg)
}

async fn build_autooptimizer_dispatch(
    provider: &str,
    xvn_home: &std::path::Path,
) -> Result<Arc<dyn LlmDispatch + Send + Sync>, DashboardError> {
    let config_path = xvision_core::config::runtime_config_path(xvn_home);
    let provider_name = provider.to_owned();
    let rt = tokio::task::spawn_blocking(move || xvision_core::config::load_runtime(&config_path))
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("task join: {e}")))?
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("load runtime config: {e}")))?;
    let entry = rt
        .providers
        .iter()
        .find(|p| p.name == provider_name)
        .ok_or_else(|| DashboardError::Validation {
            field: "provider".into(),
            msg: format!("autooptimizer provider '{provider_name}' not configured in Settings → Providers"),
        })?;
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(&entry.api_key_env).map_err(|_| DashboardError::Validation {
            field: "provider".into(),
            msg: format!(
                "env var '{}' unset for provider '{provider_name}'",
                entry.api_key_env
            ),
        })?
    };
    Ok(match entry.kind {
        ProviderKind::Anthropic => Arc::new(AnthropicDispatch::new(api_key)),
        ProviderKind::OpenaiCompat | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::LocalCandle => {
            return Err(DashboardError::Validation {
                field: "provider".into(),
                msg: "local-candle is not supported for the autooptimizer".into(),
            })
        }
    })
}

fn build_mutator_and_judge(
    cfg: &AutoOptimizerConfig,
    mutator_provider: String,
    mutator_model: String,
    mutator_dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    judge_provider: String,
    judge_model: String,
    judge_dispatch: Arc<dyn LlmDispatch + Send + Sync>,
) -> (Mutator, Judge) {
    let mutator = Mutator {
        provider: mutator_provider,
        model: mutator_model,
        dispatch: mutator_dispatch,
        max_retries: cfg.mutator.max_retries,
    };
    let judge = Judge {
        dispatch: judge_dispatch,
        provider: judge_provider,
        model: judge_model,
    };
    (mutator, judge)
}

fn build_cycle_config(
    cfg: &AutoOptimizerConfig,
    judge: &Judge,
    day_scenario: Scenario,
    baseline_scenario: Scenario,
    parent_strategies: HashMap<String, Strategy>,
    explicit_parent_hashes: Vec<ContentHash>,
) -> CycleConfig {
    CycleConfig {
        num_parents: if explicit_parent_hashes.is_empty() {
            2
        } else {
            explicit_parent_hashes.len()
        },
        mutations_per_parent: 1,
        sabotage_seed: 42,
        judge_provider: cfg.mutator.provider.clone(),
        judge_model: judge.model.clone(),
        prompt_version: "v1".into(),
        sustained_no_pass_cycles: 0,
        day_scenario,
        baseline_scenario,
        parent_strategies,
        explicit_parent_hashes,
        objective: cfg.objective,
        regime_set: cfg.regime_set.clone(),
    }
}

async fn load_strategy_parent(
    strategy_id: &str,
    xvn_home: &std::path::Path,
    lineage: &LineageStore,
    blobs: &BlobStore,
) -> Result<(ContentHash, Strategy), DashboardError> {
    let store = FilesystemStore::new(strategy_store_dir(xvn_home));
    store
        .path_for(strategy_id)
        .map_err(|e| DashboardError::Validation {
            field: "strategy_id".into(),
            msg: format!("invalid strategy id '{strategy_id}': {e}"),
        })?;
    let strategy = store.load(strategy_id).await.map_err(|e| {
        if e.to_string().contains("reading ") {
            DashboardError::NotFound(format!("strategy '{strategy_id}' not found"))
        } else {
            DashboardError::Internal(anyhow::anyhow!("load strategy '{strategy_id}': {e}"))
        }
    })?;
    let strategy_json = serde_json::to_value(&strategy)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("serialize strategy '{strategy_id}': {e}")))?;
    let bundle_hash = blobs
        .put_json(&strategy_json)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("write strategy blob '{strategy_id}': {e}")))?;

    match lineage
        .get(&bundle_hash)
        .await
        .map_err(DashboardError::Internal)?
    {
        Some(node) if node.status != LineageStatus::Active => {
            return Err(DashboardError::Validation {
                field: "strategy_id".into(),
                msg: format!(
                    "strategy '{strategy_id}' resolves to lineage parent {} but that parent is not active",
                    bundle_hash.to_hex()
                ),
            });
        }
        Some(_) => {}
        None => {
            let root_node = LineageNode {
                bundle_hash,
                parent_hash: None,
                gate_verdict: GateVerdict::Pass,
                status: LineageStatus::Active,
                cycle_id: None,
                created_at: Utc::now(),
                diversity_score: None,
            };
            lineage
                .insert(&root_node)
                .await
                .map_err(DashboardError::Internal)?;
        }
    }

    Ok((bundle_hash, strategy))
}

fn build_day_scenario(cfg: &AutoOptimizerConfig) -> Result<Scenario, DashboardError> {
    // F10: delegate to the single shared optimizer scenario builder so the
    // dashboard and CLI never drift on venue/fee/fill settings.
    Ok(synthesize_optimizer_day_scenario(
        &cfg.day_window,
        "xvn-dashboard",
    ))
}

/// Load the cached provider catalog for `provider` so the metering dispatch can
/// price each LLM completion. Best-effort: an absent/unreadable catalog yields no
/// pricing (calls are counted as "unpriced", never silently $0) — the same
/// "unknown ≠ zero" stance the CLI uses.
async fn load_metering_catalogs(
    xvn_home: &std::path::Path,
    provider: &str,
) -> Vec<Arc<xvision_core::providers::Catalog>> {
    match xvision_engine::providers::load_cached_catalog(xvn_home, provider).await {
        Ok(Some(cat)) => vec![Arc::new(cat)],
        _ => Vec::new(),
    }
}

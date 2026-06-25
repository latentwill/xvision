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

use xvision_core::config::{AgentRuntime, ProviderKind};
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, OpenaiCompatDispatch};
use xvision_engine::api::memory;
use xvision_engine::autooptimizer::{
    blob_store::BlobStore,
    config::AutoOptimizerConfig,
    content_hash::ContentHash,
    cycle::{run_cycle, CycleConfig},
    cycle_runs::persist_cycle_cost,
    dspy_flywheel::DspyContext,
    eval_adapter::{BudgetCappedPaperTester, CachedBacktestPaperTester, PaperTestRunner},
    events_store,
    gate::GateVerdict,
    judge::Judge,
    lineage::{LineageNode, LineageStatus, LineageStore},
    metering_dispatch::{CostMeteringDispatch, CycleMeter},
    mutator::Mutator,
    parent_policy::ParentPolicy,
    preflight::infer_trader_provider,
    preflight::preflight_trader_provider,
    preflight_cycle,
    progress::CycleProgressEvent,
    scenario_synthesis::{synthesize_baseline_untouched_scenario, synthesize_optimizer_day_scenario},
};
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

use crate::error::DashboardError;
use crate::routes::autooptimizer::table_exists;
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
    /// Candidate experiments to generate per parent this cycle (1..=64).
    /// Overrides `experiments_per_cycle` from autooptimizer.toml; omit for the
    /// configured value. Mirrors the CLI `--experiments-per-cycle`.
    pub experiments_per_cycle: Option<u32>,
}

#[derive(Serialize)]
pub struct StartCycleResponse {
    pub started: bool,
    pub message: String,
    /// P1-W4: new additive field. Existing callers that only read `started`/`message`
    /// are unaffected. Set when a session record was created for this run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Serialize)]
pub struct RunDefaultsResponse {
    pub mutator_provider: String,
    pub mutator_model: String,
    pub judge_provider: String,
    pub judge_model: String,
    pub config_path: String,
    pub config_exists: bool,
}

pub async fn run_defaults() -> Result<Json<RunDefaultsResponse>, DashboardError> {
    let (cfg, config_path, config_exists) = load_optimizer_config_with_path()?;
    Ok(Json(RunDefaultsResponse {
        mutator_provider: cfg.mutator.provider.clone(),
        mutator_model: cfg.mutator.model.clone(),
        // Dashboard run-cycle defaults the reviewer to the writer provider/model.
        judge_provider: cfg.mutator.provider.clone(),
        judge_model: cfg.mutator.model.clone(),
        config_path: config_path.display().to_string(),
        config_exists,
    }))
}

/// Extract `(session_id, cycle_id)` from a `CycleProgressEvent`.
/// `SessionStateChanged` has no `cycle_id` field, so we return `None` for it.
fn event_ids(ev: &CycleProgressEvent) -> (String, Option<String>) {
    use CycleProgressEvent::*;
    match ev {
        CycleStarted {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        ParentSelected {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        MutationProposed {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        NoCandidate {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        CandidateError {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        MutationGated {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        HonestyCheckRun {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        JudgeFinding {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        CycleFinished {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        PhaseStarted {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        PhaseFinished {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        EvalProgress {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        Heartbeat {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
        SessionStateChanged { session_id, .. } => (session_id.clone(), None),
        FlywheelCompiled {
            session_id, cycle_id, ..
        } => (session_id.clone(), Some(cycle_id.clone())),
    }
}

/// Persist a `CycleProgressEvent` to `autooptimizer_events`. Best-effort:
/// any storage error is logged as a warning and never propagates to the caller.
///
/// Dashboard-launched cycles run outside a session: `run_cycle` emits events
/// with an EMPTY `session_id`. Persisting "" would orphan the rows —
/// `prune_old_events` retains only session_ids present in
/// `autooptimizer_session_state` (plus `cycle:`-prefixed keys), so "" rows
/// would be deleted on the next prune, silently breaking cycle replay.
/// Substitute the stable fallback key `cycle:<cycle_id>` instead.
async fn persist_progress_event(pool: &sqlx::SqlitePool, ev: &CycleProgressEvent) {
    use crate::sse::autooptimizer_labels::event_kind;
    let (session_id, cycle_id) = event_ids(ev);
    let session_key = if session_id.is_empty() {
        format!("cycle:{}", cycle_id.as_deref().unwrap_or("unknown"))
    } else {
        session_id
    };
    let payload = serde_json::to_string(ev).unwrap_or_else(|_| "{}".into());
    if let Err(e) =
        events_store::append_event(pool, &session_key, cycle_id.as_deref(), event_kind(ev), &payload).await
    {
        tracing::warn!(error = %e, "persist optimizer cycle event failed (best-effort)");
    }
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
    if let Some(n) = body.experiments_per_cycle {
        cfg.experiments_per_cycle = n;
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
    let judge_model = body.judge_model.unwrap_or_else(|| mutator_model.clone());
    let raw_mutator_dispatch =
        build_autooptimizer_dispatch(&mutator_provider, &mutator_model, &state.xvn_home).await?;
    let raw_judge_dispatch = if judge_provider == mutator_provider {
        if judge_model != mutator_model {
            validate_autooptimizer_model_allowlist(&judge_provider, &judge_model, &state.xvn_home).await?;
        }
        Arc::clone(&raw_mutator_dispatch)
    } else {
        build_autooptimizer_dispatch(&judge_provider, &judge_model, &state.xvn_home).await?
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
    // The paper-test backtest dispatches every trader decision. By default it
    // uses the mutator's provider (the cycle provider). But if the strategy's
    // trader routes to a different provider (e.g. ollama), we auto-detect that
    // and build a separate paper-test dispatch for the trader's provider so the
    // backtest decisions flow through the right API. This matches the CLI's
    // `--provider X --mutator-provider Y` pattern.
    let mutator_provider_for_cycle = mutator_provider.clone();
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
    // B9: load the seed strategy BEFORE building the scenario so the scenario
    // granularity matches the strategy's decision cadence (was hardcoded 1h).
    let (bundle_hash, strategy) =
        load_strategy_parent(strategy_id, &state.xvn_home, &lineage_store, &strategy_blob_store).await?;

    // Determine the trader's effective provider+model so we can route the
    // paper-test backtest through the right dispatch. Agents-backed strategies
    // resolve through agent slots; legacy strategies use the trader_slot.
    let paper_test_info = resolve_paper_test_provider(&pool, &strategy).await;
    let skip_provider_check = paper_test_info
        .as_ref()
        .map_or(false, |(p, _)| !p.eq_ignore_ascii_case(&mutator_provider_for_cycle));

    // When the trader uses a different provider, build a separate dispatch for
    // the paper-test so backtest decisions flow through the trader's own provider
    // (matching the CLI's `--provider <trader> --mutator-provider <other>` pattern).
    let (backtest_dispatch, effective_cycle_provider): (
        Arc<dyn LlmDispatch + Send + Sync>,
        String,
    ) = if let Some((trader_provider, trader_model)) = &paper_test_info {
        if *trader_provider != mutator_provider_for_cycle {
            let raw_pt = build_autooptimizer_dispatch(trader_provider, trader_model, &state.xvn_home).await?;
            let pt_catalogs = load_metering_catalogs(&state.xvn_home, trader_provider).await;
            let metered_pt: Arc<dyn LlmDispatch + Send + Sync> = Arc::new(
                CostMeteringDispatch::new(raw_pt, pt_catalogs, Arc::clone(&meter)),
            );
            (metered_pt, trader_provider.clone())
        } else {
            (Arc::clone(&metered_mutator), mutator_provider_for_cycle.clone())
        }
    } else {
        (Arc::clone(&metered_mutator), mutator_provider_for_cycle.clone())
    };

    let cadence_minutes = strategy.manifest.decision_cadence_minutes;
    let day_scenario = build_day_scenario(&cfg, cadence_minutes)?;
    let baseline_scenario =
        synthesize_baseline_untouched_scenario(&day_scenario, &cfg.baseline_untouched_window)?;
    let scenario_pool = build_scenario_pool(&cfg, cadence_minutes)?;

    // Preflight: shared with the CLI via `autooptimizer::preflight`.
    // When the trader's provider differs from the mutator's, skip the
    // provider-consistency check — we've already built a separate dispatch.
    preflight_trader_provider(
        &pool, &strategy, strategy_id, &effective_cycle_provider, false, skip_provider_check,
    )
    .await
    .map_err(|e| DashboardError::Validation {
        field: "strategy_id".into(),
        msg: e.message,
    })?;
    preflight_cycle::preflight_cycle(&pool, &strategy, strategy_id, false)
        .await
        .map_err(|e| DashboardError::Validation {
            field: "strategy_id".into(),
            msg: e.message,
        })?;

    let sidecar_provider = strategy
        .trader_slot
        .as_ref()
        .and_then(|s| s.provider.as_deref())
        .unwrap_or(&cfg.mutator.provider)
        .to_string();
    let mut parent_strategies = HashMap::new();
    parent_strategies.insert(bundle_hash.to_hex(), strategy);
    let explicit_parent_hashes = vec![bundle_hash];
    let cycle_config = build_cycle_config(
        &cfg,
        &judge,
        day_scenario,
        baseline_scenario,
        scenario_pool,
        parent_strategies,
        explicit_parent_hashes,
    );
    let tx = state.autooptimizer_tx.clone();
    let cycle_blob_store = BlobStore::new(state.xvn_home.join("lineage").join("blobs"));
    let api_ctx = state.api_context();
    // F35: generate the cycle id up-front so a background ticker can persist the
    // running cost/tokens INCREMENTALLY under it. Previously the cost was written
    // once at cycle end, so a killed/crashed UI cycle (the runaway-token case)
    // recorded $0.00 — the operator's "cost shows $0.00" report. With an upfront
    // id + ticker, partial spend survives an interrupt.
    let cycle_id = ulid::Ulid::new().to_string();
    // F34: serialize cycles per workspace (cross-process via the shared DB lock).
    // Refuse to launch if a CLI or dashboard cycle is already running, instead of
    // starving each other.
    let lock_outcome =
        xvision_engine::autooptimizer::run_lock::try_acquire(&state.pool, &cycle_id, "dashboard", Utc::now())
            .await
            .map_err(DashboardError::Internal)?;
    if let Some(reclaimed) = &lock_outcome.reclaimed {
        // GH #967: a stale prior lock was auto-cleared. Log it; the dashboard
        // does not block on this — the new cycle proceeds.
        tracing::warn!(
            prior_cycle = %reclaimed.prior_cycle,
            age_s = reclaimed.age_s,
            reason = %reclaimed.reason,
            "optimizer: cleared a stale cycle lock before launching",
        );
    }
    match lock_outcome.acquire {
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
    // P4: register a cooperative pause flag for this cycle so
    // `POST /cycles/:id/pause` can suspend it and `POST /cycles/:id/resume`
    // can continue it. Deregistered when the cycle ends.
    let pause_flag = state.autooptimizer_register_pause(&cycle_id);
    // Heartbeat TTL: register and tick so the UI can detect crashed cycles.
    state.autooptimizer_register_heartbeat(&cycle_id);
    let heartbeat_state = state.clone();
    let heartbeat_cycle_id = cycle_id.clone();
    let heartbeat = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            heartbeat_state.autooptimizer_heartbeat(&heartbeat_cycle_id);
        }
    });
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
    // and build a `DspyContext` with a `GepaBridge` before the spawn so
    // the owned context can be moved into the task.  Mirrors the `cycle_memory`
    // pattern above.  The bridge reuses `metered_mutator` (the
    // `CostMeteringDispatch`-wrapped mutator dispatch) so DSPy reflection is
    // priced through the shared per-cycle meter.
    let cycle_dspy_ctx: Option<DspyContext> = if cfg.dspy_enabled {
        match memory::open_default_store().await {
            Ok(store) => Some(DspyContext {
                store,
                bridge: std::sync::Arc::new(xvision_engine::autooptimizer::gepa::GepaBridge {
                    dispatch: std::sync::Arc::clone(&metered_mutator),
                    model: cfg.mutator.model.clone(),
                    provider: cfg.mutator.provider.clone(),
                    candidates: cfg.gepa_candidates,
                    generations: cfg.gepa_generations,
                    reflection_dispatch: None,
                    reflection_model: None,
                    selection_strategy: xvision_engine::autooptimizer::gepa::GepaSelectionStrategy::Pareto,
                    reflection_minibatch_size: 3,
                    skip_perfect: true,
                    use_merge: true,
                    merge_frequency: 3,
                }),
                namespace: "autooptimizer:dspy".to_string(),
                pool: pool.clone(),
            }),
            Err(e) => {
                tracing::warn!(error = %e, "dspy_enabled but could not open memory store; skipping DSPy context");
                None
            }
        }
    } else {
        None
    };
    // WU-6: the Cline sidecar is mandatory for the trader (LlmDispatch retired).
    // The sidecar dispatches TRADER LLM calls; mutator+judge use separate dispatch.
    // sidecar_provider was resolved from the strategy before it was moved.
    let cline_ctx = xvision_engine::api::eval::spawn_optimizer_cline_ctx(
        &api_ctx,
        &sidecar_provider,
        Arc::new(ToolRegistry::default_with_builtins()),
        xvision_engine::eval::run::RunMode::Backtest,
    )
    .await
    .map_err(|e| DashboardError::Validation {
        field: "sidecar".to_string(),
        msg: format!(
            "optimizer requires the Cline sidecar (WU-6: LlmDispatch was retired): {e} \
             — ensure XVN_AGENTD_BIN is set and the sidecar is provisioned"
        ),
    })?
    .ok_or_else(|| DashboardError::Validation {
        field: "sidecar".to_string(),
        msg: "optimizer requires the Cline sidecar (WU-6): XVN_AGENTD_BIN must be set \
              and the sidecar must be provisioned"
            .to_string(),
    })?;
    tokio::spawn(async move {
        // The production paper tester: real cached-backtest Executor, metered at
        // the dispatch boundary, with the shared per-cycle meter feeding both the
        // (currently unbounded) budget gate and the persisted cost summary.
        let cached = CachedBacktestPaperTester::new(
            api_ctx,
            backtest_dispatch,
            Arc::new(ToolRegistry::default_with_builtins()),
        )
        .with_cline_runtime(AgentRuntime::Cline, Some(cline_ctx));
        // F28: enforce the operator-set budget ceiling. Once the metered
        // paper-test cost reaches `budget_cap`, the cycle stops before launching
        // another backtest (no cap → f64::INFINITY). This is the guard against the
        // runaway token spew an unbounded UI cycle could produce.
        let paper_tester: Box<dyn PaperTestRunner> = Box::new(BudgetCappedPaperTester::new_with_handle(
            Box::new(cached),
            budget_cap,
            Arc::clone(&meter),
        ));

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
                let _ = persist_cycle_cost(&ticker_pool, &ticker_cycle_id, &totals, &Utc::now().to_rfc3339())
                    .await;
            }
        });

        // Persist cycle events best-effort via an unbounded channel so the
        // sync progress callback never blocks. The persister task drains the
        // receiver; dropping `persist_tx` (end of `run_cycle`) signals it to exit.
        let (persist_tx, mut persist_rx) = tokio::sync::mpsc::unbounded_channel::<CycleProgressEvent>();
        let persist_pool = pool.clone();
        tokio::spawn(async move {
            while let Some(ev) = persist_rx.recv().await {
                persist_progress_event(&persist_pool, &ev).await;
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
                let _ = persist_tx.send(ev.clone());
                let _ = tx.send(ev);
            },
            // DSPy in-loop: `Some` when `dspy_enabled = true` and the store
            // opened successfully; `None` otherwise (operator opt-in default off).
            cycle_dspy_ctx.as_ref(),
            // Cortex memory: optimizer recall/record (default ON, config-backed).
            cycle_memory.as_deref(),
            Some(cycle_id.clone()),
            Some(cancel_flag),
            // P4: cooperative pause flag so `POST /cycles/:id/pause` can suspend
            // and `POST /cycles/:id/resume` can continue.
            Some(pause_flag),
        )
        .await;
        // Stop the ticker; we persist the final totals below.
        ticker.abort();
        // F28: drop the cancel flag now the cycle is finished.
        state_for_dereg.autooptimizer_deregister_cancel(&cycle_id);
        // P4: drop the pause flag now the cycle is finished.
        state_for_dereg.autooptimizer_deregister_pause(&cycle_id);
        // Stop the heartbeat ticker and drop the heartbeat entry.
        heartbeat.abort();
        state_for_dereg.autooptimizer_deregister_heartbeat(&cycle_id);
        // F34: release the workspace cycle lock so the next cycle can run.
        let _ = xvision_engine::autooptimizer::run_lock::release(&pool, &cycle_id).await;
        // F23/F26/F35: persist the FINAL per-cycle tokens + cost so the panel and
        // `GET /api/autooptimizer/cycles/:id` show real spend after the run.
        // Best-effort — a failure here must not mask the completed cycle. Keyed by
        // the upfront `cycle_id` so this lands even if `run_cycle` errored.
        let totals = *meter.lock().expect("meter mutex poisoned");
        if let Err(e) = persist_cycle_cost(&pool, &cycle_id, &totals, &Utc::now().to_rfc3339()).await {
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
            session_id: None,
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
    // Check if the cycle's heartbeat is stale (process died without cleanup).
    if state.autooptimizer_is_stale(&cycle_id) {
        let _ = state
            .autooptimizer_tx
            .send(CycleProgressEvent::SessionStateChanged {
                session_id: cycle_id.clone(),
                state: "crashed".to_string(),
            });
        return Ok((
            StatusCode::GONE,
            Json(StartCycleResponse {
                started: false,
                message: format!(
                    "Cycle '{cycle_id}' has no heartbeat — it likely crashed. Flags cleaned up."
                ),
                session_id: None,
            }),
        ));
    }
    if state.autooptimizer_request_cancel(&cycle_id) {
        // P4: also clear the pause flag so a paused cycle wakes and sees the
        // cancel rather than looping forever waiting for resume.
        state.autooptimizer_request_resume(&cycle_id);
        Ok((
            StatusCode::ACCEPTED,
            Json(StartCycleResponse {
                started: false,
                message: format!(
                    "Cancellation requested for cycle {cycle_id}; it will stop before the next candidate."
                ),
                session_id: None,
            }),
        ))
    } else {
        Err(DashboardError::NotFound(format!(
            "no in-flight optimizer cycle '{cycle_id}'"
        )))
    }
}

/// P4: `POST /api/autooptimizer/cycles/:cycle_id/pause` — suspend an in-flight
/// optimizer cycle at its next safe checkpoint between candidates. Sets the pause
/// flag; the cycle will emit `SessionStateChanged { state: "paused" }` once it
/// reaches the checkpoint and suspends polling until resumed or cancelled.
///
/// Returns 200 if the pause flag was set, 409 if the cycle is not running
/// (i.e. no pause flag registry entry — cycle not in flight, already finished,
/// or already paused).
pub async fn pause_cycle(
    State(state): State<AppState>,
    Path(cycle_id): Path<String>,
) -> Result<(StatusCode, Json<StartCycleResponse>), DashboardError> {
    // 409 if already paused (flag already set) — idempotent re-pause is
    // confusing at the API boundary; callers should check state first.
    if state.autooptimizer_is_paused(&cycle_id) {
        return Err(DashboardError::Conflict(format!(
            "optimizer cycle '{cycle_id}' is already paused"
        )));
    }
    if state.autooptimizer_request_pause(&cycle_id) {
        Ok((
            StatusCode::OK,
            Json(StartCycleResponse {
                started: false,
                message: format!(
                    "Pause requested for cycle {cycle_id}; it will suspend before the next candidate."
                ),
                session_id: None,
            }),
        ))
    } else {
        Err(DashboardError::Conflict(format!(
            "optimizer cycle '{cycle_id}' is not running (not in flight or already finished)"
        )))
    }
}

/// P4: `POST /api/autooptimizer/cycles/:cycle_id/resume` — resume a paused
/// optimizer cycle. Clears the pause flag; the cycle poll loop will detect the
/// clear, emit `SessionStateChanged { state: "running" }`, and continue.
///
/// Returns 200 if the resume flag was cleared, 409 if the cycle is not currently
/// paused (not in flight, never paused, or already running).
pub async fn resume_cycle(
    State(state): State<AppState>,
    Path(cycle_id): Path<String>,
) -> Result<(StatusCode, Json<StartCycleResponse>), DashboardError> {
    // 409 if not paused — clearing an already-running flag is a no-op that
    // would silently look successful; explicit 409 surfaces the misuse.
    if !state.autooptimizer_is_paused(&cycle_id) {
        return Err(DashboardError::Conflict(format!(
            "optimizer cycle '{cycle_id}' is not paused (not in flight, never paused, or already running)"
        )));
    }
    if state.autooptimizer_request_resume(&cycle_id) {
        Ok((
            StatusCode::OK,
            Json(StartCycleResponse {
                started: false,
                message: format!(
                    "Resume requested for cycle {cycle_id}; it will continue from its pause checkpoint."
                ),
                session_id: None,
            }),
        ))
    } else {
        Err(DashboardError::Conflict(format!(
            "optimizer cycle '{cycle_id}' is not paused"
        )))
    }
}

pub(super) fn load_optimizer_config() -> Result<AutoOptimizerConfig, DashboardError> {
    load_optimizer_config_with_path().map(|(cfg, _, _)| cfg)
}

fn load_optimizer_config_with_path() -> Result<(AutoOptimizerConfig, std::path::PathBuf, bool), DashboardError>
{
    let path = AutoOptimizerConfig::default_path()?;
    let exists = path.exists();
    let cfg = if exists {
        AutoOptimizerConfig::load(&path)?
    } else {
        AutoOptimizerConfig::default()
    };
    Ok((cfg, path, exists))
}

pub(super) async fn build_autooptimizer_dispatch(
    provider: &str,
    model: &str,
    xvn_home: &std::path::Path,
) -> Result<Arc<dyn LlmDispatch + Send + Sync>, DashboardError> {
    let entry = load_autooptimizer_provider(provider, xvn_home).await?;
    validate_enabled_model(&entry, provider, model)?;
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(&entry.api_key_env).map_err(|_| DashboardError::Validation {
            field: "provider".into(),
            msg: format!("env var '{}' unset for provider '{provider}'", entry.api_key_env),
        })?
    };
    Ok(match entry.kind {
        ProviderKind::Anthropic => Arc::new(AnthropicDispatch::new(api_key)),
        ProviderKind::OpenaiCompat | ProviderKind::LlamaCpp | ProviderKind::Vllm => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::Ollama => {
            let base = entry.base_url.trim_end_matches('/');
            let url = if base.ends_with("/v1") {
                base.to_string()
            } else {
                format!("{base}/v1")
            };
            Arc::new(OpenaiCompatDispatch::new(url, api_key))
        }
        ProviderKind::LocalCandle => {
            return Err(DashboardError::Validation {
                field: "provider".into(),
                msg: "local-candle is not supported for the autooptimizer".into(),
            })
        }
    })
}

async fn validate_autooptimizer_model_allowlist(
    provider: &str,
    model: &str,
    xvn_home: &std::path::Path,
) -> Result<(), DashboardError> {
    let entry = load_autooptimizer_provider(provider, xvn_home).await?;
    validate_enabled_model(&entry, provider, model)
}

async fn load_autooptimizer_provider(
    provider: &str,
    xvn_home: &std::path::Path,
) -> Result<xvision_core::config::ProviderEntry, DashboardError> {
    let config_path = xvision_core::config::runtime_config_path(xvn_home);
    let provider_name = provider.to_owned();
    let rt = tokio::task::spawn_blocking(move || xvision_core::config::load_runtime(&config_path))
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("task join: {e}")))?
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("load runtime config: {e}")))?;
    rt.providers
        .into_iter()
        .find(|p| p.name == provider_name)
        .ok_or_else(|| DashboardError::Validation {
            field: "provider".into(),
            msg: format!("autooptimizer provider '{provider_name}' not configured in Settings -> Providers"),
        })
}

fn validate_enabled_model(
    entry: &xvision_core::config::ProviderEntry,
    provider: &str,
    model: &str,
) -> Result<(), DashboardError> {
    if !entry.enabled_models.is_empty() && !entry.enabled_models.iter().any(|m| m == model) {
        return Err(DashboardError::Validation {
            field: "model".into(),
            msg: format!(
                "model '{model}' is not in the enabled_models allowlist for provider \
                 '{provider}'; update the allowlist in Settings -> Providers"
            ),
        });
    }
    Ok(())
}

pub(super) fn build_mutator_and_judge(
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

pub(super) fn build_cycle_config(
    cfg: &AutoOptimizerConfig,
    judge: &Judge,
    day_scenario: Scenario,
    baseline_scenario: Scenario,
    scenario_pool: Vec<(Scenario, Scenario)>,
    parent_strategies: HashMap<String, Strategy>,
    explicit_parent_hashes: Vec<ContentHash>,
) -> CycleConfig {
    CycleConfig {
        num_parents: if explicit_parent_hashes.is_empty() {
            2
        } else {
            explicit_parent_hashes.len()
        },
        mutations_per_parent: cfg.experiments_per_cycle as usize,
        sabotage_seed: 42,
        judge_provider: judge.provider.clone(),
        judge_model: judge.model.clone(),
        prompt_version: "v1".into(),
        sustained_no_pass_cycles: 0,
        day_scenario,
        baseline_scenario,
        parent_strategies,
        explicit_parent_hashes,
        objective: cfg.objective,
        regime_set: cfg.regime_set.clone(),
        scenario_pool,
        // The dashboard cycle launcher does not (yet) surface a per-cycle
        // output-token cap; preserve prior behaviour (no cycle-level cap).
        max_output_tokens: None,
        max_consecutive_errors: 3,
    }
}

/// B19: synthesize the round-robin `scenario_pool` for the dashboard cycle
/// launcher. One `(day, baseline)` pair per configured `ScenarioWindowPair`,
/// built through the same shared optimizer scenario builders as the single
/// pair. Empty when `scenario_pool` is unset (back-compat).
pub(super) fn build_scenario_pool(
    cfg: &AutoOptimizerConfig,
    cadence_minutes: u32,
) -> Result<Vec<(Scenario, Scenario)>, DashboardError> {
    cfg.scenario_pool
        .iter()
        .map(|pair| {
            let day = synthesize_optimizer_day_scenario(&pair.day, cadence_minutes, "xvn-dashboard");
            let baseline = synthesize_baseline_untouched_scenario(&day, &pair.baseline).map_err(|e| {
                DashboardError::Validation {
                    field: "scenario_pool".into(),
                    msg: format!("synthesize scenario_pool '{}' baseline: {e}", pair.label),
                }
            })?;
            Ok((day, baseline))
        })
        .collect()
}

pub(super) async fn load_strategy_parent(
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

/// Resolve the strategy's trader to an effective (provider, model) pair for
/// the paper-test backtest. Returns `None` for mechanistic strategies or when
/// the trader cannot be resolved (the normal run path will surface that error).
async fn resolve_paper_test_provider(
    pool: &sqlx::SqlitePool,
    strategy: &Strategy,
) -> Option<(String, String)> {
    if strategy.decision_mode == xvision_engine::strategies::DecisionMode::Mechanistic {
        return None;
    }
    if !strategy.agents.is_empty() {
        if let Ok(slots) =
            xvision_engine::agent::pipeline::resolve_agent_slots_for_strategy(pool, strategy).await
        {
            for rs in &slots {
                let model = rs.slot.effective_model();
                if let Some(p) = infer_trader_provider(
                    rs.slot.provider.as_deref().unwrap_or(""),
                    &model,
                ) {
                    return Some((p, model));
                }
            }
        }
    } else if let Some(slot) = &strategy.trader_slot {
        if slot.has_model_binding() {
            let model = slot.effective_model();
            if let Some(p) = infer_trader_provider(
                slot.provider.as_deref().unwrap_or(""),
                &model,
            ) {
                return Some((p, model));
            }
        }
    }
    None
}

pub(super) fn build_day_scenario(
    cfg: &AutoOptimizerConfig,
    cadence_minutes: u32,
) -> Result<Scenario, DashboardError> {
    // F10: delegate to the single shared optimizer scenario builder so the
    // dashboard and CLI never drift on venue/fee/fill settings.
    // B9: granularity follows the seed strategy's decision cadence.
    Ok(synthesize_optimizer_day_scenario(
        &cfg.day_window,
        cadence_minutes,
        "xvn-dashboard",
    ))
}

/// Load the cached provider catalog for `provider` so the metering dispatch can
/// price each LLM completion. Best-effort: an absent/unreadable catalog yields no
/// pricing (calls are counted as "unpriced", never silently $0) — the same
/// "unknown ≠ zero" stance the CLI uses.
pub(super) async fn load_metering_catalogs(
    xvn_home: &std::path::Path,
    provider: &str,
) -> Vec<Arc<xvision_core::providers::Catalog>> {
    match xvision_engine::providers::load_cached_catalog(xvn_home, provider).await {
        Ok(Some(cat)) => vec![Arc::new(cat)],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use xvision_engine::agent::llm::MockDispatch;

    fn provider_with_enabled_models(models: Vec<&str>) -> xvision_core::config::ProviderEntry {
        xvision_core::config::ProviderEntry {
            name: "ollama".into(),
            kind: xvision_core::config::ProviderKind::Ollama,
            base_url: "http://localhost:11434".into(),
            api_key_env: String::new(),
            enabled_models: models.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn autooptimizer_allowlist_accepts_enabled_model() {
        let entry = provider_with_enabled_models(vec!["lfm2.5:8b"]);
        validate_enabled_model(&entry, "ollama", "lfm2.5:8b").expect("enabled model passes");
    }

    #[test]
    fn autooptimizer_allowlist_rejects_disabled_model() {
        let entry = provider_with_enabled_models(vec!["lfm2.5:8b"]);
        let err = validate_enabled_model(&entry, "ollama", "gemma4:26b-mlx")
            .expect_err("disabled model should fail");
        let msg = format!("{err}");
        assert!(msg.contains("gemma4:26b-mlx"), "error should name model: {msg}");
        assert!(
            msg.contains("enabled_models"),
            "error should name allowlist: {msg}"
        );
    }

    #[test]
    fn autooptimizer_allowlist_empty_list_preserves_legacy_passthrough() {
        let entry = provider_with_enabled_models(vec![]);
        validate_enabled_model(&entry, "ollama", "any-model").expect("empty allowlist passes");
    }

    // ── P4 pause/resume/cancel flag tests ─────────────────────────────────────

    /// P4: `autooptimizer_register_pause` / `autooptimizer_request_pause` /
    /// `autooptimizer_is_paused` / `autooptimizer_request_resume` /
    /// `autooptimizer_deregister_pause` — flag lifecycle.
    #[test]
    fn test_pause_flag_lifecycle() {
        use std::collections::HashMap;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};

        // Manually replicate the pause registry logic (mirrors AppState internals)
        // so we can test it without a full DB bootstrap.
        let pauses: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::new(Mutex::new(HashMap::new()));

        let register = |id: &str| -> Arc<AtomicBool> {
            let flag = Arc::new(AtomicBool::new(false));
            pauses.lock().unwrap().insert(id.to_string(), Arc::clone(&flag));
            flag
        };
        let request_pause = |id: &str| -> bool {
            pauses
                .lock()
                .unwrap()
                .get(id)
                .map(|f| {
                    f.store(true, Ordering::Relaxed);
                    true
                })
                .unwrap_or(false)
        };
        let is_paused = |id: &str| -> bool {
            pauses
                .lock()
                .unwrap()
                .get(id)
                .map(|f| f.load(Ordering::Relaxed))
                .unwrap_or(false)
        };
        let request_resume = |id: &str| -> bool {
            pauses
                .lock()
                .unwrap()
                .get(id)
                .map(|f| {
                    f.store(false, Ordering::Relaxed);
                    true
                })
                .unwrap_or(false)
        };
        let deregister = |id: &str| {
            pauses.lock().unwrap().remove(id);
        };

        let cycle = "cycle-001";

        // Not registered → request_pause returns false, is_paused returns false.
        assert!(!request_pause(cycle), "pause on unregistered cycle returns false");
        assert!(!is_paused(cycle), "is_paused on unregistered cycle returns false");

        // Register the cycle.
        let _flag = register(cycle);
        assert!(!is_paused(cycle), "newly registered cycle is not paused");

        // Pause it.
        assert!(request_pause(cycle), "pause on registered cycle returns true");
        assert!(is_paused(cycle), "is_paused after pause returns true");

        // Resume it.
        assert!(request_resume(cycle), "resume on paused cycle returns true");
        assert!(!is_paused(cycle), "is_paused after resume returns false");

        // Deregister.
        deregister(cycle);
        assert!(!is_paused(cycle), "is_paused after deregister returns false");
        assert!(!request_pause(cycle), "pause after deregister returns false");
    }

    /// P4: cancelling a paused cycle must also clear the pause flag so the cycle
    /// wakes up, observes the cancel, and terminates.
    #[test]
    fn test_cancel_clears_pause_flag() {
        use std::collections::HashMap;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};

        let cancels: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::new(Mutex::new(HashMap::new()));
        let pauses: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::new(Mutex::new(HashMap::new()));

        let cycle = "cycle-002";

        // Register both flags.
        let cancel_flag = Arc::new(AtomicBool::new(false));
        cancels
            .lock()
            .unwrap()
            .insert(cycle.to_string(), Arc::clone(&cancel_flag));
        let pause_flag = Arc::new(AtomicBool::new(false));
        pauses
            .lock()
            .unwrap()
            .insert(cycle.to_string(), Arc::clone(&pause_flag));

        // Pause the cycle.
        pauses
            .lock()
            .unwrap()
            .get(cycle)
            .unwrap()
            .store(true, Ordering::Relaxed);
        assert!(
            pause_flag.load(Ordering::Relaxed),
            "cycle is paused before cancel"
        );

        // Simulate cancel_cycle: set cancel + clear pause.
        cancels
            .lock()
            .unwrap()
            .get(cycle)
            .unwrap()
            .store(true, Ordering::Relaxed);
        pauses
            .lock()
            .unwrap()
            .get(cycle)
            .unwrap()
            .store(false, Ordering::Relaxed);

        assert!(
            cancel_flag.load(Ordering::Relaxed),
            "cancel flag is set after cancel"
        );
        assert!(
            !pause_flag.load(Ordering::Relaxed),
            "pause flag is cleared after cancel"
        );
    }

    /// P4: pause returns 409 (not-running) when no pause flag is registered
    /// (simulates idle / finished cycle). We test this via the conflict detection
    /// logic: `autooptimizer_request_pause` returns false → Conflict.
    #[test]
    fn test_pause_route_409_when_not_in_flight() {
        use std::collections::HashMap;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};

        let pauses: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::new(Mutex::new(HashMap::new()));

        // No flag registered for this cycle_id.
        let cycle = "cycle-finished";
        let found = pauses
            .lock()
            .unwrap()
            .get(cycle)
            .map(|f| {
                f.store(true, std::sync::atomic::Ordering::Relaxed);
                true
            })
            .unwrap_or(false);

        // Route would return Conflict when found == false.
        assert!(
            !found,
            "pause on non-in-flight cycle returns false (triggers 409)"
        );
    }

    /// P4: resume returns 409 (not paused) when the pause flag is not set.
    #[test]
    fn test_resume_route_409_not_paused() {
        use std::collections::HashMap;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};

        let pauses: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::new(Mutex::new(HashMap::new()));

        let cycle = "cycle-running";

        // Register with pause=false (cycle is running, not paused).
        let flag = Arc::new(AtomicBool::new(false));
        pauses
            .lock()
            .unwrap()
            .insert(cycle.to_string(), Arc::clone(&flag));

        let is_paused = pauses
            .lock()
            .unwrap()
            .get(cycle)
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false);

        // Route guards on `is_paused` before issuing resume; false → 409.
        assert!(!is_paused, "cycle not paused → resume should be rejected (409)");
    }

    // ── existing test ─────────────────────────────────────────────────────────

    #[test]
    fn build_cycle_config_uses_resolved_judge_provider() {
        let cfg = AutoOptimizerConfig::default();
        let day_scenario = synthesize_optimizer_day_scenario(&cfg.day_window, 60, "xvn-dashboard-test");
        let baseline_scenario =
            synthesize_baseline_untouched_scenario(&day_scenario, &cfg.baseline_untouched_window)
                .expect("baseline scenario");
        let judge = Judge {
            dispatch: Arc::new(MockDispatch::echo("ok")) as Arc<dyn LlmDispatch + Send + Sync>,
            provider: "ollama".into(),
            model: "qwen2.5-coder:7b".into(),
        };

        let cycle = build_cycle_config(
            &cfg,
            &judge,
            day_scenario,
            baseline_scenario,
            Vec::new(),
            HashMap::new(),
            Vec::new(),
        );

        assert_eq!(cycle.judge_provider, "ollama");
        assert_eq!(cycle.judge_model, "qwen2.5-coder:7b");
        // B19: default config has no scenario_pool ⇒ empty pool (single-pair path).
        assert!(
            cycle.scenario_pool.is_empty(),
            "default config must yield an empty scenario_pool (back-compat)"
        );
    }

    fn strategy_with_cadence(cadence_minutes: u32) -> Strategy {
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000A",
                "display_name": "Cadence Test Strategy",
                "plain_summary": "Strategy for scenario-granularity wiring test.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": cadence_minutes,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000A", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            }
        });
        serde_json::from_value(v).expect("fixture strategy must deserialise")
    }

    /// B9 wiring guard: the reordered route path derives the scenario granularity
    /// from the seed strategy's decision cadence via `build_day_scenario`. A 15m
    /// strategy must yield a 15m day scenario AND a 15m baseline — not the old
    /// hardcoded 1h. This exercises the live helper, so an orphaned-helper
    /// regression (builder edited but caller still hardcoded) fails here.
    #[test]
    fn build_day_scenario_follows_seed_strategy_cadence() {
        use xvision_engine::eval::scenario::BarGranularity;

        let cfg = AutoOptimizerConfig::default();
        let strategy = strategy_with_cadence(15);
        // Mirror exactly what the route does: cadence from the loaded strategy.
        let cadence_minutes = strategy.manifest.decision_cadence_minutes;
        let day_scenario = build_day_scenario(&cfg, cadence_minutes).expect("day scenario");
        assert_eq!(
            day_scenario.granularity,
            BarGranularity::Minute15,
            "15m strategy must produce a 15m day scenario"
        );
        let baseline = synthesize_baseline_untouched_scenario(&day_scenario, &cfg.baseline_untouched_window)
            .expect("baseline scenario");
        assert_eq!(
            baseline.granularity,
            BarGranularity::Minute15,
            "baseline must inherit the 15m granularity"
        );
    }
}

// =============================================================================
// P5-W2: Schedule CRUD — GET/POST /api/autooptimizer/schedule,
//         DELETE /api/autooptimizer/schedule/:id
// =============================================================================

use xvision_engine::autooptimizer::scheduler::OptimizerSchedule;

/// Request body for POST /api/autooptimizer/schedule.
#[derive(serde::Deserialize)]
pub struct UpsertScheduleBody {
    pub enabled: Option<bool>,
    /// "HH:MM" — local-time-of-day when the optimizer should fire.
    pub time_local: String,
    pub strategy_id: String,
    /// JSON config forwarded to `create_session` as `config_json`.
    pub config_json: Option<String>,
}

/// GET /api/autooptimizer/schedule — return the first schedule row or `null`.
pub async fn get_schedule(
    State(state): State<AppState>,
) -> Result<Json<Option<OptimizerSchedule>>, DashboardError> {
    let row = sqlx::query_as::<_, OptimizerSchedule>(
        "SELECT id, enabled, time_local, strategy_id, config_json, last_run_at, next_run_at \
         FROM autooptimizer_schedules LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(row))
}

/// POST /api/autooptimizer/schedule — upsert by strategy_id (one per strategy, v1 limit).
/// Returns the upserted schedule.
pub async fn upsert_schedule(
    State(state): State<AppState>,
    Json(body): Json<UpsertScheduleBody>,
) -> Result<Json<OptimizerSchedule>, DashboardError> {
    let enabled = body.enabled.unwrap_or(true);
    let config_json = body.config_json.unwrap_or_else(|| "{}".to_string());

    // Validate time_local format "HH:MM".
    let parts: Vec<&str> = body.time_local.split(':').collect();
    if parts.len() != 2
        || parts[0].parse::<u32>().map(|h| h > 23).unwrap_or(true)
        || parts[1].parse::<u32>().map(|m| m > 59).unwrap_or(true)
    {
        return Err(DashboardError::Validation {
            field: "time_local".into(),
            msg: "time_local must be in HH:MM format (00:00–23:59)".into(),
        });
    }

    // Upsert: if a schedule for this strategy_id exists, update it; otherwise insert.
    sqlx::query(
        "INSERT INTO autooptimizer_schedules \
         (enabled, time_local, strategy_id, config_json) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(strategy_id) DO UPDATE SET \
           enabled = excluded.enabled, \
           time_local = excluded.time_local, \
           config_json = excluded.config_json",
    )
    .bind(enabled as i64)
    .bind(&body.time_local)
    .bind(&body.strategy_id)
    .bind(&config_json)
    .execute(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!(e)))?;

    // Fetch the upserted row.
    let row = sqlx::query_as::<_, OptimizerSchedule>(
        "SELECT id, enabled, time_local, strategy_id, config_json, last_run_at, next_run_at \
         FROM autooptimizer_schedules WHERE strategy_id = ?",
    )
    .bind(&body.strategy_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!(e)))?;

    Ok(Json(row))
}

/// DELETE /api/autooptimizer/schedule/:id — delete by id.
pub async fn delete_schedule(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, DashboardError> {
    let result = sqlx::query("DELETE FROM autooptimizer_schedules WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!(e)))?;

    if result.rows_affected() == 0 {
        return Err(DashboardError::NotFound(format!("no schedule with id {id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ── Strategy Inspector endpoints (unified optimizer plan) ────────────────────

/// GET /api/optimizer/strategy/:hash
/// Returns the strategy JSON from the blob store by content hash.
pub async fn get_optimizer_strategy_blob(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    let content_hash = ContentHash::from_hex(&hash)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("invalid hash {hash}: {e}")))?;
    let blob_dir = state.xvn_home.join("lineage").join("blobs");
    let blobs = BlobStore::new(blob_dir);
    let json = blobs
        .get_json(&content_hash)
        .await
        .map_err(|_| DashboardError::NotFound(format!("strategy {hash} not in blob store")))?;
    Ok(Json(json))
}

#[derive(Serialize)]
pub struct OriginDiffResponse {
    pub origin_hash: String,
    pub diff: xvision_engine::autooptimizer::mutator::StrategyDiff,
}

/// GET /api/optimizer/strategy/:hash/diff/origin
/// Walks the lineage chain back to the root node, computes a structural diff.
pub async fn get_strategy_origin_diff(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<OriginDiffResponse>, DashboardError> {
    let blob_dir = state.xvn_home.join("lineage").join("blobs");
    let blobs = BlobStore::new(blob_dir);
    let lineage = LineageStore::new(state.pool.clone());

    // Walk the lineage chain back to root (node with no parent).
    let mut origin_hash_hex = hash.clone();
    let mut current_hex = hash.clone();
    let mut depth = 0u32;
    loop {
        if depth > 1000 {
            break; // safety valve against cycles
        }
        let current_ch = ContentHash::from_hex(&current_hex)
            .map_err(|e| DashboardError::Internal(anyhow::anyhow!("bad hash in chain: {e}")))?;
        let node = lineage
            .get(&current_ch)
            .await
            .map_err(|e| DashboardError::Internal(anyhow::anyhow!("lineage lookup: {e}")))?;
        match node {
            None => break, // not in lineage — treat current as root
            Some(n) => match n.parent_hash {
                None => {
                    origin_hash_hex = current_hex.clone();
                    break;
                }
                Some(ph) => {
                    origin_hash_hex = current_hex.clone();
                    current_hex = ph.to_hex();
                    depth += 1;
                }
            },
        }
    }

    let origin_ch = ContentHash::from_hex(&origin_hash_hex)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("bad origin hash: {e}")))?;
    let current_ch = ContentHash::from_hex(&hash)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("bad current hash: {e}")))?;

    let origin_json = blobs
        .get_json(&origin_ch)
        .await
        .map_err(|_| DashboardError::NotFound(format!("origin blob {origin_hash_hex} not found")))?;
    let current_json = blobs
        .get_json(&current_ch)
        .await
        .map_err(|_| DashboardError::NotFound(format!("strategy blob {hash} not found")))?;

    // Deserialize and compute structural diff.
    let origin_strategy: Strategy = serde_json::from_value(origin_json)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("deserialize origin strategy: {e}")))?;
    let current_strategy: Strategy = serde_json::from_value(current_json)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("deserialize current strategy: {e}")))?;

    let diff = xvision_engine::autooptimizer::mutator::strategy_diff(&origin_strategy, &current_strategy);

    Ok(Json(OriginDiffResponse {
        origin_hash: origin_hash_hex,
        diff,
    }))
}

#[derive(Serialize)]
pub struct PromoteStrategyResponse {
    pub strategy_id: String,
}

/// POST /api/optimizer/strategy/:hash/promote
/// Saves a blob-store strategy to the filesystem strategies folder.
/// Idempotent: if a strategy with this candidate_id already exists, returns its id.
pub async fn promote_strategy(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<PromoteStrategyResponse>, DashboardError> {
    let content_hash = ContentHash::from_hex(&hash)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("invalid hash {hash}: {e}")))?;
    let blob_dir = state.xvn_home.join("lineage").join("blobs");
    let blobs = BlobStore::new(blob_dir);
    let strategy_json = blobs
        .get_json(&content_hash)
        .await
        .map_err(|_| DashboardError::NotFound(format!("strategy {hash} not in blob store")))?;

    let mut strategy: Strategy = serde_json::from_value(strategy_json)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("deserialize strategy: {e}")))?;

    // Stable candidate id from the first 8 chars of the hash.
    let hash_prefix = &hash[..hash.len().min(8)];
    let candidate_id = format!("opt-{hash_prefix}");
    let display_name = format!("optimizer-candidate-{hash_prefix}");

    let store_dir = strategy_store_dir(&state.xvn_home);
    let store = FilesystemStore::new(store_dir);

    // Idempotency: return existing id if already promoted.
    if store.load(&candidate_id).await.is_ok() {
        return Ok(Json(PromoteStrategyResponse {
            strategy_id: candidate_id,
        }));
    }

    // Stamp the strategy with the candidate id and display name.
    strategy.manifest.id = candidate_id.clone();
    strategy.manifest.display_name = display_name;

    store
        .save(&strategy)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("save promoted strategy: {e}")))?;

    Ok(Json(PromoteStrategyResponse {
        strategy_id: candidate_id,
    }))
}

/// One row from `autooptimizer_events`, returned by the cycle-replay endpoint.
#[derive(Serialize, sqlx::FromRow)]
pub struct PersistedCycleEvent {
    pub seq: i64,
    pub session_id: String,
    pub cycle_id: Option<String>,
    pub kind: String,
    pub payload_json: String,
    pub ts: String,
}

/// GET /api/autooptimizer/cycles/:cycle_id/events
///
/// Replay source for the ConsoleModule's idle state: the persisted event log
/// of a completed cycle, oldest-first. Absent table (fresh install) → empty
/// list, never an error — "no events yet" is a designed product state.
pub async fn get_cycle_events(
    Path(cycle_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<PersistedCycleEvent>>, DashboardError> {
    if !table_exists(&state.pool, "autooptimizer_events").await? {
        return Ok(Json(Vec::new()));
    }
    let events: Vec<PersistedCycleEvent> = sqlx::query_as(
        "SELECT seq, session_id, cycle_id, kind, payload_json, ts
         FROM autooptimizer_events WHERE cycle_id = ?1 ORDER BY seq ASC LIMIT 1000",
    )
    .bind(&cycle_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))?;
    Ok(Json(events))
}

#[cfg(test)]
mod schedule_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn open_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();

        // Migration 059
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS autooptimizer_schedules (
              id           INTEGER PRIMARY KEY AUTOINCREMENT,
              enabled      INTEGER NOT NULL DEFAULT 1,
              time_local   TEXT NOT NULL,
              strategy_id  TEXT NOT NULL UNIQUE,
              config_json  TEXT NOT NULL,
              last_run_at  TEXT,
              next_run_at  TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    // ── test_get_schedule_empty ──────────────────────────────────────────────

    /// GET schedule when no rows exist → returns null (None).
    #[tokio::test]
    async fn test_get_schedule_empty() {
        let pool = open_pool().await;

        let row: Option<OptimizerSchedule> = sqlx::query_as(
            "SELECT id, enabled, time_local, strategy_id, config_json, last_run_at, next_run_at \
             FROM autooptimizer_schedules LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();

        assert!(row.is_none(), "expected null response when no schedules exist");
    }

    // ── test_post_schedule_upserts ───────────────────────────────────────────

    /// POST then POST again with the same strategy_id → only one row (upserted).
    #[tokio::test]
    async fn test_post_schedule_upserts() {
        let pool = open_pool().await;

        // First insert.
        sqlx::query(
            "INSERT INTO autooptimizer_schedules \
             (enabled, time_local, strategy_id, config_json) VALUES (1, '09:00', 'strat-1', '{}')\
             ON CONFLICT(strategy_id) DO UPDATE SET \
               enabled = excluded.enabled, time_local = excluded.time_local, config_json = excluded.config_json",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Second insert (same strategy_id, different time).
        sqlx::query(
            "INSERT INTO autooptimizer_schedules \
             (enabled, time_local, strategy_id, config_json) VALUES (1, '14:30', 'strat-1', '{}')\
             ON CONFLICT(strategy_id) DO UPDATE SET \
               enabled = excluded.enabled, time_local = excluded.time_local, config_json = excluded.config_json",
        )
        .execute(&pool)
        .await
        .unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_schedules")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(count, 1, "upsert should keep exactly one row per strategy_id");

        // Verify the time was updated to the second value.
        let time: String = sqlx::query_scalar(
            "SELECT time_local FROM autooptimizer_schedules WHERE strategy_id = 'strat-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(time, "14:30", "upsert should update time_local to latest value");
    }

    // ── test_delete_schedule ─────────────────────────────────────────────────

    /// POST then DELETE → row removed.
    #[tokio::test]
    async fn test_delete_schedule() {
        let pool = open_pool().await;

        // Insert a row.
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO autooptimizer_schedules \
             (enabled, time_local, strategy_id, config_json) VALUES (1, '08:00', 'strat-del', '{}') \
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        // Confirm it exists.
        let count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_schedules")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count_before, 1);

        // Delete it.
        let result = sqlx::query("DELETE FROM autooptimizer_schedules WHERE id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(result.rows_affected(), 1, "expected one row deleted");

        let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_schedules")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count_after, 0, "schedule should be gone after delete");
    }

    // ── test_delete_nonexistent ──────────────────────────────────────────────

    /// DELETE on nonexistent id → rows_affected == 0 (maps to 404 in handler).
    #[tokio::test]
    async fn test_delete_nonexistent_schedule() {
        let pool = open_pool().await;

        let result = sqlx::query("DELETE FROM autooptimizer_schedules WHERE id = ?")
            .bind(9999_i64)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            result.rows_affected(),
            0,
            "deleting nonexistent schedule should affect 0 rows"
        );
    }
}

#[cfg(test)]
mod persist_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn open_pool() -> sqlx::SqlitePool {
        SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap()
    }

    /// Creates the autooptimizer_events table (mirrors migration 057 DDL exactly).
    /// Named `create_events_table` so Task 1 can reuse it.
    pub(super) async fn create_events_table(pool: &sqlx::SqlitePool) {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS autooptimizer_events (
                seq          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id   TEXT NOT NULL,
                cycle_id     TEXT,
                kind         TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                ts           TEXT NOT NULL
            )",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// `persist_progress_event` on a MutationGated event inserts one row with
    /// kind = "mutation_gated_passed", correct cycle_id, and payload_json = full JSON.
    #[tokio::test]
    async fn test_persist_progress_event_mutation_gated() {
        let pool = open_pool().await;
        create_events_table(&pool).await;

        // Construct event via serde_json so adding new fields (Task 0a) doesn't break this.
        let ev: xvision_engine::autooptimizer::progress::CycleProgressEvent = serde_json::from_str(
            r#"{
                    "type": "mutation_gated",
                    "session_id": "sess-1",
                    "cycle_id": "cyc-1",
                    "child_hash": "abc123",
                    "passed": true,
                    "outcome": "kept"
                }"#,
        )
        .unwrap();

        persist_progress_event(&pool, &ev).await;

        let row: (String, Option<String>, String) =
            sqlx::query_as("SELECT kind, cycle_id, payload_json FROM autooptimizer_events LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(
            row.0, "mutation_gated_passed",
            "kind should be event_kind() output"
        );
        assert_eq!(row.1.as_deref(), Some("cyc-1"), "cycle_id should be set");
        // payload_json must be valid JSON containing the event fields
        let payload: serde_json::Value = serde_json::from_str(&row.2).unwrap();
        assert_eq!(payload["type"], "mutation_gated");
        assert_eq!(payload["cycle_id"], "cyc-1");
    }

    /// A SessionStateChanged event (no cycle_id field) persists with cycle_id = NULL.
    #[tokio::test]
    async fn test_persist_progress_event_session_state_changed() {
        let pool = open_pool().await;
        create_events_table(&pool).await;

        let ev: xvision_engine::autooptimizer::progress::CycleProgressEvent = serde_json::from_str(
            r#"{"type":"session_state_changed","session_id":"sess-2","state":"running"}"#,
        )
        .unwrap();

        persist_progress_event(&pool, &ev).await;

        let row: (String, Option<String>) =
            sqlx::query_as("SELECT kind, cycle_id FROM autooptimizer_events LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(row.0, "session_state_changed");
        assert!(row.1.is_none(), "cycle_id should be NULL for SessionStateChanged");
    }

    /// Dashboard-launched cycles: `run_cycle` emits events with an EMPTY
    /// session_id. Persisting them under "" would orphan the rows —
    /// `prune_old_events` keeps only session_ids present in
    /// `autooptimizer_session_state`, so "" rows would be silently deleted on
    /// the next prune. The fallback key `cycle:<cycle_id>` keeps them
    /// attributable and prune-safe (prune retains `cycle:`-prefixed keys).
    #[tokio::test]
    async fn test_persist_progress_event_empty_session_uses_cycle_fallback() {
        let pool = open_pool().await;
        create_events_table(&pool).await;

        let ev: xvision_engine::autooptimizer::progress::CycleProgressEvent = serde_json::from_str(
            r#"{
                    "type": "mutation_gated",
                    "session_id": "",
                    "cycle_id": "cyc-dash-1",
                    "child_hash": "abc123",
                    "passed": true,
                    "outcome": "kept"
                }"#,
        )
        .unwrap();

        persist_progress_event(&pool, &ev).await;

        let row: (String, Option<String>) =
            sqlx::query_as("SELECT session_id, cycle_id FROM autooptimizer_events LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(
            row.0, "cycle:cyc-dash-1",
            "empty session_id must persist under the cycle: fallback key"
        );
        assert_eq!(row.1.as_deref(), Some("cyc-dash-1"));
    }

    /// A non-empty session_id is stored verbatim (no fallback rewrite).
    #[tokio::test]
    async fn test_persist_progress_event_keeps_real_session_id() {
        let pool = open_pool().await;
        create_events_table(&pool).await;

        let ev: xvision_engine::autooptimizer::progress::CycleProgressEvent = serde_json::from_str(
            r#"{"type":"session_state_changed","session_id":"sess-9","state":"running"}"#,
        )
        .unwrap();

        persist_progress_event(&pool, &ev).await;

        let session_id: String = sqlx::query_scalar("SELECT session_id FROM autooptimizer_events LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(session_id, "sess-9");
    }
}

#[cfg(test)]
mod cycle_events_tests {
    use super::*;
    use axum::extract::{Path, State};
    use tempfile::TempDir;

    /// Spin up a fresh `AppState` backed by a temp dir.
    /// Mirrors the `fresh_state` pattern in `routes/agent_runs.rs`.
    /// Note: `AppState::new` calls `ApiContext::open` which runs engine migrations,
    /// including `migrate_autooptimizer_sessions` — so the pool already has
    /// `autooptimizer_events` after this returns.
    async fn fresh_state() -> (crate::state::AppState, TempDir) {
        let tmp = TempDir::new().unwrap();
        let xvn_home = tmp.path().to_path_buf();
        std::fs::create_dir_all(xvn_home.join("config")).unwrap();
        let cfg =
            std::fs::read_to_string("../../config/default.toml").expect("read workspace config/default.toml");
        std::fs::write(xvn_home.join("config/default.toml"), cfg).unwrap();
        let state = crate::state::AppState::new(xvn_home)
            .await
            .expect("AppState::new");
        (state, tmp)
    }

    // ── test_get_cycle_events_returns_ordered_events ──────────────────────────

    /// Seeding 5 rows (4 for cyc-1, 1 for cyc-2) must return exactly the 4
    /// rows for cyc-1 in seq order.
    #[tokio::test]
    async fn test_get_cycle_events_returns_ordered_events() {
        let (state, _tmp) = fresh_state().await;
        // `fresh_state` already creates the table via engine migrations.
        super::persist_tests::create_events_table(&state.pool).await;

        for (kind, cycle) in [
            ("cycle_started", "cyc-1"),
            ("mutation_proposed", "cyc-1"),
            ("mutation_gated", "cyc-1"),
            ("cycle_started", "cyc-2"), // other cycle — must be filtered out
            ("cycle_finished", "cyc-1"),
        ] {
            sqlx::query(
                "INSERT INTO autooptimizer_events (session_id, cycle_id, kind, payload_json, ts)
                 VALUES ('sess-1', ?1, ?2, '{}', '2026-06-11T00:00:00Z')",
            )
            .bind(cycle)
            .bind(kind)
            .execute(&state.pool)
            .await
            .unwrap();
        }

        let resp = get_cycle_events(Path("cyc-1".into()), State(state))
            .await
            .unwrap();
        let events = resp.0;
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].kind, "cycle_started");
        assert_eq!(events[3].kind, "cycle_finished");
        assert!(events.windows(2).all(|w| w[0].seq < w[1].seq));
    }

    // ── test_get_cycle_events_missing_table_returns_empty ─────────────────────

    /// When `autooptimizer_events` does not exist (fresh install / pre-migration
    /// DB), the handler must return an empty list — never an error.
    ///
    /// `fresh_state` runs engine migrations which CREATE the table, so we must
    /// explicitly DROP it afterward to exercise the missing-table guard branch in
    /// `get_cycle_events`.
    #[tokio::test]
    async fn test_get_cycle_events_missing_table_returns_empty() {
        let (state, _tmp) = fresh_state().await;
        // Drop the table so the guard branch is genuinely exercised.
        sqlx::query("DROP TABLE IF EXISTS autooptimizer_events")
            .execute(&state.pool)
            .await
            .unwrap();
        let resp = get_cycle_events(Path("cyc-x".into()), State(state))
            .await
            .unwrap();
        assert!(resp.0.is_empty());
    }
}

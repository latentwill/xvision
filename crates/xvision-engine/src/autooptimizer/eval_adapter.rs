//! Bridges autooptimizer paper-test calls to the eval engine's backtest executor.
//! The `PaperTestRunner` trait decouples the autooptimizer orchestrator from
//! the eval engine so tests can substitute a deterministic `StubPaperTester`.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use xvision_core::market::Ohlcv;
use xvision_core::trading::AssetSymbol;

use crate::agent::dispatch_capability::ClineDispatchCtx;
use crate::agent::llm::LlmDispatch;
use crate::api::ApiContext;
use crate::eval::bars::{self, BarCacheArgs};
use crate::eval::market_data::MarketDataContext;
use crate::eval::executor::asset_set::active_assets;
use crate::eval::executor::{Executor, RunExecutor};
use crate::eval::run::{DeploymentSource, MetricsSummary, Run, RunMode};
use crate::eval::scenario::Scenario;
use crate::eval::scenario_store;
use crate::eval::store::RunStore;
use crate::strategies::Strategy;
use crate::tools::ToolRegistry;
use xvision_core::config::AgentRuntime;

#[async_trait]
pub trait PaperTestRunner: Send + Sync {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary>;

    /// WS-11b: like [`run`](Self::run) but additionally surfaces the persisted
    /// eval `Run.id` for this evaluation, so the optimizer cycle can nest a
    /// navigable eval-run node under the candidate's experiment row
    /// (cycle → experiment → that candidate's eval-run trace).
    ///
    /// The default delegates to [`run`](Self::run) and returns `None` for the
    /// run id — test stubs and non-persisting implementors are unaffected. Only
    /// the production [`CachedBacktestPaperTester`] (and the wrapping
    /// [`BudgetCappedPaperTester`], which forwards) override it to return the
    /// real run id created at backtest time. The cycle calls this ONLY for the
    /// candidate's primary day-window eval; every other paper-test call site
    /// (parent baselines, untouched window, regime, inversion) keeps using
    /// [`run`](Self::run) so the hot path is unchanged.
    async fn run_with_run_id(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
    ) -> Result<(MetricsSummary, Option<String>)> {
        let metrics = self.run(strategy, scenario).await?;
        Ok((metrics, None))
    }

    /// F9: run a deliberately-sabotaged honesty-check (canary) strategy,
    /// tagging the run with the sabotage variant so the backtest executor
    /// relabels broker-rule rejections produced *by design* (e.g. the
    /// `kill-trades` variant zero-sizes every order → min-order-notional
    /// rejection) as expected honesty-check noise rather than emitting them as
    /// bare `WARN min_order_size_violation`. The default ignores the label, so
    /// stub/test runners and any future implementor are unaffected.
    async fn run_canary(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        _sabotage_variant: &str,
    ) -> Result<MetricsSummary> {
        self.run(strategy, scenario).await
    }

    /// Run a fixed-seed RANDOM "no-intelligence" trader with the SAME strategy
    /// structure (risk sizing, filters) over `scenario`, picking each decision
    /// uniformly from `direction`'s action set. Used to compute the optimizer's
    /// `edge_over_random` metric. The default impl is unsupported (test stubs
    /// fall back to `Err`, which the cycle treats as "no baseline" → edges 0).
    async fn run_random_baseline(
        &self,
        _strategy: &Strategy,
        _scenario: &Scenario,
        _direction: crate::autooptimizer::config::TradeDirection,
    ) -> Result<MetricsSummary> {
        anyhow::bail!("random baseline not supported by this PaperTestRunner")
    }
}

pub struct BacktestPaperTester {
    store: RunStore,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
    injected_bars: Option<Vec<Ohlcv>>,
}

impl BacktestPaperTester {
    pub fn new(store: RunStore, dispatch: Arc<dyn LlmDispatch>, tools: Arc<ToolRegistry>) -> Self {
        Self {
            store,
            dispatch,
            tools,
            injected_bars: None,
        }
    }

    pub fn with_bars(
        store: RunStore,
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        bars: Vec<Ohlcv>,
    ) -> Self {
        Self {
            store,
            dispatch,
            tools,
            injected_bars: Some(bars),
        }
    }
}

impl BacktestPaperTester {
    async fn run_inner(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        canary: Option<&str>,
    ) -> Result<MetricsSummary> {
        self.run_inner_with_dispatch(strategy, scenario, canary, None)
            .await
    }

    /// Like [`run_inner`] but allows overriding the LLM dispatch — used to run
    /// the random baseline through the identical backtest path with a
    /// `RandomBaselineDispatch` instead of the real model dispatch.
    async fn run_inner_with_dispatch(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        canary: Option<&str>,
        dispatch_override: Option<Arc<dyn LlmDispatch>>,
    ) -> Result<MetricsSummary> {
        let dispatch = dispatch_override.unwrap_or_else(|| Arc::clone(&self.dispatch));
        let mut executor = match self.injected_bars.as_ref() {
            Some(bars) => Executor::with_bars(bars.clone()),
            None => Executor::new(),
        };
        if let Some(variant) = canary {
            executor = executor.with_canary_sabotage(variant);
        }
        // CT5 §9.2: the optimizer path stamps source=Optimizer at run creation
        // so the dashboard Cancel-gate can distinguish optimizer runs from the
        // human queue (agent_id is NOT a reliable discriminator here — the
        // optimizer reuses strategy.manifest.id).
        let mut run = Run::new_queued(
            strategy.manifest.id.clone(),
            scenario.id.clone(),
            RunMode::Backtest,
        )
        .with_source(DeploymentSource::Optimizer);
        self.store.create(&run).await?;
        // Resolve the candidate strategy's agent slots so the trader has a
        // real model/prompt binding. Passing `&[]` here (the prior bug) left
        // the executor unable to find a trader slot, so every decision came
        // back `<no_response>` with 0 tokens and the run died at decision 0.
        let agent_slots =
            crate::agent::pipeline::resolve_agent_slots_for_strategy(self.store.pool(), strategy).await?;
        executor
            .run(
                &mut run,
                strategy,
                scenario,
                &agent_slots,
                Arc::clone(&dispatch),
                Arc::clone(&self.tools),
                &self.store,
            )
            .await
    }
}

#[async_trait]
impl PaperTestRunner for BacktestPaperTester {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary> {
        self.run_inner(strategy, scenario, None).await
    }

    async fn run_canary(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        sabotage_variant: &str,
    ) -> Result<MetricsSummary> {
        self.run_inner(strategy, scenario, Some(sabotage_variant)).await
    }

    async fn run_random_baseline(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        direction: crate::autooptimizer::config::TradeDirection,
    ) -> Result<MetricsSummary> {
        let actions = direction
            .baseline_actions()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let dispatch: Arc<dyn LlmDispatch> = Arc::new(
            crate::autooptimizer::random_baseline::RandomBaselineDispatch::new(
                crate::autooptimizer::random_baseline::RANDOM_BASELINE_SEED,
                actions,
            ),
        );
        self.run_inner_with_dispatch(strategy, scenario, None, Some(dispatch))
            .await
    }
}

/// Backtest paper tester that sources bars through the eval cache wrapper.
///
/// `BacktestPaperTester::new` preserves the legacy fixture-loader behavior
/// used by unit tests. This tester is the production CLI/dashboard adapter:
/// it fetches or reads DB-cached bars for the scenario window, injects them
/// into the unified backtest executor, and records eval run rows in `xvn.db`.
pub struct CachedBacktestPaperTester {
    ctx: ApiContext,
    dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    tools: Arc<ToolRegistry>,
    /// U5/UI4: optional progress bus shared with the backtest executor. When
    /// set, each `run`/`run_canary` constructs the executor with
    /// `.with_progress(bus.sender())`, so the executor emits
    /// `ProgressEvent::EvalHeartbeat`/`RunTick` while the (potentially long)
    /// parent baseline backtest is in flight. The cycle orchestrator subscribes
    /// via [`Self::subscribe`] before each paper-test call and re-emits a
    /// throttled `CycleProgressEvent::EvalProgress`/`Heartbeat` through the
    /// cycle's progress callback (see `cycle.rs`). `None` keeps the legacy
    /// silent behavior for callers that don't want progress.
    progress_bus: Option<Arc<crate::eval::progress::ProgressBus>>,
    /// Phase 1 parity: the shared Cline runtime + sidecar ctx for the trader,
    /// so the optimizer evaluates the SAME path as live. `None` (or
    /// `LlmDispatch`) keeps the legacy raw-dispatch trader. Cloned into each
    /// executor build (the client is an Arc, so one sidecar serves all runs).
    agent_runtime: AgentRuntime,
    cline_ctx: Option<ClineDispatchCtx>,
}

impl CachedBacktestPaperTester {
    /// Construct the production optimizer paper tester.
    ///
    /// F11 (QA run-4): realized cost is metered by routing `dispatch` through a
    /// [`super::metering_dispatch::CostMeteringDispatch`] at the call site (the
    /// CLI shares one meter across the backtest + mutator + judge). This tester
    /// therefore does no cost bookkeeping of its own — the earlier
    /// `model_calls.cost_usd` enrichment read $0.00 because the optimizer's
    /// decision model_calls aren't linked to this run's `eval_run_id`.
    pub fn new(
        ctx: ApiContext,
        dispatch: Arc<dyn LlmDispatch + Send + Sync>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            ctx,
            dispatch,
            tools,
            progress_bus: None,
            agent_runtime: AgentRuntime::default(),
            cline_ctx: None,
        }
    }

    /// Phase 1: attach the shared Cline runtime + sidecar ctx (spawned once by
    /// the optimizer via `spawn_optimizer_cline_ctx`). When set, every
    /// paper-test trader decision routes through `execute_slot_cline`.
    pub fn with_cline_runtime(mut self, runtime: AgentRuntime, cline_ctx: Option<ClineDispatchCtx>) -> Self {
        self.agent_runtime = runtime;
        self.cline_ctx = cline_ctx;
        self
    }

    /// U5: attach a shared progress bus so the backtest executor emits
    /// liveness events the cycle can bridge into `CycleProgressEvent`. Builder
    /// style so the existing `new` call sites are unchanged.
    pub fn with_progress_bus(mut self, bus: Arc<crate::eval::progress::ProgressBus>) -> Self {
        self.progress_bus = Some(bus);
        self
    }

    /// Subscribe to the attached progress bus, if any. The cycle orchestrator
    /// calls this BEFORE each `paper_tester.run(...)` so it doesn't miss the
    /// early `RunStarted`/`RunTick` events, then drains the receiver on a
    /// throttle to re-emit `CycleProgressEvent::EvalProgress`. Returns `None`
    /// when no bus was attached (the legacy silent path).
    pub fn subscribe(&self) -> Option<crate::eval::progress::ProgressRx> {
        self.progress_bus.as_ref().map(|b| b.subscribe())
    }
}

impl CachedBacktestPaperTester {
    async fn run_inner(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        canary: Option<&str>,
    ) -> Result<MetricsSummary> {
        // Discard the run id on the bare `run`/`run_canary` paths.
        self.run_inner_with_dispatch(strategy, scenario, canary, None)
            .await
            .map(|(metrics, _run_id)| metrics)
    }

    /// WS-11b: like [`run_inner`](Self::run_inner) but also returns the
    /// persisted eval `Run.id` so `run_with_run_id` can surface it for the
    /// optimizer experiment → eval-run nesting.
    async fn run_inner_with_dispatch(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        canary: Option<&str>,
        dispatch_override: Option<Arc<dyn LlmDispatch>>,
    ) -> Result<(MetricsSummary, String)> {
        ensure_scenario_persisted(&self.ctx, scenario).await?;
        let executor = build_cached_backtest_executor(
            &self.ctx,
            strategy,
            scenario,
            canary,
            self.progress_bus.as_deref(),
            self.agent_runtime,
            self.cline_ctx.clone(),
        )
        .await?;
        let store = RunStore::new(self.ctx.db.clone());
        // CT5 §9.2: optimizer-sourced run (see the BacktestPaperTester path above).
        let mut run = Run::new_queued(
            strategy.manifest.id.clone(),
            scenario.id.clone(),
            RunMode::Backtest,
        )
        .with_source(DeploymentSource::Optimizer);
        store.create(&run).await?;
        store
            .ensure_agent_run_baseline(&run.id, self.ctx.obs_config.retention.mode.as_db_str())
            .await?;
        // WS-11b: capture the persisted run id before `executor.run` borrows
        // `&mut run` so we can hand it back to the cycle for experiment nesting.
        let run_id = run.id.clone();
        let dispatch: Arc<dyn LlmDispatch> = dispatch_override.unwrap_or_else(|| self.dispatch.clone());
        // Resolve the candidate strategy's agent slots (trader model/prompt
        // binding). The production CLI/dashboard optimizer adapter previously
        // passed `&[]` here, which is why a real `run-cycle` failed at
        // decision 0 with `trader_output[missing_response]` for every strategy.
        let agent_slots =
            crate::agent::pipeline::resolve_agent_slots_for_strategy(&self.ctx.db, strategy).await?;
        let metrics = executor
            .run(
                &mut run,
                strategy,
                scenario,
                &agent_slots,
                dispatch,
                Arc::clone(&self.tools),
                &store,
            )
            .await?;
        Ok((metrics, run_id))
    }
}

#[async_trait]
impl PaperTestRunner for CachedBacktestPaperTester {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary> {
        self.run_inner(strategy, scenario, None).await
    }

    async fn run_with_run_id(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
    ) -> Result<(MetricsSummary, Option<String>)> {
        // WS-11b: surface the candidate's persisted eval run id so the cycle
        // can nest a navigable eval-run node under the experiment row.
        self.run_inner_with_dispatch(strategy, scenario, None, None)
            .await
            .map(|(metrics, run_id)| (metrics, Some(run_id)))
    }

    async fn run_canary(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        sabotage_variant: &str,
    ) -> Result<MetricsSummary> {
        self.run_inner(strategy, scenario, Some(sabotage_variant)).await
    }

    async fn run_random_baseline(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        direction: crate::autooptimizer::config::TradeDirection,
    ) -> Result<MetricsSummary> {
        let actions = direction
            .baseline_actions()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let dispatch: Arc<dyn LlmDispatch> = Arc::new(
            crate::autooptimizer::random_baseline::RandomBaselineDispatch::new(
                crate::autooptimizer::random_baseline::RANDOM_BASELINE_SEED,
                actions,
            ),
        );
        self.run_inner_with_dispatch(strategy, scenario, None, Some(dispatch))
            .await
            .map(|(metrics, _run_id)| metrics)
    }
}

async fn ensure_scenario_persisted(ctx: &ApiContext, scenario: &Scenario) -> Result<()> {
    if scenario_store::get_scenario(ctx, &scenario.id)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .is_some()
    {
        return Ok(());
    }
    scenario_store::insert_scenario(ctx, scenario)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

pub struct StubPaperTester {
    pub metrics: MetricsSummary,
}

#[async_trait]
impl PaperTestRunner for StubPaperTester {
    async fn run(&self, _strategy: &Strategy, _scenario: &Scenario) -> Result<MetricsSummary> {
        Ok(self.metrics.clone())
    }
}

/// Wraps another `PaperTestRunner` and enforces a USD ceiling on cumulative
/// cycle inference cost. Before each backtest it checks the shared meter and, if
/// the accumulator has reached `budget_usd`, aborts the cycle instead of
/// launching another (full-window) backtest.
///
/// F11 (QA run-4): the meter is fed by the cycle's shared
/// [`super::metering_dispatch::CostMeteringDispatch`], which prices EVERY LLM
/// call — backtest trader decisions, the experiment writer, and the judge —
/// via the provider catalog as they happen. This wrapper therefore no longer
/// reads cost from the returned metrics (the prior `model_calls.cost_usd`
/// enrichment read $0.00 because the optimizer's decision calls aren't linked
/// to the paper-test eval run id); it only gates on the shared total. Pass the
/// shared handle via [`Self::new_with_handle`] (use `f64::INFINITY` to meter an
/// unbudgeted cycle without ever tripping).
///
/// It exists so `xvn optimizer run-cycle --budget` is a real guard rather than
/// the silent no-op it used to be (QA 2026-06-04, F2/F11).
pub struct BudgetCappedPaperTester {
    inner: Box<dyn PaperTestRunner>,
    budget_usd: f64,
    meter: Arc<std::sync::Mutex<super::metering_dispatch::CycleMeter>>,
}

impl BudgetCappedPaperTester {
    pub fn new(inner: Box<dyn PaperTestRunner>, budget_usd: f64) -> Self {
        Self {
            inner,
            budget_usd,
            meter: Arc::new(std::sync::Mutex::new(
                super::metering_dispatch::CycleMeter::default(),
            )),
        }
    }

    /// Like [`Self::new`] but gates on a caller-provided [`CycleMeter`] so the
    /// cycle's metering dispatch (backtest + mutator + judge) and this budget
    /// gate share one running total (F11/F23). Pass `f64::INFINITY` for
    /// `budget_usd` to meter without ever tripping (an unbudgeted cycle that
    /// still wants a correct realized `cycle cost:` + token totals).
    pub fn new_with_handle(
        inner: Box<dyn PaperTestRunner>,
        budget_usd: f64,
        meter: Arc<std::sync::Mutex<super::metering_dispatch::CycleMeter>>,
    ) -> Self {
        Self {
            inner,
            budget_usd,
            meter,
        }
    }
}

impl BudgetCappedPaperTester {
    async fn run_budgeted(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        canary: Option<&str>,
    ) -> Result<MetricsSummary> {
        // Pre-check in a scoped block so the lock is released before the
        // `.await` below (never hold a std Mutex across an await point).
        {
            let spent = self.meter.lock().expect("budget mutex poisoned").spent_usd;
            if spent >= self.budget_usd {
                anyhow::bail!(
                    "optimizer cycle --budget of ${:.4} reached (spent ${:.4} on paper-test \
                     inference); stopping before the next backtest",
                    self.budget_usd,
                    spent,
                );
            }
        }
        // Preserve the canary label through the budget wrapper so the inner
        // tester still relabels sabotage broker-rule noise. Cost is accumulated
        // by the shared metering dispatch as the backtest runs — not from the
        // returned metrics — so there's nothing to add here (doing so would
        // double-count).
        match canary {
            Some(variant) => self.inner.run_canary(strategy, scenario, variant).await,
            None => self.inner.run(strategy, scenario).await,
        }
    }
}

#[async_trait]
impl PaperTestRunner for BudgetCappedPaperTester {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary> {
        self.run_budgeted(strategy, scenario, None).await
    }

    async fn run_with_run_id(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
    ) -> Result<(MetricsSummary, Option<String>)> {
        // WS-11b: forward to the inner tester so the wrapped
        // CachedBacktestPaperTester's real run id reaches the cycle. The budget
        // pre-check still applies (same path as `run_budgeted` with no canary).
        {
            let spent = self.meter.lock().expect("budget mutex poisoned").spent_usd;
            if spent >= self.budget_usd {
                anyhow::bail!(
                    "optimizer cycle --budget of ${:.4} reached (spent ${:.4} on paper-test \
                     inference); stopping before the next backtest",
                    self.budget_usd,
                    spent,
                );
            }
        }
        self.inner.run_with_run_id(strategy, scenario).await
    }

    async fn run_canary(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        sabotage_variant: &str,
    ) -> Result<MetricsSummary> {
        self.run_budgeted(strategy, scenario, Some(sabotage_variant))
            .await
    }

    async fn run_random_baseline(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        direction: crate::autooptimizer::config::TradeDirection,
    ) -> Result<MetricsSummary> {
        // The random baseline spends zero tokens, so the budget cap is moot —
        // forward straight to the inner tester so the real backtest path runs.
        self.inner
            .run_random_baseline(strategy, scenario, direction)
            .await
    }
}


async fn build_market_data_context(
    ctx: &ApiContext,
    strategy: &Strategy,
    scenario: &Scenario,
    assets: &[AssetSymbol],
) -> Result<MarketDataContext> {
    let mut market_data = MarketDataContext::new();
<<<<<<< HEAD
    for asset in assets {
        let bars = load_ohlcv_for_scenario(ctx, scenario, *asset).await?;
        market_data.insert_series(*asset, scenario.granularity, bars);
=======
    let native_granularity = crate::strategies::bar_granularity_for_cadence(
        strategy.manifest.decision_cadence_minutes,
    );
    for asset in assets {
        let bars = load_ohlcv_for_scenario(ctx, scenario, *asset, native_granularity).await?;
        market_data.insert_series(*asset, native_granularity, bars);
>>>>>>> feat/multi-timeframe-strategies
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
            "2h" => xvision_data::alpaca::BarGranularity::new(
                2,
                xvision_data::alpaca::BarGranularityUnit::Hour,
            )
            .expect("validated 2h granularity"),
            "4h" => xvision_data::alpaca::BarGranularity::Hour4,
            "1d" => xvision_data::alpaca::BarGranularity::Day1,
            _ => continue,
        };
        for asset in assets {
            let asset_pair = asset.as_alpaca_pair();
            let cache_key = bars::compute_cache_key(
                &asset_pair,
                granularity,
                scenario.time_window.start,
                scenario.time_window.end,
                "alpaca-historical-v1",
            );
            let bars = bars::load_bars(
                ctx,
                &BarCacheArgs {
                    cache_key,
                    asset_pair: asset_pair.clone(),
                    granularity,
                    start: scenario.time_window.start,
                    end: scenario.time_window.end,
                    data_source_tag: "alpaca-historical-v1".into(),
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("load {} bars for {asset_pair} in scenario {}", tf.as_str(), scenario.id))?;
            market_data.insert_series(*asset, granularity, market_bars_to_ohlcv(bars));
        }
    }
    Ok(market_data)
}
async fn build_cached_backtest_executor(
    ctx: &ApiContext,
    strategy: &Strategy,
    scenario: &Scenario,
    canary: Option<&str>,
    progress_bus: Option<&crate::eval::progress::ProgressBus>,
    agent_runtime: AgentRuntime,
    cline_ctx: Option<ClineDispatchCtx>,
) -> Result<Executor> {
    let active = active_assets(&strategy.manifest.asset_universe, None)?;
    let first_asset = *active.first().context("strategy asset_universe resolved empty")?;
    let market_data = build_market_data_context(ctx, strategy, scenario, &active).await?;

    let native_granularity = crate::strategies::bar_granularity_for_cadence(
        strategy.manifest.decision_cadence_minutes,
    );
    let mut asset_bars = BTreeMap::new();
    for asset in &active {
        let bars = market_data
<<<<<<< HEAD
            .series(*asset, scenario.granularity)
=======
            .series(*asset, native_granularity)
>>>>>>> feat/multi-timeframe-strategies
            .with_context(|| format!("missing native bars for {}", asset.as_alpaca_pair()))?;
        asset_bars.insert(*asset, bars.to_vec());
    }

    let warmup = load_warmup_for_scenario(ctx, scenario, first_asset, native_granularity).await?;
    let mut executor = if asset_bars.len() == 1 && asset_bars.contains_key(&first_asset) {
        Executor::with_bars(
            asset_bars
                .remove(&first_asset)
                .expect("first asset bars were inserted"),
        )
    } else {
        Executor::new().with_asset_bars(asset_bars)
    }
    .with_market_data(market_data)
    .with_warmup(warmup)
    .with_event_bus(ctx.event_bus.clone());
    // Parity (2026-06-13): trader runs on Cline, which does NOT do execute_slot-layer per-decision
    // memory recall/write (matching live). No with_memory_recorder here — adding it back would
    // re-invert the optimizer vs production.
    // F9: tag honesty-check (canary) runs so the executor relabels the
    // by-design broker-rule rejections as expected honesty-check noise.
    if let Some(variant) = canary {
        executor = executor.with_canary_sabotage(variant);
    }
    // U5: wire the executor's progress channel to the shared bus so the
    // optimizer cycle can observe liveness (EvalHeartbeat/RunTick) during the
    // parent baseline backtest and re-emit it as CycleProgressEvent::EvalProgress.
    if let Some(bus) = progress_bus {
        executor = executor.with_progress_tx(bus.sender());
    }
    // Phase 1 parity: route the trader through the SAME Cline runtime as
    // live/eval. With `cline_ctx = Some`, `should_use_cline` is true and the
    // shared pipeline dispatches via `execute_slot_cline`.
    executor = executor.with_cline_runtime(agent_runtime, cline_ctx);

    Ok(executor)
}

async fn load_ohlcv_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
    asset: AssetSymbol,
    granularity: xvision_data::alpaca::BarGranularity,
) -> Result<Vec<Ohlcv>> {
    let asset_pair = asset.as_alpaca_pair();
    let cache_key = bars::compute_cache_key(
        &asset_pair,
<<<<<<< HEAD
        scenario.granularity,
=======
        granularity,
>>>>>>> feat/multi-timeframe-strategies
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );
    let bars = bars::load_bars(
        ctx,
        &BarCacheArgs {
            cache_key,
            asset_pair: asset_pair.clone(),
            granularity,
            start: scenario.time_window.start,
            end: scenario.time_window.end,
            data_source_tag: "alpaca-historical-v1".into(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
    .with_context(|| format!("load bars for {asset_pair} in scenario {}", scenario.id))?;
    Ok(market_bars_to_ohlcv(bars))
}

async fn load_warmup_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
    asset: AssetSymbol,
    granularity: xvision_data::alpaca::BarGranularity,
) -> Result<Vec<Ohlcv>> {
    let asset_pair = asset.as_alpaca_pair();
    let bars = bars::load_warmup_bars(
        ctx,
        &asset_pair,
        granularity,
        scenario.time_window.start,
        scenario.warmup_bars,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
    .with_context(|| format!("load warmup bars for {asset_pair} in scenario {}", scenario.id))?;
    Ok(market_bars_to_ohlcv(bars))
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

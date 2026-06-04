//! Bridges autooptimizer paper-test calls to the eval engine's backtest executor.
//! The `PaperTestRunner` trait decouples the autooptimizer orchestrator from
//! the eval engine so tests can substitute a deterministic `StubPaperTester`.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use xvision_core::market::Ohlcv;
use xvision_core::trading::AssetSymbol;

use crate::agent::llm::LlmDispatch;
use crate::api::ApiContext;
use crate::eval::bars::{self, BarCacheArgs};
use crate::eval::executor::asset_set::active_assets;
use crate::eval::executor::{Executor, RunExecutor};
use crate::eval::run::{MetricsSummary, Run, RunMode};
use crate::eval::scenario::Scenario;
use crate::eval::scenario_store;
use crate::eval::store::RunStore;
use crate::strategies::Strategy;
use crate::tools::ToolRegistry;

#[async_trait]
pub trait PaperTestRunner: Send + Sync {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary>;

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
        let mut executor = match self.injected_bars.as_ref() {
            Some(bars) => Executor::with_bars(bars.clone()),
            None => Executor::new(),
        };
        if let Some(variant) = canary {
            executor = executor.with_canary_sabotage(variant);
        }
        let mut run = Run::new_queued(
            strategy.manifest.id.clone(),
            scenario.id.clone(),
            RunMode::Backtest,
        );
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
                Arc::clone(&self.dispatch),
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
    /// F11: optional shared counter of paper-test model calls that had no
    /// catalog price (token-bearing but unpriceable), so the CLI can report
    /// "cost unknown — N calls unpriced" instead of a misleading `$0.00`.
    unpriced_calls: Option<Arc<std::sync::Mutex<u64>>>,
}

impl CachedBacktestPaperTester {
    pub fn new(
        ctx: ApiContext,
        dispatch: Arc<dyn LlmDispatch + Send + Sync>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            ctx,
            dispatch,
            tools,
            unpriced_calls: None,
        }
    }

    /// Attach a shared counter that accumulates paper-test model calls with no
    /// catalog price (F11).
    pub fn with_unpriced_counter(mut self, counter: Arc<std::sync::Mutex<u64>>) -> Self {
        self.unpriced_calls = Some(counter);
        self
    }
}

impl CachedBacktestPaperTester {
    async fn run_inner(
        &self,
        strategy: &Strategy,
        scenario: &Scenario,
        canary: Option<&str>,
    ) -> Result<MetricsSummary> {
        ensure_scenario_persisted(&self.ctx, scenario).await?;
        let executor = build_cached_backtest_executor(&self.ctx, strategy, scenario, canary).await?;
        let store = RunStore::new(self.ctx.db.clone());
        let mut run = Run::new_queued(
            strategy.manifest.id.clone(),
            scenario.id.clone(),
            RunMode::Backtest,
        );
        store.create(&run).await?;
        store
            .ensure_agent_run_baseline(&run.id, self.ctx.obs_config.retention.mode.as_db_str())
            .await?;
        let dispatch: Arc<dyn LlmDispatch> = self.dispatch.clone();
        // Resolve the candidate strategy's agent slots (trader model/prompt
        // binding). The production CLI/dashboard optimizer adapter previously
        // passed `&[]` here, which is why a real `run-cycle` failed at
        // decision 0 with `trader_output[missing_response]` for every strategy.
        let agent_slots =
            crate::agent::pipeline::resolve_agent_slots_for_strategy(&self.ctx.db, strategy).await?;
        let mut metrics = executor
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
        // F11 (2026-06-04): the executor returns metrics *before* the eval
        // finalize path (`api::eval::enrich_with_inference_cost`) runs, so the
        // optimizer never saw a populated `inference_cost_quote_total` — the
        // `--budget` meter summed 0 and `cycle cost:` printed $0.00 even though
        // `model_calls.cost_usd` recorded real spend (~$2.04 for a gemini
        // cycle). Enrich here from the same `model_calls` ledger so the metered
        // total and the budget cap reflect realized paper-test inference cost.
        if metrics.inference_cost_quote_total.is_none() {
            if let Some(cost) =
                crate::eval::cost::aggregate_eval_run_inference_cost(&self.ctx.db, &run.id).await
            {
                metrics.inference_cost_quote_total = Some(cost);
            }
        }
        // F11: tally any unpriced calls so the operator isn't shown a
        // misleading `$0.00` when a real (but uncatalogued) model was billed.
        if let Some(counter) = &self.unpriced_calls {
            let n = crate::eval::cost::aggregate_eval_run_unpriced_calls(&self.ctx.db, &run.id).await;
            if n > 0 {
                *counter.lock().expect("unpriced counter mutex poisoned") += n;
            }
        }
        Ok(metrics)
    }
}

#[async_trait]
impl PaperTestRunner for CachedBacktestPaperTester {
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

/// Wraps another `PaperTestRunner` and enforces a USD ceiling on the
/// cumulative paper-test inference cost. After each run reports
/// `inference_cost_quote_total`, the cost is added to the accumulator; once
/// the accumulator reaches `budget_usd`, the next `run` aborts the cycle
/// instead of launching another (full-window) backtest.
///
/// This caps the *dominant* cost surface — the per-candidate backtests the
/// optimizer fans out (parents × mutations × two windows).
///
/// F11 (QA 2026-06-04) closed the two gaps that previously made the cap
/// blind:
///   1. The accumulator can be a *shared* handle (`new_with_handle`) that the
///      mutator/judge `CostMeteringDispatch` also writes to, so the realized
///      total — and therefore the cap — includes the experiment-writer and
///      judge LLM calls, not just paper-test inference.
///   2. The optimizer paper tester now enriches its returned metrics from the
///      `model_calls` ledger (token-count × catalog pricing), so providers
///      like OpenRouter that don't self-report `inference_cost_quote_total`
///      still contribute their real cost and the cap can trip for them.
///
/// It exists so `xvn optimizer run-cycle --budget` is a real guard rather than
/// the silent no-op it used to be (QA 2026-06-04, F2/F11).
pub struct BudgetCappedPaperTester {
    inner: Box<dyn PaperTestRunner>,
    budget_usd: f64,
    spent_quote: Arc<std::sync::Mutex<f64>>,
}

impl BudgetCappedPaperTester {
    pub fn new(inner: Box<dyn PaperTestRunner>, budget_usd: f64) -> Self {
        Self {
            inner,
            budget_usd,
            spent_quote: Arc::new(std::sync::Mutex::new(0.0)),
        }
    }

    /// Like [`Self::new`] but accumulates into a caller-provided handle so the
    /// cycle's mutator/judge metering shares one running total with the
    /// paper-test meter (F11). Pass `f64::INFINITY` for `budget_usd` to meter
    /// without ever tripping (an unbudgeted cycle that still wants a correct
    /// realized `cycle cost:`).
    pub fn new_with_handle(
        inner: Box<dyn PaperTestRunner>,
        budget_usd: f64,
        spent_quote: Arc<std::sync::Mutex<f64>>,
    ) -> Self {
        Self {
            inner,
            budget_usd,
            spent_quote,
        }
    }

    /// Shared handle to the running total of metered paper-test inference cost
    /// (USD). The CLI clones this before boxing the tester so it can print the
    /// realized "cycle cost: $X.XX" once the cycle finishes (minor find,
    /// 2026-06-04 — F2 metered cost for the cap but never surfaced the total).
    pub fn spent_quote_handle(&self) -> Arc<std::sync::Mutex<f64>> {
        Arc::clone(&self.spent_quote)
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
            let spent = *self.spent_quote.lock().expect("budget mutex poisoned");
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
        // tester still relabels sabotage broker-rule noise.
        let metrics = match canary {
            Some(variant) => self.inner.run_canary(strategy, scenario, variant).await?,
            None => self.inner.run(strategy, scenario).await?,
        };
        if let Some(cost) = metrics.inference_cost_quote_total {
            *self.spent_quote.lock().expect("budget mutex poisoned") += cost;
        }
        Ok(metrics)
    }
}

#[async_trait]
impl PaperTestRunner for BudgetCappedPaperTester {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary> {
        self.run_budgeted(strategy, scenario, None).await
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
}

async fn build_cached_backtest_executor(
    ctx: &ApiContext,
    strategy: &Strategy,
    scenario: &Scenario,
    canary: Option<&str>,
) -> Result<Executor> {
    let active = active_assets(&strategy.manifest.asset_universe, None)?;
    let first_asset = *active.first().context("strategy asset_universe resolved empty")?;

    let mut asset_bars = BTreeMap::new();
    for asset in &active {
        let bars = load_ohlcv_for_scenario(ctx, scenario, *asset).await?;
        asset_bars.insert(*asset, bars);
    }

    let warmup = load_warmup_for_scenario(ctx, scenario, first_asset).await?;
    let mut executor = if asset_bars.len() == 1 && asset_bars.contains_key(&first_asset) {
        Executor::with_bars(
            asset_bars
                .remove(&first_asset)
                .expect("first asset bars were inserted"),
        )
    } else {
        Executor::new().with_asset_bars(asset_bars)
    }
    .with_warmup(warmup)
    .with_event_bus(ctx.event_bus.clone());

    if let Some(recorder) = ctx.memory_recorder.clone() {
        executor = executor.with_memory_recorder(recorder);
    }
    // F9: tag honesty-check (canary) runs so the executor relabels the
    // by-design broker-rule rejections as expected honesty-check noise.
    if let Some(variant) = canary {
        executor = executor.with_canary_sabotage(variant);
    }

    Ok(executor)
}

async fn load_ohlcv_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
    asset: AssetSymbol,
) -> Result<Vec<Ohlcv>> {
    let asset_pair = asset.as_alpaca_pair();
    let bars = bars::load_bars(
        ctx,
        &BarCacheArgs {
            cache_key: scenario.bar_cache_policy.cache_key.clone(),
            asset_pair: asset_pair.clone(),
            granularity: scenario.granularity,
            start: scenario.time_window.start,
            end: scenario.time_window.end,
            data_source_tag: "alpaca-historical-v1".into(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
    .with_context(|| {
        format!(
            "load bars for {asset_pair} in scenario {} ({})",
            scenario.id, scenario.bar_cache_policy.cache_key
        )
    })?;
    Ok(market_bars_to_ohlcv(bars))
}

async fn load_warmup_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
    asset: AssetSymbol,
) -> Result<Vec<Ohlcv>> {
    let asset_pair = asset.as_alpaca_pair();
    let bars = bars::load_warmup_bars(
        ctx,
        &asset_pair,
        scenario.granularity,
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

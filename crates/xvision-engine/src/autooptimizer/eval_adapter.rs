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

#[async_trait]
impl PaperTestRunner for BacktestPaperTester {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary> {
        let executor = match self.injected_bars.as_ref() {
            Some(bars) => Executor::with_bars(bars.clone()),
            None => Executor::new(),
        };
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
            crate::agent::pipeline::resolve_agent_slots_for_strategy(self.store.pool(), strategy)
                .await?;
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
}

impl CachedBacktestPaperTester {
    pub fn new(
        ctx: ApiContext,
        dispatch: Arc<dyn LlmDispatch + Send + Sync>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self { ctx, dispatch, tools }
    }
}

#[async_trait]
impl PaperTestRunner for CachedBacktestPaperTester {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary> {
        ensure_scenario_persisted(&self.ctx, scenario).await?;
        let executor = build_cached_backtest_executor(&self.ctx, strategy, scenario).await?;
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
        executor
            .run(
                &mut run,
                strategy,
                scenario,
                &agent_slots,
                dispatch,
                Arc::clone(&self.tools),
                &store,
            )
            .await
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
/// cumulative paper-test inference cost. Once the accumulated
/// `inference_cost_quote_total` across all runs reaches `budget_usd`,
/// the next `run` aborts the cycle instead of launching another
/// (full-window) backtest.
///
/// This is a best-effort ceiling on the *dominant* cost surface — the
/// per-candidate backtests the optimizer fans out (parents × mutations ×
/// two windows). Two honest limitations:
///   1. Mutator/judge LLM calls are not metered here, so the realized
///      total is slightly higher than what is capped.
///   2. Providers that don't report `inference_cost_quote_total`
///      contribute `0`, so the cap can't trip for them.
///
/// It exists so `xvn optimizer run-cycle --budget` is a real guard
/// rather than the silent no-op it used to be (QA 2026-06-04, F2).
pub struct BudgetCappedPaperTester {
    inner: Box<dyn PaperTestRunner>,
    budget_usd: f64,
    spent_quote: std::sync::Mutex<f64>,
}

impl BudgetCappedPaperTester {
    pub fn new(inner: Box<dyn PaperTestRunner>, budget_usd: f64) -> Self {
        Self {
            inner,
            budget_usd,
            spent_quote: std::sync::Mutex::new(0.0),
        }
    }
}

#[async_trait]
impl PaperTestRunner for BudgetCappedPaperTester {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary> {
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
        let metrics = self.inner.run(strategy, scenario).await?;
        if let Some(cost) = metrics.inference_cost_quote_total {
            *self.spent_quote.lock().expect("budget mutex poisoned") += cost;
        }
        Ok(metrics)
    }
}

async fn build_cached_backtest_executor(
    ctx: &ApiContext,
    strategy: &Strategy,
    scenario: &Scenario,
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

//! Bridges autooptimizer paper-test calls to the eval engine's backtest executor.
//! The `PaperTestRunner` trait decouples the autooptimizer orchestrator from
//! the eval engine so tests can substitute a deterministic `StubPaperTester`.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use xvision_core::market::Ohlcv;

use crate::agent::llm::LlmDispatch;
use crate::eval::executor::{Executor, RunExecutor};
use crate::eval::run::{MetricsSummary, Run, RunMode};
use crate::eval::scenario::Scenario;
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
        executor
            .run(
                &mut run,
                strategy,
                scenario,
                &[],
                Arc::clone(&self.dispatch),
                Arc::clone(&self.tools),
                &self.store,
            )
            .await
    }
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

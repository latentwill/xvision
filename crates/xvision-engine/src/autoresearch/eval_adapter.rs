//! PaperTestRunner bridge — AR-2 Task 1.
//!
//! `StubPaperTester` returns pre-set metrics for unit tests.
//! `BacktestPaperTester` runs a real backtest via the eval executor.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use ulid::Ulid;
use xvision_core::market::Ohlcv;
use xvision_memory::types::MemoryMode;

use crate::agent::llm::LlmDispatch;
use crate::agent::pipeline::ResolvedAgentSlot;
use crate::agents::{Capability, InputsPolicy};
use crate::eval::executor::{Executor, RunExecutor};
use crate::eval::run::{MetricsSummary, Run, RunMode};
use crate::eval::scenario::Scenario;
use crate::eval::store::RunStore;
use crate::strategies::Strategy;
use crate::tools::ToolRegistry;

/// Runs a strategy against a scenario and returns performance metrics.
#[async_trait]
pub trait PaperTestRunner: Send + Sync {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary>;
}

/// Returns identical preset metrics for every call — suitable for unit tests
/// where the tester should not influence the strategy under evaluation.
pub struct StubPaperTester {
    pub metrics: MetricsSummary,
}

#[async_trait]
impl PaperTestRunner for StubPaperTester {
    async fn run(&self, _strategy: &Strategy, _scenario: &Scenario) -> Result<MetricsSummary> {
        Ok(self.metrics.clone())
    }
}

/// Runs a real backtest via the eval executor with pre-loaded OHLCV bars.
/// Deterministic for a given strategy + scenario + bars triple.
pub struct BacktestPaperTester {
    store: RunStore,
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
    bars: Vec<Ohlcv>,
}

impl BacktestPaperTester {
    pub fn with_bars(
        store: RunStore,
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        bars: Vec<Ohlcv>,
    ) -> Self {
        Self { store, dispatch, tools, bars }
    }
}

#[async_trait]
impl PaperTestRunner for BacktestPaperTester {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary> {
        let agent_slots = resolve_slots_from_strategy(strategy);
        let mut run = Run::new_queued(
            strategy.manifest.id.clone(),
            scenario.id.clone(),
            RunMode::Backtest,
        );
        self.store.create(&run).await?;
        let executor = Executor::with_bars(self.bars.clone());
        let metrics = executor
            .run(
                &mut run,
                strategy,
                scenario,
                &agent_slots,
                self.dispatch.clone(),
                self.tools.clone(),
                &self.store,
            )
            .await?;
        Ok(metrics)
    }
}

/// Resolve `ResolvedAgentSlot`s from a strategy, supporting both the new
/// `agents` field and legacy `trader_slot` / `intern_slot` / `regime_slot`.
fn resolve_slots_from_strategy(strategy: &Strategy) -> Vec<ResolvedAgentSlot> {
    // Legacy path: legacy slots present, no agents
    if strategy.agents.is_empty() {
        let mut out = Vec::new();
        for slot in [
            strategy.regime_slot.as_ref().map(|s| ("regime", s)),
            strategy.intern_slot.as_ref().map(|s| ("intern", s)),
            strategy.trader_slot.as_ref().map(|s| ("trader", s)),
        ]
        .into_iter()
        .flatten()
        {
            out.push(ResolvedAgentSlot {
                role: slot.0.to_string(),
                slot: slot.1.clone(),
                system_prompt: String::new(),
                max_tokens: None,
                max_wall_ms: None,
                temperature: None,
                inputs_policy: InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: MemoryMode::Off,
                agent_id: Ulid::new().to_string(),
                capabilities: BTreeSet::new(),
                noop_skip: true,
            });
        }
        return out;
    }
    // New path: agents field populated — caller must have resolved them externally.
    // BacktestPaperTester cannot look up agent records without a DB, so we
    // return empty and let the executor run with no agent slots (noop run).
    Vec::new()
}

//! Executor trait + concrete impls. Phase 3.B of the Eval Engine plan.
//!
//! The Executor abstracts over the two run modes:
//! - **Backtest** — replays a parquet fixture in chronological order,
//!   simulates fills with slippage + fees. No broker required.
//! - **Paper** — drives `BrokerSurface::submit_order` against a real or
//!   mocked broker, suitable for the v1 demo path against Alpaca paper.
//!
//! Callers (`engine::api::eval::run`, the eval CLI) pick an executor by
//! `RunMode` and call `run(...)` once per `xvn eval run` invocation.

pub mod backtest;
pub mod paper;

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::LlmDispatch;
use crate::bundle::StrategyBundle;
use crate::eval::run::{MetricsSummary, Run};
use crate::eval::scenario::Scenario;
use crate::eval::store::RunStore;
use crate::tools::ToolRegistry;

pub use backtest::BacktestExecutor;
pub use paper::PaperExecutor;

#[async_trait]
pub trait Executor: Send + Sync {
    /// Run the strategy against the scenario end-to-end. Mutates `run`
    /// in-place to reflect status transitions (Queued → Running → Completed
    /// or Failed) and the final `MetricsSummary`. Persists every decision
    /// + equity sample + the final metrics through `store`. Returns the
    /// computed `MetricsSummary` for callers that want the value without
    /// re-reading from the store.
    async fn run(
        &self,
        run: &mut Run,
        bundle: &StrategyBundle,
        scenario: &Scenario,
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        store: &RunStore,
    ) -> anyhow::Result<MetricsSummary>;
}

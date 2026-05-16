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
pub mod trader_output;

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::LlmDispatch;
use crate::agent::pipeline::ResolvedAgentSlot;
use crate::eval::run::{MetricsSummary, Run};
use crate::eval::scenario::Scenario;
use crate::eval::store::RunStore;
use crate::strategies::Strategy;
use crate::tools::ToolRegistry;

pub use backtest::BacktestExecutor;
pub use paper::PaperExecutor;
pub use trader_output::{TraderFailureKind, TraderOutputError};

/// Stable failure-class tag for a run-level error. Paper/backtest executors
/// prefix the persisted `eval_runs.error` string with `[<class>]` so review
/// and UI consumers can read the class without re-parsing the full message.
///
/// Classes:
///  - Trader output classes: `empty`, `tool_use_only`, `truncated`,
///    `invalid_json`, `missing_field`, `invalid_field`, `missing_response`.
///  - Provider transport classes: `provider_timeout`, `provider_connect`,
///    `provider_http_error`.
///  - `unclassified` for anything else.
pub fn classify_run_failure(err: &anyhow::Error) -> &'static str {
    if let Some(te) = err.downcast_ref::<TraderOutputError>() {
        return te.class_tag();
    }
    let s = err.to_string().to_lowercase();
    // Trader-output errors may have been wrapped with `.context(...)` and
    // not survive downcast — check the message form as a fallback.
    for kind in [
        TraderFailureKind::EmptyText,
        TraderFailureKind::ToolUseOnly,
        TraderFailureKind::Truncated,
        TraderFailureKind::InvalidJson,
        TraderFailureKind::MissingField,
        TraderFailureKind::InvalidField,
        TraderFailureKind::MissingResponse,
    ] {
        let needle = format!("trader_output[{}]", kind.tag());
        if s.contains(&needle) {
            return kind.tag();
        }
    }
    if s.contains("timed out") || s.contains("timeout") {
        return "provider_timeout";
    }
    if s.contains("tcp connect") || s.contains("dns error") || s.contains("connection refused") {
        return "provider_connect";
    }
    if s.contains("anthropic api error") || s.contains("openai-compat api error") {
        return "provider_http_error";
    }
    "unclassified"
}

/// Format the persisted/displayed failure string for a run error. The
/// `[<class>] ` prefix is the stable wire shape downstream consumers parse.
pub(crate) fn format_failure_reason(err: &anyhow::Error) -> String {
    let class = classify_run_failure(err);
    let raw = err.to_string();
    if raw.starts_with(&format!("[{class}]")) {
        raw
    } else {
        format!("[{class}] {raw}")
    }
}

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
        strategy: &Strategy,
        scenario: &Scenario,
        agent_slots: &[ResolvedAgentSlot],
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        store: &RunStore,
    ) -> anyhow::Result<MetricsSummary>;
}

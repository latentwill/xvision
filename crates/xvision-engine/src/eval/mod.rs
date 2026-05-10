//! Eval engine — runs strategies against scenarios, persists every decision
//! and equity sample, finalizes metrics. Module foundations only in this
//! Phase 3.A scope; executors / metrics / findings / compare / CLI / MCP
//! arrive in subsequent phases.
//!
//! See `docs/superpowers/plans/2026-05-08-eval-engine-plan.md` for the full
//! roadmap.

pub mod attestation;
pub mod compare;
pub mod executor;
pub mod findings;
pub mod metrics;
pub mod run;
pub mod scenario;
pub mod store;

pub use attestation::{EvalAttestation, TokensUsed};
pub use compare::{
    compare_runs, ComparisonEquityCurve, ComparisonEquitySample, ComparisonReport,
    ComparisonRunSummary,
};
pub use findings::{Finding, Severity};

pub use run::{MetricsSummary, Run, RunMode, RunStatus};
pub use scenario::{
    canonical_scenarios, Capital, Fees, LatencyModel, Scenario, ScenarioRisk, SlippageModel,
    TimeWindow,
};
pub use store::{DecisionRow, ListFilter, RunStore};

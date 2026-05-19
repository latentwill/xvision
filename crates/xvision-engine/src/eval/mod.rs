//! Eval engine — runs strategies against scenarios, persists every decision
//! and equity sample, finalizes metrics. Module foundations only in this
//! Phase 3.A scope; executors / metrics / findings / compare / CLI / MCP
//! arrive in subsequent phases.
//!
//! See `docs/superpowers/plans/2026-05-08-eval-engine-plan.md` for the full
//! roadmap.

pub mod attestation;
pub mod bars;
pub mod behavior;
pub mod compare;
pub mod concurrency;
pub mod cost;
pub mod executor;
pub mod export;
pub mod findings;
pub mod metrics;
pub mod postprocess;
pub mod progress;
pub mod review;
pub mod run;
pub mod scenario;
pub mod scenario_seed;
pub mod scenario_store;
pub mod store;

pub use attestation::{EvalAttestation, TokensUsed};
pub use compare::{
    compare_runs, ComparisonEquityCurve, ComparisonEquitySample, ComparisonReport, ComparisonRunSummary,
};
pub use cost::{compute_token_cost_usd, compute_token_cost_usd_from_catalog};
pub use findings::{Finding, Severity};
pub use progress::{send_event, ProgressBus, ProgressEvent, ProgressRx, ProgressTx};
pub use review::{AgentProfile, EvalReview, ReviewStatus, ReviewVerdict};

pub use run::{MetricsSummary, Run, RunMode, RunStatus};
#[allow(deprecated)]
pub use scenario::canonical_scenarios;
pub use scenario::{
    AdjustmentMode, AssetClass, AssetRef, BarCachePolicy, BarGranularity, CalendarRef, DataSource, Fees,
    FillModel, LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode,
    Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings, VolumeConstraint, WalkModel,
};
pub use store::{DecisionRow, ListFilter, RunStore};

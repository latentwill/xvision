//! Eval engine — runs strategies against scenarios, persists every decision
//! and equity sample, finalizes metrics. Module foundations only in this
//! Phase 3.A scope; executors / metrics / findings / compare / CLI / MCP
//! arrive in subsequent phases.
//!
//! See `docs/superpowers/plans/2026-05-08-eval-engine-plan.md` for the full
//! roadmap.

pub mod attestation;
pub mod attestation_engine;
pub mod attestation_verdict;
pub mod bars;
pub mod batch_store;
pub mod behavior;
pub mod broker_rules;
pub mod candle_integrity;
#[cfg(feature = "chain-attest")]
pub mod chain_attestation;
pub mod compare;
pub mod concurrency;
pub mod cost;
pub mod cost_arrays;
pub mod cycle_features;
pub mod determinism;
pub mod early_stop;
pub mod executor;
pub mod experiment_store;
pub mod export;
pub mod market_data;
pub mod filter_hook;
pub mod finalize_writer;
pub mod findings;
pub mod guardrail_summary;
pub mod guardrails;
pub mod limits;
pub mod live_config;
pub mod live_run_state;
pub mod metrics;
pub mod orders;
pub mod postprocess;
pub mod preflight;
pub mod progress;
pub mod regime;
pub mod report;
pub mod review;
pub mod run;
pub mod scenario;
pub mod scenario_seed;
pub mod scenario_store;
pub mod store;
pub mod watchdog;

pub use attestation::{EvalAttestation, TokensUsed};
pub use attestation_engine::{maybe_attest, AttestationTrigger};
pub use attestation_verdict::{
    should_fire, verdict, window_sharpe, Verdict, VerdictLabel, ATTESTATION_TRADE_WINDOW, TAG1_TRADING_YIELD,
    TAG2_MONTH,
};
pub use broker_rules::{
    rule_set_for_asset_class, AlpacaCryptoRules, AlpacaEquityRules, AlpacaEquityViolationKind, BrokerRuleSet,
    BrokerRuleViolation, BrokerViolationSeverity, OrderKind, PendingOrder, TimeInForce,
};
pub use compare::{
    compare_runs, compare_runs_default, CompareOptions, ComparisonEquityCurve, ComparisonEquitySample,
    ComparisonReport, ComparisonRunSummary, ManifestMismatch,
};
pub use cost::{
    aggregate_eval_run_inference_cost, aggregate_inference_cost_since, aggregate_optimizer_cost_since,
    compute_token_cost_usd, compute_token_cost_usd_from_catalog, get_daily_budget_cap,
    provider_reports_zero_cost, set_daily_budget_cap,
};
pub use cost_arrays::{BarCostEntry, BarCostTable};
pub use findings::{Finding, Severity};
pub use orders::OrderState;
pub use progress::{send_event, ProgressBus, ProgressEvent, ProgressRx, ProgressTx};
pub use review::{AgentProfile, EvalReview, ReviewStatus, ReviewVerdict};

pub use batch_store::{Batch, BatchStore};
pub use run::{DeploymentSource, MetricsSummary, Run, RunMode, RunStatus};
#[allow(deprecated)]
pub use scenario::canonical_scenarios;
pub use scenario::{
    AdjustmentMode, AssetClass, AssetRef, BarCachePolicy, BarGranularity, CalendarRef, DataSource, FeeSource,
    Fees, FillModel, FillProvenance, LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency,
    RefreshPolicy, ReplayMode, Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueOverride,
    VenueSettings, VolumeConstraint, WalkModel,
};
pub use store::{DecisionRow, ListFilter, RunStore};

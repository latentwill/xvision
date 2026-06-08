//! Filter v1 — deterministic per-bar gate data model, DSL parser,
//! validator, and runtime evaluator.
//!
//! Stages 1 + 2 of the Filter v1 plan. Stage 1 shipped the pure data
//! model. Stage 2 (this expansion — see
//! `team/contracts/track-plan-touches.md`) adds:
//!
//! * `indicators` — incremental math for the filter DSL catalog.
//! * `state` — per-filter mutable runtime state (warmup, cooldown,
//!   daily wakeup counter, previous-bar leaf cache for `crosses_*`).
//! * `runtime` — the per-bar evaluator. Given a validated `Filter`,
//!   a `FilterState`, a `Bar`, and an `EvalContext`, produces a
//!   `FilterEvalOutcome` carrying an `ActivationDecision`.
//!
//! The crate stays engine-independent: `Bar` is a local OHLCV
//! reduction, and the runtime takes `&self` so callers can hold one
//! `RuntimeFilter` across many concurrent `FilterState`s.

pub mod errors;
pub mod events;
pub mod indicators;
pub mod parse;
pub mod runtime;
pub mod state;
pub mod types;
pub mod validate;
pub mod warmup;

pub use errors::{ParseError, ValidationError};
pub use events::{FilterEventV1, FilterSummary, SuppressedReason};
pub use indicators::{Bar, IndicatorEngine, IndicatorKey};
pub use parse::{parse_json, parse_toml};
pub use runtime::{
    dsl_to_filter_signal, referenced_indicators, ActivationDecision, BridgedFilterSignal, ConditionResult,
    EvalContext, FilterEvalOutcome, RuntimeFilter, Transition,
};
pub use state::{collect_filter_indicator_refs, collect_indicator_refs, FilterState};
pub use types::{
    ActivationMode, AgentContextTemplateId, Condition, ConditionGroup, ConditionItem, ConditionTree,
    Filter, FilterFire, FilterId, FilterStatus, IndicatorName, IndicatorRef, Operand, Operator,
    ScanCadence, StrategyId, Symbol, Timeframe, WakeInPosition, DEFAULT_AGENT_CONTEXT_TEMPLATE,
};
pub use validate::validate;
pub use warmup::{check_filter_warmup, WarmupWarning};

/// Average token cost of a single LLM briefing dispatch, used by
/// [`events::FilterSummary::from_events`] to estimate the tokens saved
/// by FilterGated activation versus the EveryBar baseline.
///
/// v1 ships a global constant matching the scaling-assumption block in
/// `MANUAL.md`. A v1.5 follow-up will replace this with a per-strategy
/// measurement so the savings number tracks actual prompt sizes.
pub const AVG_BRIEFING_TOKEN_COST: u64 = 50_000;

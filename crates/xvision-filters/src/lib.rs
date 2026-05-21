//! Filter v1 — deterministic per-bar gate data model, DSL parser, and
//! validator.
//!
//! Stage 1 of the Filter v1 plan: pure data model with no engine
//! wiring. See `docs/superpowers/specs/2026-05-21-filter-v1.md` and the
//! companion plan at `docs/superpowers/plans/2026-05-21-filter-v1.md`
//! § "Stage 1 — Domain + validation".
//!
//! Public surface:
//!
//! * `parse_toml(&str)` / `parse_json(&str)` — DSL → `Filter`.
//! * `validate(&Filter)` — semantic checks, returns typed
//!   `ValidationError` with stable `E_FILTER_*` codes.
//! * `Filter`, `Condition`, `Operand`, `IndicatorRef`, `Operator`, and
//!   the supporting enums.
//!
//! The crate intentionally has **no** runtime behaviour — no indicator
//! math, no event emission, no engine deps. Stages 2–5 add those.

pub mod errors;
pub mod parse;
pub mod types;
pub mod validate;

pub use errors::{ParseError, ValidationError};
pub use parse::{parse_json, parse_toml};
pub use types::{
    ActivationMode, AgentContextTemplateId, Condition, ConditionTree, Filter, FilterId, FilterStatus,
    IndicatorName, IndicatorRef, Operand, Operator, ScanCadence, StrategyId, Symbol, Timeframe,
    WakeInPosition, DEFAULT_AGENT_CONTEXT_TEMPLATE,
};
pub use validate::validate;

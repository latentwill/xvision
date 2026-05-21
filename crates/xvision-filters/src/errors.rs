//! Error types for the Filter v1 crate.
//!
//! `ParseError` covers syntactic failures (DSL → struct) and
//! `ValidationError` covers semantic failures (struct → typed reject).
//! Both expose stable wire identifiers (`code()`) and JSON-pointer
//! field paths (`field_path()`) for the eventual frontend renderer.

use thiserror::Error;

/// Syntactic failures from `parse_toml` / `parse_json`.
///
/// `path` is a human-readable location string (line/column for TOML
/// errors, JSON pointer for JSON errors when available, or a descriptive
/// breadcrumb otherwise).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// TOML deserialization failure (or wrapper-shape mismatch).
    #[error("toml parse error at {path}: {message}")]
    Toml { path: String, message: String },

    /// JSON deserialization failure.
    #[error("json parse error at {path}: {message}")]
    Json { path: String, message: String },

    /// Indicator DSL token outside the v1 catalog or syntactically
    /// invalid (e.g. unparseable period, future-bar `+N` syntax).
    #[error("invalid indicator dsl '{token}' at {path}")]
    IndicatorDsl { path: String, token: String },

    /// Operator field carried a string outside the v1 catalog. Mapped
    /// from a serde error in `parse.rs` when the failure can be
    /// attributed to the `op` field.
    ///
    /// This is distinct from `ValidationError::UnknownOperator` (same
    /// code, different layer): the parser catches DSL inputs where the
    /// `op` string is wrong; the validator catches in-memory structs
    /// (impossible today because the enum is closed, but the variant is
    /// reserved for symmetry).
    #[error("unknown operator '{token}' at {path}")]
    UnknownOperator { path: String, token: String },

    /// Negative integer where a `u32` was expected. The `cooldown_bars`
    /// field uses this when serde rejects `-1` at the type level.
    #[error("negative integer for unsigned field at {path}: {token}")]
    NegativeUnsigned { path: String, token: String },
}

impl ParseError {
    pub fn field_path(&self) -> &str {
        match self {
            ParseError::Toml { path, .. }
            | ParseError::Json { path, .. }
            | ParseError::IndicatorDsl { path, .. }
            | ParseError::UnknownOperator { path, .. }
            | ParseError::NegativeUnsigned { path, .. } => path,
        }
    }
}

/// Semantic failures from `validate(&Filter)`.
///
/// Codes are part of the crate's public surface — frontend matchers
/// rely on the exact `E_FILTER_*` strings. See the error-code table in
/// `team/contracts/filter-v1.md`.
#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    #[error("E_FILTER_UNKNOWN_INDICATOR at {path}: {detail}")]
    UnknownIndicator { path: String, detail: String },

    #[error("E_FILTER_UNKNOWN_OPERATOR at {path}: {detail}")]
    UnknownOperator { path: String, detail: String },

    #[error("E_FILTER_OPERAND_TYPE at {path}: {detail}")]
    OperandType { path: String, detail: String },

    #[error("E_FILTER_RANGE_ORDER at {path}: {detail}")]
    RangeOrder { path: String, detail: String },

    #[error("E_FILTER_NUMERIC_BOUNDS at {path}: {detail}")]
    NumericBounds { path: String, detail: String },

    #[error("E_FILTER_FUTURE_LEAK at {path}: {detail}")]
    FutureLeak { path: String, detail: String },

    #[error("E_FILTER_COOLDOWN_NEG at {path}: {detail}")]
    CooldownNeg { path: String, detail: String },

    #[error("E_FILTER_WAKEUP_CAP at {path}: {detail}")]
    WakeupCap { path: String, detail: String },

    #[error("E_FILTER_ASSET_SCOPE at {path}: {detail}")]
    AssetScope { path: String, detail: String },

    #[error("E_FILTER_EMPTY_TREE at {path}: {detail}")]
    EmptyTree { path: String, detail: String },
}

impl ValidationError {
    /// Stable wire identifier. Frontend matchers depend on these
    /// strings; renaming requires a coordinated PR per the contract.
    pub fn code(&self) -> &'static str {
        match self {
            ValidationError::UnknownIndicator { .. } => "E_FILTER_UNKNOWN_INDICATOR",
            ValidationError::UnknownOperator { .. } => "E_FILTER_UNKNOWN_OPERATOR",
            ValidationError::OperandType { .. } => "E_FILTER_OPERAND_TYPE",
            ValidationError::RangeOrder { .. } => "E_FILTER_RANGE_ORDER",
            ValidationError::NumericBounds { .. } => "E_FILTER_NUMERIC_BOUNDS",
            ValidationError::FutureLeak { .. } => "E_FILTER_FUTURE_LEAK",
            ValidationError::CooldownNeg { .. } => "E_FILTER_COOLDOWN_NEG",
            ValidationError::WakeupCap { .. } => "E_FILTER_WAKEUP_CAP",
            ValidationError::AssetScope { .. } => "E_FILTER_ASSET_SCOPE",
            ValidationError::EmptyTree { .. } => "E_FILTER_EMPTY_TREE",
        }
    }

    /// JSON-pointer path to the offending field
    /// (e.g. `/conditions/all/2/rhs`).
    pub fn field_path(&self) -> &str {
        match self {
            ValidationError::UnknownIndicator { path, .. }
            | ValidationError::UnknownOperator { path, .. }
            | ValidationError::OperandType { path, .. }
            | ValidationError::RangeOrder { path, .. }
            | ValidationError::NumericBounds { path, .. }
            | ValidationError::FutureLeak { path, .. }
            | ValidationError::CooldownNeg { path, .. }
            | ValidationError::WakeupCap { path, .. }
            | ValidationError::AssetScope { path, .. }
            | ValidationError::EmptyTree { path, .. } => path,
        }
    }
}

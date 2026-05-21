//! V2E trace-surface types — per-decision and per-fill provenance fields.
//!
//! These types are the wire-stable schema for `decisions.jsonl` and
//! `fills.jsonl` artifacts emitted by both `BacktestExecutor` and
//! `PaperExecutor`. Old runs without these fields deserialize via
//! `serde(default)` and report `schema_version = "1"`.
//!
//! Schema version history:
//!   "1"  — original shape
//!   "2"  — V2E trace-surface: new per-decision LLM metadata fields +
//!          per-fill provenance fields (migration 026, 2026-05-21).

use serde::{Deserialize, Serialize};

/// Current JSONL schema version for the decisions + fills wire format.
pub const DECISIONS_SCHEMA_VERSION: &str = "2";

// ---------------------------------------------------------------------------
// Fill provenance types
// ---------------------------------------------------------------------------

/// Which intra-bar path was taken for a simulated fill.
///
/// Foundation lands the type as `Option<FillBranch>` defaulting `None`;
/// `eval-intra-bar-fill-ordering` populates the values in a later V2E track.
/// The enum is `#[non_exhaustive]` so downstream tracks can add variants
/// without breaking existing match arms.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FillBranch {
    /// Bar gapped past the trigger price at open — filled immediately.
    GapPast,
    /// OHLC sequence was O→H→L→C (high closer to open).
    OhlcHighFirst,
    /// OHLC sequence was O→L→H→C (low closer to open).
    OhlcLowFirst,
    /// No intra-bar ordering applied; fill at the next bar's open (default
    /// backtest behavior, pre-intra-bar-ordering).
    NextOpenOnly,
}

/// Source of the `fee_bps` applied to a fill.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FeeSource {
    /// Global scenario default fee schedule.
    #[default]
    Default,
    /// Override specified at the scenario level.
    ScenarioOverride,
    /// Per-asset override from the scenario's asset fee table.
    PerAssetOverride,
    /// Per-bar array value (populated by `eval-per-bar-cost-arrays`).
    PerBarArray,
}

/// Aggressor side for a fill — maker vs taker classification.
///
/// Foundation lands the type as `Option<AggressorSide>` defaulting `None`;
/// `eval-intra-bar-fill-ordering` populates the values.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AggressorSide {
    /// Market order or crossing limit — taker fee applies.
    Taker,
    /// Passive limit that rested and filled — maker fee applies.
    Maker,
}

// ---------------------------------------------------------------------------
// Per-decision LLM metadata
// ---------------------------------------------------------------------------

/// A tool call record embedded in a `DecisionTrace`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ToolCall {
    /// Tool name as declared in the strategy's tool registry.
    pub name: String,
    /// Input JSON blob (open-ended).
    #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
    #[serde(default)]
    pub input: serde_json::Value,
}

/// Per-decision LLM provenance record — attaches alongside the fill outcome
/// in the `decisions.jsonl` sidecar.
///
/// All fields default to zero-values so old runs (schema_version="1") that
/// lack these fields deserialize without error.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DecisionTrace {
    /// Model identifier used for this decision (e.g. `"claude-opus-4-7"`).
    #[serde(default)]
    pub model_id: String,
    /// SHA-256 hex digest of the prompt template string. Stable across runs
    /// using the same prompt version; changes on any prompt edit.
    #[serde(default)]
    pub prompt_template_hash: String,
    /// Sampling temperature applied to this decision.
    #[serde(default)]
    pub temperature: f32,
    /// `top_p` applied to this decision.
    #[serde(default)]
    pub top_p: f32,
    /// Random seed, if deterministic sampling was requested.
    #[serde(default)]
    pub seed: u64,
    /// Input tokens consumed (prompt + system).
    #[serde(default)]
    pub tokens_in: u32,
    /// Output tokens generated (completion).
    #[serde(default)]
    pub tokens_out: u32,
    /// Wall-clock LLM round-trip time in milliseconds.
    #[serde(default)]
    pub latency_ms: u32,
    /// Tools invoked during this decision cycle.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools_called: Vec<ToolCall>,
}

/// Per-fill provenance record — appended to the fill columns in
/// `decisions.jsonl` so downstream tracks have a stable slot to write into.
///
/// All fields default to zero-values / `None` so old runs (schema_version="1")
/// deserialize cleanly.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FillTrace {
    /// Intra-bar fill branch taken. `None` when not populated (pre-intra-bar
    /// ordering) or for non-fill decisions (hold/flat-no-position).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_branch: Option<FillBranch>,
    /// Slippage applied in basis points.
    #[serde(default)]
    pub slip_bps_applied: f64,
    /// Half-spread applied in basis points.
    #[serde(default)]
    pub spread_bps_applied: f64,
    /// Fee applied in basis points.
    #[serde(default)]
    pub fee_bps_applied: f64,
    /// Source of the fee rate.
    #[serde(default)]
    pub fee_source: FeeSource,
    /// Fill quantity as a fraction of the bar's traded volume.
    /// `None` when volume-share slippage is not modelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_share: Option<f64>,
    /// True when the fill was capped by the venue's volume-share limit.
    #[serde(default)]
    pub volume_cap_bound: bool,
    /// Aggressor side for this fill. `None` when not classified (pre-intra-bar
    /// ordering track).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggressor_side: Option<AggressorSide>,
}

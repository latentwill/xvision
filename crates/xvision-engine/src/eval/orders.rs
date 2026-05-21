//! Order state enum for the eval simulator.
//!
//! `OrderState` tracks the lifecycle of a simulated order within the backtest
//! engine. This is the minimal enum sufficient for V2E — no queue model,
//! no multi-bar partial-fill carry loop (deferred). The `PartiallyFilled`
//! variant exists so `eval-cost-model-per-bar-and-volume-share`'s volume-cap-
//! binding case can write into it; the carry loop is a follow-up wave.
//!
//! Schema note: `OrderState` is a JSONL field on the decisions wire format
//! (not a DB column). Old runs that lack the field deserialize as `None`
//! via `#[serde(default)]` wrappers on the containing struct.

use serde::{Deserialize, Serialize};

/// Lifecycle state of a simulated order in the backtest engine.
///
/// # Variant notes
///
/// - `Open` — order was submitted but has not triggered yet (e.g. a limit
///   order whose price level has not been reached).
/// - `PartiallyFilled` — the volume cap in `eval-cost-model-per-bar-and-
///   volume-share` bound; the cap-sized portion was filled and the remainder
///   is conceptually still open. Full carry-to-next-bar mechanics are deferred.
/// - `Filled` — order filled completely.
/// - `Cancelled` — order was cancelled before filling.
/// - `Expired` — order expired (e.g. DAY TIF past session close).
/// - `Rejected` — order was rejected at order-emission time (broker rule
///   violation, insufficient funds, etc.).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderState {
    /// Submitted; not yet triggered or filled.
    Open,
    /// Volume cap bound — filled up to the cap, remainder conceptually open.
    PartiallyFilled,
    /// Order filled completely.
    Filled,
    /// Cancelled before filling.
    Cancelled,
    /// Expired (e.g. DAY TIF past session close).
    Expired,
    /// Rejected at order-emission time.
    Rejected,
}

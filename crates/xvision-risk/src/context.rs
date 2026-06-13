//! Evaluation context passed to each `RiskRule`.
//!
//! `RiskEvalContext` bundles everything a rule needs to make its verdict.
//! It wraps the `TraderDecision` and the `PortfolioState` (which the old
//! signature passed separately) and adds `conviction` — the trader's stated
//! confidence in the decision on a 0..1 scale.
//!
//! **Conviction is informational.** The engine never applies a default
//! `size *= conviction` mapping. User-authored rules that want to scale sizing
//! by conviction can read `ctx.conviction`; the built-in ruleset ignores it.

use xvision_core::{AssetSymbol, PortfolioState, TraderDecision};

/// Context for a single rule evaluation pass.
///
/// Passed to [`crate::RiskRule::evaluate`] so every rule sees the same
/// bundle. Callers that do not have a conviction signal should pass `0.0`
/// (neutral); the built-in rules ignore the field entirely.
#[derive(Debug)]
pub struct RiskEvalContext<'a> {
    /// The trader's decision being evaluated (may have been modified by a
    /// prior rule in the chain).
    pub decision: &'a TraderDecision,
    /// Current portfolio snapshot.
    pub portfolio: &'a PortfolioState,
    /// Authoritative asset for this evaluation cycle.
    pub asset: AssetSymbol,
    /// Trader's stated confidence in this decision, 0.0..=1.0.
    ///
    /// Defaults to `0.0` when the caller has no conviction signal. The
    /// engine's built-in rules do **not** use this value; it is exposed
    /// solely so user-authored rules can opt into conviction-scaled sizing.
    pub conviction: f32,
    /// Latest perp funding rate for `asset`, in the same units as
    /// [`xvision_core::OnchainPanel::funding_rate_8h`] (positive ⇒ longs pay
    /// shorts). `None` when the caller has no funding signal — funding-aware
    /// rules (e.g. `FundingCarryGuard`) then no-op (fail-safe). Spot/backtest
    /// paths leave this `None`; the live perps path populates it.
    pub funding_rate_8h: Option<f64>,
}

/// Live market context threaded into [`crate::RiskLayer::evaluate_with_market`].
///
/// Bundles the per-cycle market signals that perps-aware rules need but that
/// live on neither the `TraderDecision` nor the `PortfolioState`. Every field
/// is optional and defaults to `None`; spot/backtest callers pass
/// `MarketContext::default()` so perps-aware rules no-op (fail-safe). The live
/// perps path populates the fields it has.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MarketContext {
    /// Latest perp funding rate for the cycle's asset (same units as
    /// [`xvision_core::OnchainPanel::funding_rate_8h`]; positive ⇒ longs pay
    /// shorts). Consumed by `FundingCarryGuard`.
    pub funding_rate_8h: Option<f64>,
}

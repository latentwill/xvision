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
}

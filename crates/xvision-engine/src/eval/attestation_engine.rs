//! §3.6 attestation engine hook — composes the rolling 20-trade trigger and
//! the verdict into a single call the live-run loop can invoke (Task C6).
//!
//! This is the **engine-side composition** that sits between the pure units in
//! [`crate::eval::attestation_verdict`] and the on-chain bridge in
//! `xvision-identity::attestation`. It is deliberately chain-free: it decides
//! *whether* an attestation should fire and *what verdict* it carries, and
//! emits an [`AttestationTrigger`] describing the post the caller should make.
//! The caller (live loop) is responsible for:
//!
//! 1. recording the off-chain Ed25519 pre-anchor (see
//!    [`crate::eval::attestation`]), and then
//! 2. handing [`AttestationTrigger::verdict_value`] to
//!    `xvision_identity::IdentityClient::submit_attestation`, which applies the
//!    license + deploy gate and posts on-chain.
//!
//! ## Integration seams (documented, deferred per C6 scope)
//!
//! C6 ships the pure logic + this composition unit + the bridge, all
//! unit-tested without a live chain. Two pieces of *wiring* are intentionally
//! deferred because their data sources are not cleanly reachable engine-side
//! yet:
//!
//! - **Seam A — per-trade realized-return series.** The live loop counts
//!   completed trades via `n_trades` in `executor/backtest.rs`, but it does
//!   NOT currently accumulate the per-trade realized-return series needed for
//!   a windowed Sharpe. `decide_one_live` has `fill.realized_pnl` in scope and
//!   returns a `LiveDecisionOutcome`; threading the per-trade return out of
//!   that call and into a rolling buffer in the live driver is the hook-up
//!   point. [`maybe_attest`] takes the return series + trade count as inputs
//!   so it is testable today; the live driver passes its accumulated buffer
//!   once Seam A is wired.
//!
//! - **Seam B — listing's claimed Sharpe (`listed_sharpe`).** The claimed
//!   Sharpe lives in the marketplace listing manifest (`PublicManifest` /
//!   `MarketplaceData` seam), which is deploy/C7-gated and not reachable
//!   engine-side yet. [`maybe_attest`] takes `listed_sharpe` as a parameter;
//!   wiring "where it comes from" is owned by the marketplace data layer.
//!
//! When both seams land, the live driver calls [`maybe_attest`] after each
//! completed trade with `(n_trades, &per_trade_returns, periods_per_year,
//! listed_sharpe)` and, on `Some(trigger)`, performs the pre-anchor + gated
//! on-chain submission.

use crate::eval::attestation_verdict::{should_fire, verdict, window_sharpe, Verdict};

/// A fired §3.6 attestation: the computed verdict plus the live window Sharpe
/// that produced it. The caller turns this into a pre-anchor record + a gated
/// on-chain `giveFeedback`.
#[derive(Debug, Clone, PartialEq)]
pub struct AttestationTrigger {
    /// The completed-trade count that tripped the trigger (a multiple of 20).
    pub at_trade_count: u32,
    /// Live Sharpe over the trailing 20-trade window.
    pub live_window_sharpe: f64,
    /// The listing's claimed Sharpe this verdict was measured against.
    pub listed_sharpe: f64,
    /// The §3.6 verdict (tag1/tag2/value/label).
    pub verdict: Verdict,
}

impl AttestationTrigger {
    /// Convenience: the platform-fixed `tradingYield` value (100|50|0) to pass
    /// to the on-chain bridge.
    pub fn verdict_value(&self) -> u8 {
        self.verdict.value
    }
}

/// Decide whether an attestation should fire at the current completed-trade
/// count and, if so, compute the §3.6 verdict.
///
/// Returns `None` off a 20-trade boundary (no attestation due). On a boundary,
/// computes the trailing-20 window Sharpe from `per_trade_returns` (annualized
/// by `periods_per_year`) and the verdict against `listed_sharpe`.
///
/// Pure and chain-free — see the module doc for the two integration seams the
/// caller is responsible for (per-trade return accumulation; `listed_sharpe`
/// source).
pub fn maybe_attest(
    completed_trades: u32,
    per_trade_returns: &[f64],
    periods_per_year: f64,
    listed_sharpe: f64,
) -> Option<AttestationTrigger> {
    if !should_fire(completed_trades) {
        return None;
    }
    let live = window_sharpe(per_trade_returns, periods_per_year);
    Some(AttestationTrigger {
        at_trade_count: completed_trades,
        live_window_sharpe: live,
        listed_sharpe,
        verdict: verdict(live, listed_sharpe),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::attestation_verdict::VerdictLabel;

    fn returns(n: usize, per: f64) -> Vec<f64> {
        // n identical-ish positive returns with a tiny ramp so std_dev > 0.
        (0..n).map(|i| per + (i as f64) * 1e-5).collect()
    }

    #[test]
    fn no_trigger_off_boundary() {
        assert!(maybe_attest(19, &returns(19, 0.01), 252.0, 1.0).is_none());
        assert!(maybe_attest(21, &returns(21, 0.01), 252.0, 1.0).is_none());
        assert!(maybe_attest(0, &[], 252.0, 1.0).is_none());
    }

    #[test]
    fn fires_on_boundary_with_verdict() {
        let r = returns(20, 0.01);
        let t = maybe_attest(20, &r, 252.0, 1.0).expect("should fire at 20");
        assert_eq!(t.at_trade_count, 20);
        assert_eq!(t.listed_sharpe, 1.0);
        // window sharpe matches the standalone helper.
        assert_eq!(t.live_window_sharpe, window_sharpe(&r, 252.0));
        assert_eq!(t.verdict.tag1, "tradingYield");
        assert_eq!(t.verdict.tag2, "month");
    }

    #[test]
    fn refires_every_20() {
        let r = returns(60, 0.01);
        assert!(maybe_attest(40, &r, 252.0, 1.0).is_some());
        assert!(maybe_attest(60, &r, 252.0, 1.0).is_some());
    }

    #[test]
    fn strong_live_against_modest_claim_endorses() {
        // Clean positive returns → high Sharpe; modest listed claim → Endorses.
        let r = returns(20, 0.02);
        let t = maybe_attest(20, &r, 252.0, 0.5).unwrap();
        assert!(t.live_window_sharpe > 0.0);
        assert_eq!(t.verdict.label, VerdictLabel::Endorses);
        assert_eq!(t.verdict_value(), 100);
    }

    #[test]
    fn negative_live_against_positive_claim_rejects() {
        // All-negative trailing returns → negative Sharpe vs a positive claim.
        let r: Vec<f64> = (0..20).map(|i| -0.02 - (i as f64) * 1e-5).collect();
        let t = maybe_attest(20, &r, 252.0, 1.5).unwrap();
        assert!(t.live_window_sharpe < 0.0);
        assert_eq!(t.verdict.label, VerdictLabel::Rejects);
        assert_eq!(t.verdict_value(), 0);
    }
}

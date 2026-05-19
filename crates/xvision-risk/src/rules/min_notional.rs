//! Rule: order notional must meet the venue's deterministic minimum.
//!
//! Operator-reported failure mode 2026-05-19: paper-venue ETH/USD orders
//! sized ~$6 (0.00274 ETH × ~$2,200) were submitted every decision cycle
//! and rejected by Alpaca with `cost basis must be >= minimal amount of
//! order 10`. The post-submit classifier (PR #314) handles the rejection
//! recoverably, but every cycle still pays the broker round-trip to
//! learn what we already know.
//!
//! This rule reads the per-venue `min_notional_usd` from `RiskConfig`
//! and vetoes any decision whose notional (computed as `equity_usd ×
//! size_bps / 10_000`) is strictly below the configured minimum. Vetoes
//! short-circuit the layer before any stop-loss / take-profit checks,
//! so we don't burn cycles validating stops on an order that's about
//! to be killed.
//!
//! Defaults to a no-op when the venue is unconfigured or its minimum
//! is `0.0` — preserves today's pass-everything behavior on venues that
//! haven't been mapped yet.

use xvision_core::{Action, AssetSymbol, PortfolioState, TraderDecision, VetoReason};

use crate::{RiskRule, RuleVerdict};

pub struct MinNotional {
    /// Minimum notional in USD. `0.0` disables the rule.
    pub min_notional_usd: f64,
    /// Venue id this rule was instantiated for; used only for tracing.
    pub venue_id: String,
}

impl RiskRule for MinNotional {
    fn name(&self) -> &'static str {
        "MinNotional"
    }

    fn evaluate(
        &self,
        decision: &TraderDecision,
        portfolio: &PortfolioState,
        _asset: AssetSymbol,
    ) -> RuleVerdict {
        // No-op when unconfigured. Matches pre-rule behavior on venues
        // we haven't catalogued yet (zero is the serde default for
        // `VenueLimits::min_notional_usd`).
        if self.min_notional_usd <= 0.0 {
            return RuleVerdict::Pass;
        }
        // Non-actionable decisions don't produce an order — let them
        // through so a `hold` or `flat` near a tiny portfolio doesn't
        // surface as a notional veto.
        if !matches!(decision.action, Action::Buy | Action::Sell) {
            return RuleVerdict::Pass;
        }
        // Zero-size decisions are degenerate but harmless; treat them
        // as a pass and let other rules / executor guards decide.
        if decision.size_bps == 0 {
            return RuleVerdict::Pass;
        }
        let notional = portfolio.equity_usd * (decision.size_bps as f64) / 10_000.0;
        if notional < self.min_notional_usd {
            tracing::debug!(
                venue = %self.venue_id,
                min = self.min_notional_usd,
                notional,
                equity = portfolio.equity_usd,
                size_bps = decision.size_bps,
                "MinNotional veto"
            );
            RuleVerdict::Veto(VetoReason::BelowVenueMinNotional)
        } else {
            RuleVerdict::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{flat_portfolio, make_decision, make_portfolio};
    use xvision_core::{Action, AssetSymbol, Direction};

    fn rule_paper() -> MinNotional {
        MinNotional {
            min_notional_usd: 10.0,
            venue_id: "paper".into(),
        }
    }

    #[test]
    fn pass_when_min_is_zero() {
        let rule = MinNotional {
            min_notional_usd: 0.0,
            venue_id: "unset".into(),
        };
        // Tiny equity + tiny size → ~$0.01 notional; still passes
        // because the venue is unconfigured (no-op).
        let portfolio = make_portfolio(100.0, 0.0);
        let d = make_decision(Action::Buy, Direction::Long, 1, 2.0, 5.0);
        assert!(matches!(
            rule.evaluate(&d, &portfolio, AssetSymbol::Eth),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn veto_below_min() {
        // $1000 equity × 50 bps = $5 notional, below $10 minimum.
        let portfolio = make_portfolio(1000.0, 0.0);
        let d = make_decision(Action::Buy, Direction::Long, 50, 2.0, 5.0);
        assert!(matches!(
            rule_paper().evaluate(&d, &portfolio, AssetSymbol::Eth),
            RuleVerdict::Veto(VetoReason::BelowVenueMinNotional)
        ));
    }

    #[test]
    fn pass_above_min() {
        // $100_000 equity × 1500 bps = $15_000 notional, well above $10.
        let portfolio = flat_portfolio();
        let d = make_decision(Action::Buy, Direction::Long, 1500, 2.0, 5.0);
        assert!(matches!(
            rule_paper().evaluate(&d, &portfolio, AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn pass_exactly_at_min() {
        // Equal to the minimum is allowed (strict less-than veto).
        // $1000 × 100 bps = $10 notional, equals $10 minimum.
        let portfolio = make_portfolio(1000.0, 0.0);
        let d = make_decision(Action::Buy, Direction::Long, 100, 2.0, 5.0);
        assert!(matches!(
            rule_paper().evaluate(&d, &portfolio, AssetSymbol::Eth),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn pass_for_non_actionable() {
        // `Flat` / `Close` actions don't produce orders; the rule
        // shouldn't second-guess them. Use a tiny portfolio so size_bps
        // × equity would be below the min if it mattered.
        let portfolio = make_portfolio(100.0, 0.0);
        for action in [Action::Flat, Action::Close] {
            let d = make_decision(action, Direction::Flat, 50, 2.0, 5.0);
            assert!(
                matches!(
                    rule_paper().evaluate(&d, &portfolio, AssetSymbol::Eth),
                    RuleVerdict::Pass
                ),
                "non-actionable {:?} should pass MinNotional",
                action,
            );
        }
    }

    #[test]
    fn pass_for_zero_size() {
        // Zero size is degenerate; let other rules handle it.
        let portfolio = make_portfolio(1000.0, 0.0);
        let d = make_decision(Action::Buy, Direction::Long, 0, 2.0, 5.0);
        assert!(matches!(
            rule_paper().evaluate(&d, &portfolio, AssetSymbol::Eth),
            RuleVerdict::Pass
        ));
    }
}

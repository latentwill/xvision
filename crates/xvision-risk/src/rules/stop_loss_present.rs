//! Rule: stop-loss must be present and within bounds.
//!
//! - Missing (< min) → Veto(StopLossMissing)
//! - Too wide (> max) → Modify (clamp to max, reason StopLossTooWide)

use xvision_core::{Action, AssetSymbol, PortfolioState, TraderDecision, VetoReason};

use crate::{RiskRule, RuleVerdict};

pub struct StopLossPresent {
    pub required: bool,
    pub min_pct: f64,
    pub max_pct: f64,
}

impl RiskRule for StopLossPresent {
    fn name(&self) -> &'static str {
        "StopLossPresent"
    }

    fn evaluate(
        &self,
        decision: &TraderDecision,
        _portfolio: &PortfolioState,
        _asset: AssetSymbol,
    ) -> RuleVerdict {
        // Flat/close decisions don't require a stop.
        if matches!(decision.action, Action::Flat | Action::Close) {
            return RuleVerdict::Pass;
        }

        let sl = decision.stop_loss_pct as f64;

        if self.required && sl < self.min_pct {
            return RuleVerdict::Veto(VetoReason::StopLossMissing);
        }

        if sl > self.max_pct {
            let mut modified = decision.clone();
            modified.stop_loss_pct = self.max_pct as f32;
            return RuleVerdict::Modify(modified, VetoReason::StopLossTooWide);
        }

        RuleVerdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{flat_portfolio, make_decision};
    use xvision_core::{Action, AssetSymbol, Direction, VetoReason};

    fn rule() -> StopLossPresent {
        StopLossPresent {
            required: true,
            min_pct: 0.5,
            max_pct: 10.0,
        }
    }

    #[test]
    fn pass_normal_stop() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn veto_missing_stop() {
        // stop_loss_pct = 0.1 is below min of 0.5
        let d = make_decision(Action::Buy, Direction::Long, 1000, 0.1, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc),
            RuleVerdict::Veto(VetoReason::StopLossMissing)
        ));
    }

    #[test]
    fn modify_stop_too_wide() {
        // stop_loss_pct = 15.0 > max of 10.0
        let d = make_decision(Action::Buy, Direction::Long, 1000, 15.0, 5.0);
        match rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc) {
            RuleVerdict::Modify(modified, VetoReason::StopLossTooWide) => {
                assert!((modified.stop_loss_pct - 10.0).abs() < 1e-6);
            }
            other => panic!("expected Modify, got {other:?}"),
        }
    }

    #[test]
    fn flat_skips_stop_check() {
        let d = make_decision(Action::Flat, Direction::Flat, 0, 0.1, 0.1);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }
}

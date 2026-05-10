//! Rule: take-profit R:R check.
//!
//! - If `take_profit_required` and tp < min → Veto(TakeProfitMissing)
//! - If tp present and tp/sl < min_rr → Modify (widen tp to sl * min_rr)

use xvision_core::{Action, AssetSymbol, PortfolioState, TraderDecision, VetoReason};

use crate::{RiskRule, RuleVerdict};

pub struct TakeProfitRR {
    pub required: bool,
    pub min_rr: f64,
    pub stop_loss_min_pct: f64,
}

impl RiskRule for TakeProfitRR {
    fn name(&self) -> &'static str {
        "TakeProfitRR"
    }

    fn evaluate(
        &self,
        decision: &TraderDecision,
        _portfolio: &PortfolioState,
        _asset: AssetSymbol,
    ) -> RuleVerdict {
        if matches!(decision.action, Action::Flat | Action::Close) {
            return RuleVerdict::Pass;
        }

        let tp = decision.take_profit_pct as f64;
        let sl = decision.stop_loss_pct as f64;

        // If take-profit is missing/negligible and it's required.
        if self.required && tp < self.stop_loss_min_pct {
            return RuleVerdict::Veto(VetoReason::TakeProfitMissing);
        }

        // If tp is present (> 0), check R:R.
        if tp > 0.0 && sl > 0.0 {
            let rr = tp / sl;
            if rr < self.min_rr {
                let required_tp = (sl * self.min_rr) as f32;
                // Clamp to TraderDecision garde max (50.0)
                let required_tp = required_tp.min(50.0);
                let mut modified = decision.clone();
                modified.take_profit_pct = required_tp;
                return RuleVerdict::Modify(modified, VetoReason::Custom("rr_too_low".into()));
            }
        }

        RuleVerdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{flat_portfolio, make_decision};
    use xvision_core::{Action, AssetSymbol, Direction};

    fn rule() -> TakeProfitRR {
        TakeProfitRR {
            required: false,
            min_rr: 1.5,
            stop_loss_min_pct: 0.5,
        }
    }

    fn rule_required() -> TakeProfitRR {
        TakeProfitRR {
            required: true,
            min_rr: 1.5,
            stop_loss_min_pct: 0.5,
        }
    }

    #[test]
    fn pass_good_rr() {
        // sl=2.0, tp=5.0 → rr=2.5 ≥ 1.5
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn modify_poor_rr() {
        // sl=2.0, tp=2.5 → rr=1.25 < 1.5 → widen tp to 3.0
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 2.5);
        match rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc) {
            RuleVerdict::Modify(modified, _) => {
                assert!((modified.take_profit_pct - 3.0).abs() < 0.01);
            }
            other => panic!("expected Modify, got {other:?}"),
        }
    }

    #[test]
    fn veto_missing_when_required() {
        // tp=0.1 < min_pct=0.5, required=true
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 0.1);
        assert!(matches!(
            rule_required().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc),
            RuleVerdict::Veto(VetoReason::TakeProfitMissing)
        ));
    }

    #[test]
    fn pass_missing_when_not_required() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 0.1);
        // rule() has required=false; tp=0.1 < 0.5 → but not required → check rr
        // rr = 0.1/2.0 = 0.05 < 1.5 → modify
        match rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc) {
            RuleVerdict::Modify(_, _) | RuleVerdict::Pass => {} // acceptable
            RuleVerdict::Veto(_) => panic!("should not veto when not required"),
        }
    }
}

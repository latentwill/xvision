//! Rule: single position must not exceed `max_position_pct_nav` of NAV.

use xvision_core::{Action, VetoReason};

use crate::{context::RiskEvalContext, RiskRule, RuleVerdict};

pub struct MaxPositionSize {
    /// Threshold in basis points (e.g. 20% → 2000 bps).
    pub max_bps: u32,
}

impl RiskRule for MaxPositionSize {
    fn name(&self) -> &'static str {
        "MaxPositionSize"
    }

    fn evaluate(&self, ctx: &RiskEvalContext<'_>) -> RuleVerdict {
        if matches!(ctx.decision.action, Action::Flat | Action::Close) {
            return RuleVerdict::Pass;
        }
        if ctx.decision.size_bps > self.max_bps {
            RuleVerdict::Veto(VetoReason::PositionTooLarge)
        } else {
            RuleVerdict::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{flat_portfolio, make_ctx, make_decision};
    use xvision_core::{Action, AssetSymbol, Direction};

    fn rule() -> MaxPositionSize {
        MaxPositionSize { max_bps: 2000 }
    }

    #[test]
    fn pass_within_limit() {
        let d = make_decision(Action::Buy, Direction::Long, 1500, 2.0, 5.0);
        let p = flat_portfolio();
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &p, AssetSymbol::Btc)),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn veto_over_limit() {
        let d = make_decision(Action::Buy, Direction::Long, 2001, 2.0, 5.0);
        let p = flat_portfolio();
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &p, AssetSymbol::Btc)),
            RuleVerdict::Veto(VetoReason::PositionTooLarge)
        ));
    }

    #[test]
    fn pass_exactly_at_limit() {
        let d = make_decision(Action::Buy, Direction::Long, 2000, 2.0, 5.0);
        let p = flat_portfolio();
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &p, AssetSymbol::Btc)),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn close_and_flat_pass_even_when_size_exceeds_limit() {
        let p = flat_portfolio();
        for action in [Action::Close, Action::Flat] {
            let d = make_decision(action, Direction::Flat, 9000, 2.0, 5.0);
            assert!(matches!(
                rule().evaluate(&make_ctx(&d, &p, AssetSymbol::Btc)),
                RuleVerdict::Pass
            ));
        }
    }
}

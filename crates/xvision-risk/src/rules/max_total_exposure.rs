//! Rule: portfolio total exposure must not exceed `max_total_exposure_pct` of NAV.

use xvision_core::{Action, VetoReason};

use crate::{context::RiskEvalContext, RiskRule, RuleVerdict};

pub struct MaxTotalExposure {
    /// Threshold in basis points (e.g. 100% → 10000 bps).
    pub max_bps: u32,
}

impl RiskRule for MaxTotalExposure {
    fn name(&self) -> &'static str {
        "MaxTotalExposure"
    }

    fn evaluate(&self, ctx: &RiskEvalContext<'_>) -> RuleVerdict {
        // Flat/close decisions don't add exposure.
        if matches!(ctx.decision.action, Action::Flat | Action::Close) || ctx.decision.size_bps == 0 {
            return RuleVerdict::Pass;
        }
        let projected = ctx.portfolio.total_exposure_bps() + ctx.decision.size_bps;
        if projected > self.max_bps {
            RuleVerdict::Veto(VetoReason::ExposureCap)
        } else {
            RuleVerdict::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{flat_portfolio, make_ctx, make_decision, portfolio_with_position};
    use xvision_core::{Action, AssetSymbol, Direction};

    fn rule() -> MaxTotalExposure {
        // 100% NAV = 10_000 bps
        MaxTotalExposure { max_bps: 10_000 }
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
    fn veto_when_combined_exceeds_limit() {
        // Existing 9000 bps + new 2000 = 11000 > 10000
        let portfolio = portfolio_with_position(9000);
        let d = make_decision(Action::Buy, Direction::Long, 2000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &portfolio, AssetSymbol::Btc)),
            RuleVerdict::Veto(VetoReason::ExposureCap)
        ));
    }

    #[test]
    fn flat_action_always_passes() {
        let portfolio = portfolio_with_position(9000);
        let d = make_decision(Action::Flat, Direction::Flat, 0, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &portfolio, AssetSymbol::Btc)),
            RuleVerdict::Pass
        ));
    }
}

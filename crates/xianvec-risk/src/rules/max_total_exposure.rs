//! Rule: portfolio total exposure must not exceed `max_total_exposure_pct` of NAV.

use xianvec_core::{Action, AssetSymbol, PortfolioState, TraderDecision, VetoReason};

use crate::{RiskRule, RuleVerdict};

pub struct MaxTotalExposure {
    /// Threshold in basis points (e.g. 100% → 10000 bps).
    pub max_bps: u32,
}

impl RiskRule for MaxTotalExposure {
    fn name(&self) -> &'static str {
        "MaxTotalExposure"
    }

    fn evaluate(
        &self,
        decision: &TraderDecision,
        portfolio: &PortfolioState,
        _asset: AssetSymbol,
    ) -> RuleVerdict {
        // Flat/close decisions don't add exposure.
        if matches!(decision.action, Action::Flat | Action::Close) || decision.size_bps == 0 {
            return RuleVerdict::Pass;
        }
        let projected = portfolio.total_exposure_bps() + decision.size_bps;
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
    use crate::tests_common::{flat_portfolio, make_decision, portfolio_with_position};
    use xianvec_core::{Action, AssetSymbol, Direction};

    fn rule() -> MaxTotalExposure {
        // 100% NAV = 10_000 bps
        MaxTotalExposure { max_bps: 10_000 }
    }

    #[test]
    fn pass_within_limit() {
        let d = make_decision(Action::Buy, Direction::Long, 1500, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn veto_when_combined_exceeds_limit() {
        // Existing 9000 bps + new 2000 = 11000 > 10000
        let portfolio = portfolio_with_position(9000);
        let d = make_decision(Action::Buy, Direction::Long, 2000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &portfolio, AssetSymbol::Btc),
            RuleVerdict::Veto(VetoReason::ExposureCap)
        ));
    }

    #[test]
    fn flat_action_always_passes() {
        let portfolio = portfolio_with_position(9000);
        let d = make_decision(Action::Flat, Direction::Flat, 0, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &portfolio, AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }
}

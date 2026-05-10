//! Rule: asset must be listed and enabled in the whitelist.

use xvision_core::{AssetSymbol, PortfolioState, TraderDecision, VetoReason};

use crate::{whitelist::Whitelist, RiskRule, RuleVerdict};

pub struct AssetWhitelist {
    pub whitelist: Whitelist,
}

impl RiskRule for AssetWhitelist {
    fn name(&self) -> &'static str {
        "AssetWhitelist"
    }

    fn evaluate(
        &self,
        _decision: &TraderDecision,
        _portfolio: &PortfolioState,
        asset: AssetSymbol,
    ) -> RuleVerdict {
        if self.whitelist.is_enabled(asset) {
            RuleVerdict::Pass
        } else {
            RuleVerdict::Veto(VetoReason::AssetNotWhitelisted)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{flat_portfolio, make_decision, test_whitelist};
    use xvision_core::{Action, AssetSymbol, Direction};

    fn rule() -> AssetWhitelist {
        AssetWhitelist {
            whitelist: test_whitelist(),
        }
    }

    #[test]
    fn pass_for_enabled_asset() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn veto_for_disabled_asset() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Eth),
            RuleVerdict::Veto(VetoReason::AssetNotWhitelisted)
        ));
    }

    #[test]
    fn veto_for_unknown_asset() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Sol),
            RuleVerdict::Veto(VetoReason::AssetNotWhitelisted)
        ));
    }
}

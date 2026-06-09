//! Rule: asset must be listed and enabled in the whitelist.

use xvision_core::VetoReason;

use crate::{context::RiskEvalContext, whitelist::Whitelist, RiskRule, RuleVerdict};

pub struct AssetWhitelist {
    pub whitelist: Whitelist,
}

impl RiskRule for AssetWhitelist {
    fn name(&self) -> &'static str {
        "AssetWhitelist"
    }

    fn evaluate(&self, ctx: &RiskEvalContext<'_>) -> RuleVerdict {
        if self.whitelist.is_enabled(ctx.asset) {
            RuleVerdict::Pass
        } else {
            RuleVerdict::Veto(VetoReason::AssetNotWhitelisted)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{flat_portfolio, make_ctx, make_decision, test_whitelist};
    use crate::whitelist::AssetEntry;
    use std::collections::BTreeMap;
    use xvision_core::asset_registry::DataSource;
    use xvision_core::{Action, AssetSymbol, Direction};

    fn rule() -> AssetWhitelist {
        AssetWhitelist {
            whitelist: test_whitelist(),
        }
    }

    #[test]
    fn pass_for_enabled_asset() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        let p = flat_portfolio();
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &p, AssetSymbol::Btc)),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn pass_for_enabled_sol_asset() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        let p = flat_portfolio();
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &p, AssetSymbol::Sol)),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn veto_for_configured_disabled_asset() {
        let mut assets = BTreeMap::new();
        assets.insert(
            AssetSymbol::Eth,
            AssetEntry {
                enabled: false,
                category: "eth".into(),
                data: DataSource::Alpaca,
                venues: BTreeMap::new(),
            },
        );
        let disabled_rule = AssetWhitelist {
            whitelist: Whitelist::from_raw(assets),
        };
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        let p = flat_portfolio();
        assert!(matches!(
            disabled_rule.evaluate(&make_ctx(&d, &p, AssetSymbol::Eth)),
            RuleVerdict::Veto(VetoReason::AssetNotWhitelisted)
        ));
    }

    #[test]
    fn veto_for_missing_asset() {
        let missing_rule = AssetWhitelist {
            whitelist: Whitelist::from_raw(BTreeMap::new()),
        };
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        let p = flat_portfolio();
        assert!(matches!(
            missing_rule.evaluate(&make_ctx(&d, &p, AssetSymbol::Sol)),
            RuleVerdict::Veto(VetoReason::AssetNotWhitelisted)
        ));
    }
}

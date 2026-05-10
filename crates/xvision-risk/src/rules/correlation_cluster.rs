//! Rule: at most `max_correlation_cluster` concurrent open positions in the same cluster.

use xvision_core::{Action, AssetSymbol, PortfolioState, TraderDecision, VetoReason};

use crate::{whitelist::Whitelist, RiskRule, RuleVerdict};

pub struct CorrelationCluster {
    pub max: usize,
    pub whitelist: Whitelist,
}

impl RiskRule for CorrelationCluster {
    fn name(&self) -> &'static str {
        "CorrelationCluster"
    }

    fn evaluate(
        &self,
        decision: &TraderDecision,
        portfolio: &PortfolioState,
        asset: AssetSymbol,
    ) -> RuleVerdict {
        // Closing a position frees a cluster slot — always allow.
        if matches!(decision.action, Action::Flat | Action::Close) {
            return RuleVerdict::Pass;
        }

        let Some(target_cluster) = self.whitelist.cluster_of(asset) else {
            // Unknown asset has no cluster → handled by AssetWhitelist rule.
            return RuleVerdict::Pass;
        };

        let count = portfolio
            .open_positions
            .iter()
            .filter(|(sym, _)| {
                self.whitelist
                    .cluster_of(**sym)
                    .map(|c| c == target_cluster)
                    .unwrap_or(false)
            })
            .count();

        if count + 1 > self.max {
            RuleVerdict::Veto(VetoReason::CorrelationClusterCap)
        } else {
            RuleVerdict::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{flat_portfolio, make_decision, test_whitelist};
    use crate::whitelist::AssetEntry;
    use std::collections::BTreeMap;
    use xvision_core::{Action, AssetSymbol, Direction, OpenPosition, PortfolioState};

    fn rule() -> CorrelationCluster {
        CorrelationCluster {
            max: 2,
            whitelist: test_whitelist(),
        }
    }

    /// Build a custom whitelist where BTC, ETH and SOL are all in the same cluster.
    fn same_cluster_whitelist() -> Whitelist {
        let mut assets = BTreeMap::new();
        for sym in [AssetSymbol::Btc, AssetSymbol::Eth, AssetSymbol::Sol] {
            assets.insert(
                sym,
                AssetEntry {
                    enabled: true,
                    cluster: "btc".into(),
                    venues: BTreeMap::new(),
                },
            );
        }
        Whitelist::from_raw(assets)
    }

    /// Portfolio with BTC and ETH positions (both in "btc" cluster per same_cluster_whitelist).
    fn two_position_portfolio() -> PortfolioState {
        use chrono::Utc;
        let mut positions = BTreeMap::new();
        for (sym, price) in [(AssetSymbol::Btc, 50_000.0), (AssetSymbol::Eth, 2_000.0)] {
            positions.insert(
                sym,
                OpenPosition {
                    asset: sym,
                    direction: Direction::Long,
                    size_bps: 800,
                    entry_price: price,
                    mark_price: price,
                    stop_loss_pct: 2.0,
                    take_profit_pct: 5.0,
                    opened_at: Utc::now(),
                },
            );
        }
        PortfolioState {
            equity_usd: 100_000.0,
            realized_pnl_today_usd: 0.0,
            day_index: 1,
            open_positions: positions,
            as_of: Utc::now(),
        }
    }

    #[test]
    fn pass_when_no_cluster_positions() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn pass_when_one_existing_in_cluster() {
        use crate::tests_common::portfolio_with_btc;
        let portfolio = portfolio_with_btc(1000);
        // Adding BTC again: count=1+1=2 ≤ max=2 → pass.
        let d = make_decision(Action::Buy, Direction::Long, 500, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &portfolio, AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn veto_when_cluster_at_max() {
        // 2 positions already in "btc" cluster; adding a 3rd (SOL) trips the cap.
        let wl = same_cluster_whitelist();
        let rule = CorrelationCluster { max: 2, whitelist: wl };
        let portfolio = two_position_portfolio();
        let d = make_decision(Action::Buy, Direction::Long, 500, 2.0, 5.0);
        assert!(matches!(
            rule.evaluate(&d, &portfolio, AssetSymbol::Sol),
            RuleVerdict::Veto(VetoReason::CorrelationClusterCap)
        ));
    }

    #[test]
    fn flat_always_passes() {
        use crate::tests_common::portfolio_with_btc;
        let portfolio = portfolio_with_btc(1000);
        let d = make_decision(Action::Flat, Direction::Flat, 0, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &portfolio, AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }
}

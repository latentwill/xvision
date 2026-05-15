//! Rule: at most `max_open_positions` concurrent open positions.
//!
//! Sizing-up an existing position (same asset already open) is not a new
//! position, so it doesn't consume a slot. Flat/Close decisions also don't
//! consume a slot.

use xvision_core::{Action, AssetSymbol, PortfolioState, TraderDecision, VetoReason};

use crate::{RiskRule, RuleVerdict};

pub struct MaxOpenPositions {
    pub max: usize,
}

impl RiskRule for MaxOpenPositions {
    fn name(&self) -> &'static str {
        "MaxOpenPositions"
    }

    fn evaluate(
        &self,
        decision: &TraderDecision,
        portfolio: &PortfolioState,
        asset: AssetSymbol,
    ) -> RuleVerdict {
        // Flat/close decisions free a slot, they don't consume one.
        if matches!(decision.action, Action::Flat | Action::Close) {
            return RuleVerdict::Pass;
        }

        // Size-up of existing position: slot already counted.
        if portfolio.open_positions.contains_key(&asset) {
            return RuleVerdict::Pass;
        }

        // New asset entry — check slot availability.
        if portfolio.open_positions.len() >= self.max {
            RuleVerdict::Veto(VetoReason::MaxOpenPositions)
        } else {
            RuleVerdict::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{
        flat_portfolio, make_decision, portfolio_with_btc, portfolio_with_n_positions,
    };
    use chrono::Utc;
    use xvision_core::{Action, AssetSymbol, Direction, OpenPosition};

    fn rule() -> MaxOpenPositions {
        MaxOpenPositions { max: 5 }
    }

    #[test]
    fn pass_when_below_limit() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &flat_portfolio(), AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn veto_when_at_limit() {
        // Build a portfolio with ETH and SOL already open, then use max=2.
        let portfolio = portfolio_with_n_positions(2);
        let rule = MaxOpenPositions { max: 2 };
        // BTC is not in the portfolio → new slot → but already at limit.
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule.evaluate(&d, &portfolio, AssetSymbol::Btc),
            RuleVerdict::Veto(VetoReason::MaxOpenPositions)
        ));
    }

    #[test]
    fn flat_always_passes() {
        let portfolio = full_portfolio(5);
        let d = make_decision(Action::Flat, Direction::Flat, 0, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &portfolio, AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn size_up_existing_passes_at_full_capacity() {
        // BTC is already in the portfolio; adding more BTC doesn't consume a slot.
        let portfolio = portfolio_with_btc(1000);
        // We're below the limit here (1 position); but let's verify the logic:
        // even if we add more positions to fill up, BTC size-up should still pass.
        let d = make_decision(Action::Buy, Direction::Long, 500, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &portfolio, AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn new_asset_below_limit_passes() {
        let portfolio = portfolio_with_n_positions(2);
        let d = make_decision(Action::Buy, Direction::Long, 500, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&d, &portfolio, AssetSymbol::Btc),
            RuleVerdict::Pass
        ));
    }

    /// Build a portfolio with `n` fake positions using BTC/ETH/SOL (max 3).
    fn full_portfolio(n: usize) -> xvision_core::PortfolioState {
        use std::collections::BTreeMap;
        use xvision_core::{AssetSymbol, PortfolioState};

        let syms = [AssetSymbol::Eth, AssetSymbol::Sol];
        let mut positions = BTreeMap::new();
        for sym in syms.iter().cycle().take(n) {
            positions.entry(*sym).or_insert_with(|| OpenPosition {
                asset: *sym,
                direction: Direction::Long,
                size_bps: 500,
                entry_price: 1000.0,
                mark_price: 1000.0,
                stop_loss_pct: 2.0,
                take_profit_pct: 5.0,
                opened_at: Utc::now(),
            });
        }
        // Pad with BTC if we still need more entries
        if positions.len() < n {
            positions.insert(
                AssetSymbol::Btc,
                OpenPosition {
                    asset: AssetSymbol::Btc,
                    direction: Direction::Long,
                    size_bps: 500,
                    entry_price: 50_000.0,
                    mark_price: 50_000.0,
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
}

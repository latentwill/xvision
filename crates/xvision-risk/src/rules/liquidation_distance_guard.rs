//! Rule: liquidation-distance guard — block NEW entries while any open perps
//! position sits within the configured distance of its liquidation price.
//!
//! Perps positions report a `liq_price`; when the mark drifts within
//! `min_liq_distance_pct` of it, the position is one bad tick from forced
//! liquidation. Opening *more* risk in that state is reckless, so a new entry
//! is vetoed (`VetoReason::NearLiquidation`). This is a portfolio-level guard:
//! a near-liquidation position in *any* asset blocks new entries in *any*
//! asset until the operator de-risks.
//!
//! Scope: only perps positions carry a `liq_price`. Spot positions leave it
//! `None`, so the guard no-ops — it costs nothing for agents that never trade
//! perps. Exits (Flat/Close) are always allowed (they reduce risk).

use xvision_core::{Action, VetoReason};

use crate::{context::RiskEvalContext, RiskRule, RuleVerdict};

pub struct LiquidationDistanceGuard {
    /// Minimum distance, as a percent of mark, an open position's liquidation
    /// price must keep before new entries are vetoed. e.g. `5.0` blocks new
    /// entries while any position's liq price is within 5% of its mark.
    pub min_liq_distance_pct: f64,
}

impl RiskRule for LiquidationDistanceGuard {
    fn name(&self) -> &'static str {
        "LiquidationDistanceGuard"
    }

    fn evaluate(&self, ctx: &RiskEvalContext<'_>) -> RuleVerdict {
        // Exits reduce risk — never blocked.
        if matches!(ctx.decision.action, Action::Flat | Action::Close) {
            return RuleVerdict::Pass;
        }
        // Veto a new entry if ANY open position sits within the configured
        // distance of its liquidation price. Positions without a liq price
        // (spot) contribute nothing — the guard no-ops for non-perps.
        for pos in ctx.portfolio.open_positions.values() {
            if let Some(liq) = pos.liq_price {
                if pos.mark_price > 0.0 && liq > 0.0 {
                    let distance_pct = ((pos.mark_price - liq).abs() / pos.mark_price) * 100.0;
                    if distance_pct < self.min_liq_distance_pct {
                        return RuleVerdict::Veto(VetoReason::NearLiquidation);
                    }
                }
            }
        }
        RuleVerdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{make_ctx, make_decision};
    use chrono::Utc;
    use std::collections::BTreeMap;
    use xvision_core::{AssetSymbol, Direction, OpenPosition, PortfolioState};

    /// Portfolio holding one BTC position at `mark` with the given `liq_price`.
    fn portfolio_with_liq(mark: f64, liq_price: Option<f64>) -> PortfolioState {
        let mut open = BTreeMap::new();
        open.insert(
            AssetSymbol::Btc,
            OpenPosition {
                asset: AssetSymbol::Btc,
                direction: Direction::Long,
                size_bps: 1000,
                entry_price: mark,
                mark_price: mark,
                stop_loss_pct: 2.0,
                take_profit_pct: 5.0,
                opened_at: Utc::now(),
                leverage: Some(10.0),
                liq_price,
            },
        );
        PortfolioState {
            equity_usd: 100_000.0,
            realized_pnl_today_usd: 0.0,
            day_index: 0,
            open_positions: open,
            as_of: Utc::now(),
        }
    }

    fn rule() -> LiquidationDistanceGuard {
        LiquidationDistanceGuard {
            min_liq_distance_pct: 5.0,
        }
    }

    #[test]
    fn veto_new_entry_when_position_near_liquidation() {
        // mark 100, liq 97 ⇒ 3% distance < 5% threshold ⇒ veto.
        let pf = portfolio_with_liq(100.0, Some(97.0));
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &pf, AssetSymbol::Btc)),
            RuleVerdict::Veto(VetoReason::NearLiquidation)
        ));
    }

    #[test]
    fn pass_when_position_far_from_liquidation() {
        // mark 100, liq 80 ⇒ 20% distance > 5% ⇒ pass.
        let pf = portfolio_with_liq(100.0, Some(80.0));
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &pf, AssetSymbol::Btc)),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn pass_when_no_liq_price_spot_position() {
        // Spot position (liq_price None) ⇒ guard no-ops.
        let pf = portfolio_with_liq(100.0, None);
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &pf, AssetSymbol::Btc)),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn exits_pass_even_when_near_liquidation() {
        // A Close reduces risk — never blocked.
        let pf = portfolio_with_liq(100.0, Some(97.0));
        let d = make_decision(Action::Close, Direction::Flat, 0, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &pf, AssetSymbol::Btc)),
            RuleVerdict::Pass
        ));
    }
}

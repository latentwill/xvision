//! Rule: daily loss circuit-breaker — only flat/close decisions pass once the
//! daily loss threshold is breached.

use xvision_core::{Action, VetoReason};

use crate::{context::RiskEvalContext, RiskRule, RuleVerdict};

pub struct DailyLossCircuit {
    /// Maximum daily loss as a fraction of equity (e.g. 0.05 = 5%).
    pub max_daily_loss_fraction: f64,
}

impl RiskRule for DailyLossCircuit {
    fn name(&self) -> &'static str {
        "DailyLossCircuit"
    }

    fn evaluate(&self, ctx: &RiskEvalContext<'_>) -> RuleVerdict {
        // Flat/close decisions are always allowed (close-only mode).
        if matches!(ctx.decision.action, Action::Flat | Action::Close) {
            return RuleVerdict::Pass;
        }

        if ctx.portfolio.equity_usd > 0.0 {
            let loss_fraction = ctx.portfolio.realized_pnl_today_usd / ctx.portfolio.equity_usd;
            if loss_fraction < -self.max_daily_loss_fraction {
                return RuleVerdict::Veto(VetoReason::DailyLossCircuitBreaker);
            }
        }
        RuleVerdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{make_ctx, make_decision, make_portfolio};
    use xvision_core::{Action, AssetSymbol, Direction};

    fn rule() -> DailyLossCircuit {
        DailyLossCircuit {
            max_daily_loss_fraction: 0.05,
        }
    }

    #[test]
    fn pass_small_loss() {
        // -2% loss < 5% threshold
        let portfolio = make_portfolio(100_000.0, -2_000.0);
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &portfolio, AssetSymbol::Btc)),
            RuleVerdict::Pass
        ));
    }

    #[test]
    fn veto_over_threshold() {
        // -6% loss > 5% threshold
        let portfolio = make_portfolio(100_000.0, -6_000.0);
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &portfolio, AssetSymbol::Btc)),
            RuleVerdict::Veto(VetoReason::DailyLossCircuitBreaker)
        ));
    }

    #[test]
    fn flat_always_passes_even_over_threshold() {
        let portfolio = make_portfolio(100_000.0, -6_000.0);
        let d = make_decision(Action::Flat, Direction::Flat, 0, 2.0, 5.0);
        assert!(matches!(
            rule().evaluate(&make_ctx(&d, &portfolio, AssetSymbol::Btc)),
            RuleVerdict::Pass
        ));
    }
}

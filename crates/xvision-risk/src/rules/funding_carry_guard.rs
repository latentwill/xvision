//! Rule: funding-aware carry guard — block entries that would open into
//! punitive perp funding, while never blocking exits or favorable carry.
//!
//! Perp funding is a periodic cash flow between longs and shorts. With the
//! sign convention of [`xvision_core::OnchainPanel::funding_rate_8h`]
//! (positive ⇒ longs pay shorts), a *long* entry **pays** `+funding` and a
//! *short* entry **pays** `-funding`. When the rate the position would pay
//! exceeds the configured threshold, the funding cost erodes the edge, so the
//! entry is vetoed. Favorable (carry-positive) funding and sub-threshold
//! funding pass through unchanged; exits are always allowed.
//!
//! Fail-safe: when the live funding signal is absent (`None` — e.g. the
//! spot/backtest paths), the rule no-ops (`Pass`).

use xvision_core::{Action, Direction, VetoReason};

use crate::{context::RiskEvalContext, RiskRule, RuleVerdict};

pub struct FundingCarryGuard {
    /// Maximum perp funding rate (same units as `funding_rate_8h`) the
    /// position may *pay* at entry before the entry is vetoed. A long pays
    /// `+funding`, a short pays `-funding`. e.g. `0.01` blocks entries paying
    /// more than that rate.
    pub max_funding_pay_8h: f64,
}

impl RiskRule for FundingCarryGuard {
    fn name(&self) -> &'static str {
        "FundingCarryGuard"
    }

    fn evaluate(&self, ctx: &RiskEvalContext<'_>) -> RuleVerdict {
        // Exits are never blocked — funding is irrelevant when closing out.
        if matches!(ctx.decision.action, Action::Flat | Action::Close) {
            return RuleVerdict::Pass;
        }
        // Fail-safe: no funding signal (spot/backtest) ⇒ no-op.
        let Some(funding) = ctx.funding_rate_8h else {
            return RuleVerdict::Pass;
        };
        // Rate the position would *pay* per funding interval: with the
        // `funding_rate_8h` convention (positive ⇒ longs pay shorts), a long
        // pays `+funding` and a short pays `-funding`. A negative pay rate
        // means the position *receives* funding (favorable carry).
        let pay_rate = match ctx.decision.direction {
            Direction::Long => funding,
            Direction::Short => -funding,
            // No directional exposure ⇒ no funding obligation.
            Direction::Flat => return RuleVerdict::Pass,
        };
        if pay_rate > self.max_funding_pay_8h {
            RuleVerdict::Veto(VetoReason::PunitiveFunding)
        } else {
            RuleVerdict::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_common::{make_ctx_funding, make_decision, make_portfolio};
    use xvision_core::{Action, AssetSymbol, Direction};

    fn rule() -> FundingCarryGuard {
        FundingCarryGuard {
            max_funding_pay_8h: 0.01,
        }
    }

    fn pf() -> xvision_core::PortfolioState {
        make_portfolio(100_000.0, 0.0)
    }

    #[test]
    fn veto_long_paying_punitive_funding() {
        // Long pays +funding; 0.05 > 0.01 threshold ⇒ veto.
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        let p = pf();
        let ctx = make_ctx_funding(&d, &p, AssetSymbol::Btc, Some(0.05));
        assert!(matches!(
            rule().evaluate(&ctx),
            RuleVerdict::Veto(VetoReason::PunitiveFunding)
        ));
    }

    #[test]
    fn veto_short_paying_punitive_funding() {
        // Short pays -funding; funding=-0.05 ⇒ pay_rate=0.05 > 0.01 ⇒ veto.
        let d = make_decision(Action::Sell, Direction::Short, 1000, 2.0, 5.0);
        let p = pf();
        let ctx = make_ctx_funding(&d, &p, AssetSymbol::Btc, Some(-0.05));
        assert!(matches!(
            rule().evaluate(&ctx),
            RuleVerdict::Veto(VetoReason::PunitiveFunding)
        ));
    }

    #[test]
    fn pass_long_funding_below_threshold() {
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        let p = pf();
        let ctx = make_ctx_funding(&d, &p, AssetSymbol::Btc, Some(0.005));
        assert!(matches!(rule().evaluate(&ctx), RuleVerdict::Pass));
    }

    #[test]
    fn pass_long_favorable_carry() {
        // Negative funding ⇒ long receives carry ⇒ pay_rate negative ⇒ pass.
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        let p = pf();
        let ctx = make_ctx_funding(&d, &p, AssetSymbol::Btc, Some(-0.05));
        assert!(matches!(rule().evaluate(&ctx), RuleVerdict::Pass));
    }

    #[test]
    fn pass_short_favorable_carry() {
        // Positive funding ⇒ short receives carry ⇒ pass.
        let d = make_decision(Action::Sell, Direction::Short, 1000, 2.0, 5.0);
        let p = pf();
        let ctx = make_ctx_funding(&d, &p, AssetSymbol::Btc, Some(0.05));
        assert!(matches!(rule().evaluate(&ctx), RuleVerdict::Pass));
    }

    #[test]
    fn pass_when_funding_unknown() {
        // Fail-safe: no funding signal ⇒ never block.
        let d = make_decision(Action::Buy, Direction::Long, 1000, 2.0, 5.0);
        let p = pf();
        let ctx = make_ctx_funding(&d, &p, AssetSymbol::Btc, None);
        assert!(matches!(rule().evaluate(&ctx), RuleVerdict::Pass));
    }

    #[test]
    fn exits_always_pass_even_in_punitive_funding() {
        // A Close must never be blocked, regardless of funding.
        let d = make_decision(Action::Close, Direction::Flat, 0, 2.0, 5.0);
        let p = pf();
        let ctx = make_ctx_funding(&d, &p, AssetSymbol::Btc, Some(0.5));
        assert!(matches!(rule().evaluate(&ctx), RuleVerdict::Pass));
    }
}

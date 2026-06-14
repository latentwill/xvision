//! Perps entry vetoes for the engine R3 risk path. Pure; venue-gated.
//!
//! Funding-carry and liquidation-distance checks, lifted from the retired
//! `xvision-risk` `FundingCarryGuard` / `LiquidationDistanceGuard` rules.
//! Both fail-safe to no-op when the relevant datum is absent (spot/backtest)
//! and never fire unless `is_perp_venue` is true.

use xvision_core::trading::{Direction, VetoReason};

use super::RiskConfig;

/// Decide whether a NEW open should be vetoed on perps risk grounds.
/// Returns `None` (allow) when not a perps venue, not a new open, or when
/// the gating data is absent.
///
/// - `is_perp_venue`: from `BrokerSurface::is_perp_venue()` (false on spot).
/// - `is_new_open`: true only for `long_open` / `short_open`.
/// - `funding_rate_8h`: `PerpsContext.funding_rate` (None ⇒ funding check skipped).
/// - `min_position_liq_distance_pct`: smallest liq-distance % across open
///   positions (None ⇒ liquidation check skipped; populated by the follow-on
///   data-plumbing track).
pub fn perps_entry_veto(
    cfg: &RiskConfig,
    is_perp_venue: bool,
    is_new_open: bool,
    direction: Direction,
    funding_rate_8h: Option<f64>,
    min_position_liq_distance_pct: Option<f64>,
) -> Option<VetoReason> {
    if !is_perp_venue || !is_new_open {
        return None;
    }
    // Funding-carry: a long pays +funding, a short pays -funding.
    if cfg.max_funding_pay_8h > 0.0 {
        if let Some(funding) = funding_rate_8h {
            let pay_rate = match direction {
                Direction::Long => funding,
                Direction::Short => -funding,
                Direction::Flat => return None,
            };
            if pay_rate > cfg.max_funding_pay_8h {
                return Some(VetoReason::PunitiveFunding);
            }
        }
    }
    // Liquidation-distance: any open position within the configured % of liq.
    if cfg.min_liq_distance_pct > 0.0 {
        if let Some(dist) = min_position_liq_distance_pct {
            if dist < cfg.min_liq_distance_pct {
                return Some(VetoReason::NearLiquidation);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::risk::RiskPreset;

    fn cfg() -> RiskConfig {
        let mut c = RiskPreset::Balanced.expand();
        c.max_funding_pay_8h = 0.01;
        c.min_liq_distance_pct = 5.0;
        c
    }

    #[test]
    fn no_op_on_spot_venue() {
        assert_eq!(
            perps_entry_veto(&cfg(), false, true, Direction::Long, Some(0.5), Some(1.0)),
            None
        );
    }

    #[test]
    fn no_op_when_not_new_open() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, false, Direction::Long, Some(0.5), Some(1.0)),
            None
        );
    }

    #[test]
    fn veto_long_paying_punitive_funding() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Long, Some(0.05), None),
            Some(VetoReason::PunitiveFunding)
        );
    }

    #[test]
    fn short_receives_funding_passes() {
        // Short pays -funding; +0.05 funding ⇒ short receives ⇒ pass.
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Short, Some(0.05), None),
            None
        );
    }

    #[test]
    fn absent_funding_is_no_op() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Long, None, None),
            None
        );
    }

    #[test]
    fn veto_near_liquidation() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Long, None, Some(2.0)),
            Some(VetoReason::NearLiquidation)
        );
    }

    #[test]
    fn liq_distance_above_threshold_passes() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Long, None, Some(9.0)),
            None
        );
    }
}

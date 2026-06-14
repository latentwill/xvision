use serde::{Deserialize, Serialize};

pub mod perps;

fn default_max_position_pct_nav() -> f64 {
    20.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskConfig {
    pub risk_pct_per_trade: f64, // e.g., 0.015 = 1.5%
    pub max_concurrent_positions: u32,
    pub max_leverage: f64,
    pub stop_loss_atr_multiple: f64,
    pub daily_loss_kill_pct: f64, // e.g., 0.05 = 5%
    #[serde(default = "default_max_position_pct_nav")]
    pub max_position_pct_nav: f64,
    /// Maximum perp funding rate (8h, same units as
    /// `PerpsContext.funding_rate`) an entry may *pay* before it is vetoed.
    /// A long pays `+funding`, a short pays `-funding`. Perps-venue only.
    /// `0.0` disables. Default 0.0 so spot configs are unaffected.
    #[serde(default)]
    pub max_funding_pay_8h: f64,
    /// Minimum distance (percent of mark) an open perps position's
    /// liquidation price must keep before new entries are vetoed.
    /// Perps-venue only. `0.0` disables. Default 0.0.
    #[serde(default)]
    pub min_liq_distance_pct: f64,
    /// Maximum total open exposure (sum of position notionals as percent of
    /// NAV) a new open may push the book to. General control (spot + perps).
    /// `0.0` disables. Default 0.0 so existing behavior is unchanged.
    #[serde(default)]
    pub max_total_exposure_pct: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskPreset {
    Conservative,
    Balanced,
    Aggressive,
}

impl RiskPreset {
    pub fn expand(self) -> RiskConfig {
        match self {
            RiskPreset::Conservative => RiskConfig {
                risk_pct_per_trade: 0.010,
                max_concurrent_positions: 1,
                max_leverage: 2.0,
                stop_loss_atr_multiple: 2.0,
                daily_loss_kill_pct: 0.03,
                max_position_pct_nav: 20.0,
                max_funding_pay_8h: 0.01,
                min_liq_distance_pct: 8.0,
                max_total_exposure_pct: 100.0,
            },
            RiskPreset::Balanced => RiskConfig {
                risk_pct_per_trade: 0.015,
                max_concurrent_positions: 2,
                max_leverage: 3.0,
                stop_loss_atr_multiple: 2.0,
                daily_loss_kill_pct: 0.05,
                max_position_pct_nav: 20.0,
                max_funding_pay_8h: 0.02,
                min_liq_distance_pct: 5.0,
                max_total_exposure_pct: 150.0,
            },
            RiskPreset::Aggressive => RiskConfig {
                risk_pct_per_trade: 0.025,
                max_concurrent_positions: 3,
                max_leverage: 5.0,
                stop_loss_atr_multiple: 1.5,
                daily_loss_kill_pct: 0.08,
                max_position_pct_nav: 20.0,
                max_funding_pay_8h: 0.05,
                min_liq_distance_pct: 3.0,
                max_total_exposure_pct: 250.0,
            },
        }
    }
}

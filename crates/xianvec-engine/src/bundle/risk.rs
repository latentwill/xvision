use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskConfig {
    pub risk_pct_per_trade: f64, // e.g., 0.015 = 1.5%
    pub max_concurrent_positions: u32,
    pub max_leverage: f64,
    pub stop_loss_atr_multiple: f64,
    pub daily_loss_kill_pct: f64, // e.g., 0.05 = 5%
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
            },
            RiskPreset::Balanced => RiskConfig {
                risk_pct_per_trade: 0.015,
                max_concurrent_positions: 2,
                max_leverage: 3.0,
                stop_loss_atr_multiple: 2.0,
                daily_loss_kill_pct: 0.05,
            },
            RiskPreset::Aggressive => RiskConfig {
                risk_pct_per_trade: 0.025,
                max_concurrent_positions: 3,
                max_leverage: 5.0,
                stop_loss_atr_multiple: 1.5,
                daily_loss_kill_pct: 0.08,
            },
        }
    }
}

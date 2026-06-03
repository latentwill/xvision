use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskConfig {
    pub risk_pct_per_trade: f64, // e.g., 0.015 = 1.5%
    pub max_concurrent_positions: u32,
    pub max_leverage: f64,
    pub stop_loss_atr_multiple: f64,
    pub daily_loss_kill_pct: f64, // e.g., 0.05 = 5%
    /// Take-profit distance as a multiple of ATR at entry. The deterministic
    /// config-side target the backtest exit engine uses when the trader does
    /// not emit `take_profit_pct` (eval-trader risk-parity spec 2026-06-03).
    /// `0.0` disables the config target. `#[serde(default)]` so strategies
    /// persisted before this field hydrate without a migration (strategies are
    /// filesystem JSON).
    #[serde(default = "default_take_profit_atr_multiple")]
    pub take_profit_atr_multiple: f64,
    /// ATR period (Wilder) used for the config-side stop/target levels.
    #[serde(default = "default_atr_period")]
    pub atr_period: u32,
}

fn default_take_profit_atr_multiple() -> f64 {
    3.0
}

fn default_atr_period() -> u32 {
    14
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
                take_profit_atr_multiple: 3.0,
                atr_period: 14,
            },
            RiskPreset::Balanced => RiskConfig {
                risk_pct_per_trade: 0.015,
                max_concurrent_positions: 2,
                max_leverage: 3.0,
                stop_loss_atr_multiple: 2.0,
                daily_loss_kill_pct: 0.05,
                take_profit_atr_multiple: 3.0,
                atr_period: 14,
            },
            RiskPreset::Aggressive => RiskConfig {
                risk_pct_per_trade: 0.025,
                max_concurrent_positions: 3,
                max_leverage: 5.0,
                stop_loss_atr_multiple: 1.5,
                daily_loss_kill_pct: 0.08,
                take_profit_atr_multiple: 2.5,
                atr_period: 14,
            },
        }
    }
}

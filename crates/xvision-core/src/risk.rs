//! Capital + portfolio-level risk caps that live on the **StrategyBundle**.
//!
//! These types moved off `Scenario` in CS-M2 Task 5 — capital and risk are
//! properties of the *strategy*, not the *world* it runs in. The same scenario
//! can host both a conservative and an aggressive bundle, and each carries its
//! own initial capital and risk caps.
//!
//! Distinct from per-trade `RiskConfig` (which still lives in
//! `xvision-engine/src/bundle/risk.rs` and covers `risk_pct_per_trade`,
//! `stop_loss_atr_multiple`, etc). `RiskCaps` here is the *portfolio* envelope:
//! how many positions at once, max leverage, daily-loss kill switch.

use serde::{Deserialize, Serialize};

/// Initial trading capital allocated to a bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capital {
    pub initial: f64,
    pub currency: String,
}

impl Default for Capital {
    fn default() -> Self {
        Self {
            initial: 100_000.0,
            currency: "USD".to_string(),
        }
    }
}

/// Portfolio-level risk envelope (max concurrent positions, max leverage,
/// daily-loss kill switch). Separate from the per-trade `RiskConfig` already
/// on `StrategyBundle`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskCaps {
    pub max_concurrent_positions: u32,
    pub max_leverage: f64,
    pub daily_loss_kill_switch_pct: f64,
}

impl Default for RiskCaps {
    fn default() -> Self {
        Self {
            max_concurrent_positions: 1,
            max_leverage: 1.0,
            daily_loss_kill_switch_pct: 0.05,
        }
    }
}

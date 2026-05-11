pub mod manifest;
pub mod risk;
pub mod slot;
pub mod store;
pub mod validate;

use serde::{Deserialize, Serialize};
use xvision_core::{Capital, RiskCaps};

use crate::bundle::manifest::PublicManifest;
use crate::bundle::risk::RiskConfig;
use crate::bundle::slot::LLMSlot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyBundle {
    pub manifest: PublicManifest,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub regime_slot: Option<LLMSlot>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub intern_slot: Option<LLMSlot>,

    /// At least one slot must be filled; trader is required.
    pub trader_slot: Option<LLMSlot>,

    /// Per-trade risk config (risk_pct_per_trade, stop_loss_atr_multiple, ...).
    pub risk: RiskConfig,

    /// Initial trading capital allocated to the bundle. Moved off `Scenario`
    /// in CS-M2 Task 5 — capital is a property of the strategy, not the
    /// world. `#[serde(default)]` keeps pre-Task-5 bundles deserializable.
    #[serde(default)]
    pub capital: Capital,

    /// Portfolio-level risk caps (max concurrent positions, max leverage,
    /// daily-loss kill switch). Distinct from `risk: RiskConfig` above,
    /// which covers per-trade sizing. Moved off `Scenario` in CS-M2 Task 5.
    #[serde(default)]
    pub risk_caps: RiskCaps,

    /// Template-specific mechanical params (e.g., rsi thresholds, EMA periods).
    pub mechanical_params: serde_json::Value,
}

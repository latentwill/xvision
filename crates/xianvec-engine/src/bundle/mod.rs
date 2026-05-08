pub mod manifest;
pub mod risk;
pub mod slot;

use serde::{Deserialize, Serialize};

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

    pub risk: RiskConfig,

    /// Template-specific mechanical params (e.g., rsi thresholds, EMA periods).
    pub mechanical_params: serde_json::Value,
}

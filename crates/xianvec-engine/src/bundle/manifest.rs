use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublicManifest {
    pub id: String,            // ULID
    pub display_name: String,  // L1 plain English ("Buys dips")
    pub plain_summary: String, // L1 description
    pub creator: String,       // @handle or 8004 wallet
    pub template: String,      // template name
    pub regime_fit: Vec<RegimeFit>,
    pub asset_universe: Vec<String>, // e.g., ["ETH/USD", "BTC/USD"]
    pub decision_cadence_minutes: u32,
    pub required_models: Vec<String>,
    pub required_tools: Vec<String>,
    pub risk_preset_or_config: String, // "conservative" | "balanced" | "aggressive" | "custom"
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegimeFit {
    TrendingBull,
    TrendingBear,
    RangeBound,
    Chop,
    HighVol,
    LowVol,
    EventDriven,
}

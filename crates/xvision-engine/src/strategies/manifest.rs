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
    /// Informational attestation: the model(s) this strategy was last
    /// published / tested with. Surfaced in the UI but never gates which
    /// model the operator binds at eval-launch — the binding choice is
    /// owned by the operator, not the strategy author.
    pub attested_with: Vec<String>,
    pub required_tools: Vec<String>,
    pub risk_preset_or_config: String, // "conservative" | "balanced" | "aggressive" | "custom"
    pub published_at: Option<DateTime<Utc>>,
    /// Minimum context bars this strategy needs before bar 1 of the
    /// decision window. `None` means "derive from `mechanical_params`"
    /// (see [`super::Strategy::min_warmup_bars`]). Set explicitly when the
    /// derivation is wrong or when the strategy relies on indicators not
    /// reflected in `mechanical_params`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_warmup_bars: Option<u32>,
    /// Optional per-strategy display color (hex, e.g. `"#D4A547"`).
    /// Used by the Charts dashboard section (chart-rework spec
    /// Track B). When `None`, render layers fall back to the
    /// `strategyRotation` palette by stable index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// How the harness drives the asset universe. Defaults to `PerAsset`
    /// so pre-multi-asset strategy JSON parses unchanged.
    #[serde(default)]
    pub execution_mode: crate::strategies::ExecutionMode,
    /// How capital is shared across assets. Defaults to `Pooled`.
    #[serde(default)]
    pub capital_mode: crate::strategies::CapitalMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_without_modes_defaults_per_asset_pooled() {
        // Legacy strategy JSON omits the new fields → defaults.
        let json = serde_json::json!({
            "id":"s1","display_name":"d","plain_summary":"",
            "creator":"@x","template":"custom","regime_fit":[],
            "asset_universe":["BTC/USD"],"decision_cadence_minutes":60,
            "attested_with":[],"required_tools":[],
            "risk_preset_or_config":"balanced"
        });
        let m: PublicManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.execution_mode, crate::strategies::ExecutionMode::PerAsset);
        assert_eq!(m.capital_mode, crate::strategies::CapitalMode::Pooled);
    }
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

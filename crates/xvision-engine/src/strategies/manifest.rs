use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct TimeframeSpec(pub String);

impl TimeframeSpec {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TimeframeRequirements {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auxiliary: Vec<TimeframeSpec>,
}

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
    /// Optional auxiliary timeframes the strategy may access in addition to its
    /// native decision cadence. Missing/empty preserves legacy native-only behavior.
    #[serde(default, skip_serializing_if = "timeframe_requirements_is_default")]
    pub timeframe_requirements: TimeframeRequirements,
    /// Informational attestation: the model(s) this strategy was last
    /// published / tested with. Surfaced in the UI but never gates which
    /// model the operator binds at eval-launch — the binding choice is
    /// owned by the operator, not the strategy author.
    ///
    /// `#[serde(default)]` so strategy manifests written before this field
    /// existed still deserialize (missing → empty Vec) rather than failing the
    /// search reindex with `missing field attested_with`. Serialization is
    /// unchanged (no `skip_serializing_if`): the field is always written out.
    #[serde(default)]
    pub attested_with: Vec<String>,
    pub required_tools: Vec<String>,
    pub risk_preset_or_config: String, // "conservative" | "balanced" | "aggressive" | "custom"
    pub published_at: Option<DateTime<Utc>>,
    /// Minimum context bars this strategy needs before bar 1 of the
    /// decision window. `None` falls back to
    /// [`super::FALLBACK_MIN_WARMUP_BARS`] (see
    /// [`super::Strategy::min_warmup_bars`]). Set explicitly when the
    /// strategy relies on indicators that need prior-bar history.
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

fn timeframe_requirements_is_default(req: &TimeframeRequirements) -> bool {
    req.auxiliary.is_empty()
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
        assert!(m.timeframe_requirements.auxiliary.is_empty());
    }

    #[test]
    fn manifest_with_auxiliary_timeframes_roundtrips() {
        let json = serde_json::json!({
            "id":"s2","display_name":"d","plain_summary":"",
            "creator":"@x","template":"custom","regime_fit":[],
            "asset_universe":["BTC/USD"],"decision_cadence_minutes":60,
            "timeframe_requirements":{"auxiliary":["4h","1d"]},
            "attested_with":[],"required_tools":[],
            "risk_preset_or_config":"balanced"
        });
        let m: PublicManifest = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(
            m.timeframe_requirements.auxiliary,
            vec![TimeframeSpec("4h".into()), TimeframeSpec("1d".into())]
        );
        let out = serde_json::to_value(m).unwrap();
        assert_eq!(out["timeframe_requirements"], json["timeframe_requirements"]);
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

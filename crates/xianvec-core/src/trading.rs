//! Trading-domain types — InternBriefing, TraderDecision, RiskDecision.
//!
//! All structs validate via `garde::Validate`. Parsing JSON only checks shape;
//! `decision.validate(&())` runs range/length checks at the boundary.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use garde::Validate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Buy,
    Sell,
    Flat,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Long,
    Short,
    Flat,
}

/// Whitelisted tradeable assets. v1 ships BTC only — `Eth` and `Sol` declared
/// for the BTreeMap keying surface but not enabled in `whitelist.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AssetSymbol {
    Btc,
    Eth,
    Sol,
}

impl AssetSymbol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Btc => "BTC",
            Self::Eth => "ETH",
            Self::Sol => "SOL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Regime {
    Bull,
    Bear,
    Chop,
    HighVol,
    LowVol,
}

/// Tag attached to a piece of evidence in an InternBriefing's case lists.
/// Coarse — fine-grained schemas live in higher crates.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTag {
    Technical(String),
    Onchain(String),
    Macro(String),
    Sentiment(String),
    Fundamental(String),
}

/// One axis of disposition along which a steering vector can be installed.
/// v1 active axis: Conviction. The other three are extracted for pipeline
/// validation but not active in the headline experiment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispositionAxis {
    Conviction,
    Patience,
    RiskAppetite,
    TrendDisposition,
}

/// Stage 1 output: balanced bull/bear/flat case for one setup. The Intern is
/// forbidden from naming a recommendation (§2 architecture) — that keeps
/// vectors' steering surface clean for Stage 2.
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct InternBriefing {
    #[garde(skip)]
    pub setup_id: Uuid,
    #[garde(skip)]
    pub asset: AssetSymbol,

    #[garde(length(min = 20, max = 2000))]
    pub bull_case: String,
    #[garde(length(min = 20, max = 2000))]
    pub bear_case: String,
    #[garde(length(min = 20, max = 2000))]
    pub flat_case: String,

    #[garde(skip)]
    pub evidence_long: Vec<EvidenceTag>,
    #[garde(skip)]
    pub evidence_short: Vec<EvidenceTag>,
    #[garde(skip)]
    pub evidence_flat: Vec<EvidenceTag>,

    #[garde(skip)]
    pub regime: Regime,

    #[garde(range(min = 0.0, max = 1.0))]
    pub signal_quality: f32,

    #[garde(range(min = 1, max = 168))]
    pub horizon_hours: u32,

    #[garde(skip)]
    pub created_at: DateTime<Utc>,
}

/// Stage 2 output: a concrete trade decision. Vectors-on and vectors-off arms
/// each emit one of these against the same cached briefing (Tier 1 fix #1).
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct TraderDecision {
    #[garde(skip)]
    pub setup_id: Uuid,
    #[garde(skip)]
    pub action: Action,
    /// Position size in basis points of NAV (max 20% = 2000bps).
    #[garde(range(min = 0, max = 2000))]
    pub size_bps: u32,
    #[garde(skip)]
    pub direction: Direction,
    #[garde(range(min = 0.1, max = 20.0))]
    pub stop_loss_pct: f32,
    #[garde(range(min = 0.1, max = 50.0))]
    pub take_profit_pct: f32,
    #[garde(length(min = 10, max = 500))]
    pub trader_summary: String,
    /// Per-axis active magnitude. Empty = vectors-off arm. v1 uses `BTreeMap`
    /// for stable ordering in `decisions` SQL keying.
    #[garde(skip)]
    pub active_vectors: BTreeMap<DispositionAxis, f32>,
}

impl TraderDecision {
    /// Tuple keyed for divergence analysis (Tier 3 cleanup): the headline
    /// divergence rate operates on `(action, direction, size_bucket)` rather
    /// than `action` alone.
    pub fn divergence_key(&self) -> (Action, Direction, SizeBucket) {
        (self.action, self.direction, SizeBucket::from_bps(self.size_bps))
    }
}

/// Coarse size bucketing for divergence analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SizeBucket {
    Zero,
    Small,  // 1–500 bps
    Medium, // 501–1500 bps
    Large,  // 1501–2000 bps
}

impl SizeBucket {
    pub fn from_bps(bps: u32) -> Self {
        match bps {
            0 => Self::Zero,
            1..=500 => Self::Small,
            501..=1500 => Self::Medium,
            _ => Self::Large,
        }
    }
}

/// Reason a risk rule modified or vetoed a decision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VetoReason {
    PositionTooLarge,
    ExposureCap,
    AssetNotWhitelisted,
    DailyLossCircuitBreaker,
    MaxOpenPositions,
    CorrelationClusterCap,
    StopLossMissing,
    StopLossTooWide,
    TakeProfitMissing,
    Custom(String),
}

/// Risk-layer output: a `TraderDecision` is approved as-is, modified into a
/// reduced version, or fully vetoed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum RiskDecision {
    Approved {
        decision: TraderDecision,
    },
    Modified {
        original: TraderDecision,
        modified: TraderDecision,
        reason: VetoReason,
    },
    Vetoed {
        original: TraderDecision,
        reason: VetoReason,
    },
}

impl RiskDecision {
    /// The decision the executor should act on (None for veto).
    pub fn effective(&self) -> Option<&TraderDecision> {
        match self {
            Self::Approved { decision } | Self::Modified { modified: decision, .. } => Some(decision),
            Self::Vetoed { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixture_briefing() -> InternBriefing {
        InternBriefing {
            setup_id: Uuid::nil(),
            asset: AssetSymbol::Btc,
            bull_case: "Funding rate compressed; smart money accumulating spot.".into(),
            bear_case: "Realized vol expanding; long-leverage approaching prior squeeze level.".into(),
            flat_case: "Range-bound between SMA20 and SMA50; await directional break.".into(),
            evidence_long: vec![EvidenceTag::Onchain("smart_money_inflow".into())],
            evidence_short: vec![EvidenceTag::Technical("rsi_overbought".into())],
            evidence_flat: vec![EvidenceTag::Technical("range_bound".into())],
            regime: Regime::Chop,
            signal_quality: 0.6,
            horizon_hours: 24,
            created_at: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    fn fixture_decision() -> TraderDecision {
        TraderDecision {
            setup_id: Uuid::nil(),
            action: Action::Buy,
            size_bps: 1000,
            direction: Direction::Long,
            stop_loss_pct: 2.5,
            take_profit_pct: 5.0,
            trader_summary: "Long entry on confirmed range break with 2:1 R:R.".into(),
            active_vectors: BTreeMap::from([(DispositionAxis::Conviction, 1.0)]),
        }
    }

    #[test]
    fn briefing_validates() {
        fixture_briefing().validate().expect("fixture must pass");
    }

    #[test]
    fn briefing_rejects_short_bull_case() {
        let mut b = fixture_briefing();
        b.bull_case = "tiny".into();
        let err = b.validate().expect_err("short string should fail garde length(min=20)");
        assert!(format!("{err}").contains("bull_case"));
    }

    #[test]
    fn briefing_rejects_signal_quality_out_of_range() {
        let mut b = fixture_briefing();
        b.signal_quality = 1.5;
        b.validate().expect_err("signal_quality > 1.0 must fail");
    }

    #[test]
    fn decision_validates() {
        fixture_decision().validate().expect("fixture must pass");
    }

    #[test]
    fn decision_rejects_oversize_position() {
        let mut d = fixture_decision();
        d.size_bps = 2500;
        d.validate().expect_err("size_bps > 2000 must fail");
    }

    #[test]
    fn decision_rejects_zero_stop_loss() {
        let mut d = fixture_decision();
        d.stop_loss_pct = 0.0;
        d.validate().expect_err("stop_loss_pct < 0.1 must fail (Tier-3 risk hygiene)");
    }

    #[test]
    fn decision_rejects_short_summary() {
        let mut d = fixture_decision();
        d.trader_summary = "ok".into();
        d.validate().expect_err("trader_summary < 10 chars must fail");
    }

    #[test]
    fn briefing_round_trips_json() {
        let b = fixture_briefing();
        let s = serde_json::to_string(&b).unwrap();
        let back: InternBriefing = serde_json::from_str(&s).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn decision_round_trips_json() {
        let d = fixture_decision();
        let s = serde_json::to_string(&d).unwrap();
        let back: TraderDecision = serde_json::from_str(&s).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn risk_decision_round_trips_json_for_each_variant() {
        let d = fixture_decision();
        let cases = vec![
            RiskDecision::Approved { decision: d.clone() },
            RiskDecision::Modified {
                original: d.clone(),
                modified: TraderDecision { size_bps: 500, ..d.clone() },
                reason: VetoReason::PositionTooLarge,
            },
            RiskDecision::Vetoed { original: d.clone(), reason: VetoReason::DailyLossCircuitBreaker },
        ];
        for r in cases {
            let s = serde_json::to_string(&r).unwrap();
            let back: RiskDecision = serde_json::from_str(&s).unwrap();
            assert_eq!(r, back);
        }
    }

    #[test]
    fn risk_decision_effective_skips_veto() {
        let d = fixture_decision();
        assert!(RiskDecision::Approved { decision: d.clone() }.effective().is_some());
        assert!(RiskDecision::Vetoed {
            original: d,
            reason: VetoReason::AssetNotWhitelisted
        }
        .effective()
        .is_none());
    }

    #[test]
    fn divergence_key_groups_size_into_buckets() {
        let mut d = fixture_decision();
        d.size_bps = 0;
        assert_eq!(d.divergence_key().2, SizeBucket::Zero);
        d.size_bps = 250;
        assert_eq!(d.divergence_key().2, SizeBucket::Small);
        d.size_bps = 1000;
        assert_eq!(d.divergence_key().2, SizeBucket::Medium);
        d.size_bps = 1900;
        assert_eq!(d.divergence_key().2, SizeBucket::Large);
    }

    proptest::proptest! {
        #[test]
        fn size_bucket_total(bps in 0u32..=2000) {
            let _ = SizeBucket::from_bps(bps); // never panics
        }

        #[test]
        fn decision_size_bps_validation_is_total(bps in 0u32..=5000) {
            let mut d = fixture_decision();
            d.size_bps = bps;
            let result = d.validate();
            if bps <= 2000 {
                proptest::prop_assert!(result.is_ok());
            } else {
                proptest::prop_assert!(result.is_err());
            }
        }
    }
}

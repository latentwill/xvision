//! Data the Stage-1 briefing agent sees about a market cycle. Pure data: the
//! briefing agent reads, the briefing agent's prompt builder formats, the
//! briefing agent's backend sends to the LLM. No computation lives here —
//! indicators are populated by `xvision-data` upstream.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::trading::{AssetSymbol, Regime};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ohlcv {
    pub timestamp: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Snapshot of an asset at a point in time. The agent receives this
/// (formatted) as its market context to reason over.
///
/// Field semantics:
/// - `recent_bars` is ordered chronologically (oldest first); the last entry
///   is the most recent bar and `price` should match `recent_bars.last().close`.
/// - All optional indicator/onchain fields use `None` for "not computed" or
///   "not available" — the prompt builder skips them rather than printing nulls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub cycle_id: Uuid,
    pub asset: AssetSymbol,
    pub timestamp: DateTime<Utc>,
    pub price: f64,
    pub volume_24h: Option<f64>,
    pub recent_bars: Vec<Ohlcv>,
    pub indicators: IndicatorPanel,
    pub onchain: OnchainPanel,
    pub regime: Regime,
    /// Forward-looking horizon (hours) the agent should evaluate against.
    pub horizon_hours: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IndicatorPanel {
    pub rsi_14: Option<f64>,
    pub sma_20: Option<f64>,
    pub sma_50: Option<f64>,
    pub sma_200: Option<f64>,
    pub ema_12: Option<f64>,
    pub ema_26: Option<f64>,
    pub bb_upper: Option<f64>,
    pub bb_middle: Option<f64>,
    pub bb_lower: Option<f64>,
    pub atr_14: Option<f64>,
    pub macd: Option<f64>,
    pub macd_signal: Option<f64>,
    pub macd_hist: Option<f64>,
    pub donchian_upper: Option<f64>,
    pub donchian_lower: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OnchainPanel {
    /// Annualized perp funding rate (e.g. 0.012 = 1.2%).
    pub funding_rate_8h: Option<f64>,
    pub open_interest_usd: Option<f64>,
    pub long_short_ratio: Option<f64>,
    pub stablecoin_inflows_24h_usd: Option<f64>,
    pub liquidations_24h_usd: Option<f64>,
    pub realized_volatility_30d: Option<f64>,
}

/// Reference to a skill catalog entry. v1 carries name + a one-line summary;
/// the prompt builder includes the summary list so the briefing agent knows the
/// domain context it has access to. Full skill bodies live on disk under
/// `.claude/skills/{byreal,mantle}/skills/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillRef {
    pub catalog: String, // "byreal" | "mantle"
    pub name: String,
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixture_snapshot() -> MarketSnapshot {
        MarketSnapshot {
            cycle_id: Uuid::nil(),
            asset: AssetSymbol::Btc,
            timestamp: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
            price: 70_000.0,
            volume_24h: Some(28_000_000_000.0),
            recent_bars: vec![Ohlcv {
                timestamp: Utc.timestamp_opt(1_699_996_400, 0).single().unwrap(),
                open: 69_500.0,
                high: 70_200.0,
                low: 69_400.0,
                close: 70_000.0,
                volume: 1_000_000_000.0,
            }],
            indicators: IndicatorPanel {
                rsi_14: Some(52.3),
                sma_20: Some(69_500.0),
                ..Default::default()
            },
            onchain: OnchainPanel {
                funding_rate_8h: Some(0.00012),
                ..Default::default()
            },
            regime: Regime::Chop,
            horizon_hours: 24,
        }
    }

    #[test]
    fn snapshot_round_trips_json() {
        let s = fixture_snapshot();
        let j = serde_json::to_string(&s).unwrap();
        let back: MarketSnapshot = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn skill_ref_round_trips() {
        let r = SkillRef {
            catalog: "byreal".into(),
            name: "perp-risk-shapes".into(),
            summary: "Perp futures risk shapes — drawdown, funding skew, liquidation cascades.".into(),
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: SkillRef = serde_json::from_str(&j).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn defaults_initialize_empty_panels() {
        let p = IndicatorPanel::default();
        assert!(p.rsi_14.is_none());
        assert!(p.macd.is_none());
        let o = OnchainPanel::default();
        assert!(o.funding_rate_8h.is_none());
    }
}

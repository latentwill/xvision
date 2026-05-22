//! RsiMeanReversion baseline — RSI-14 oversold/overbought signals.
//!
//! - `rsi_14 <= oversold (30)` → Buy Long 800 bps, stop 2.5%, tp 5%
//! - `rsi_14 >= overbought (70)` → Sell Short 800 bps, stop 2.5%, tp 5%
//! - Otherwise or when RSI is None → `None`

use async_trait::async_trait;
use xvision_core::market::MarketSnapshot;
use xvision_core::trading::{Action, Direction, TraderDecision};

use crate::algorithm::Algorithm;

pub struct RsiMeanReversion {
    pub period: u32,
    pub oversold: f64,
    pub overbought: f64,
}

impl RsiMeanReversion {
    pub fn new() -> Self {
        Self {
            period: 14,
            oversold: 30.0,
            overbought: 70.0,
        }
    }
}

impl Default for RsiMeanReversion {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Algorithm for RsiMeanReversion {
    fn name(&self) -> &'static str {
        "rsi_mean_reversion"
    }

    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
        let rsi = snapshot.indicators.rsi_14?;

        let (action, direction, summary) = if rsi <= self.oversold {
            (
                Action::Buy,
                Direction::Long,
                "RsiMeanReversion: RSI oversold — long entry at 800 bps.",
            )
        } else if rsi >= self.overbought {
            (
                Action::Sell,
                Direction::Short,
                "RsiMeanReversion: RSI overbought — short entry at 800 bps.",
            )
        } else {
            return None;
        };

        Some(TraderDecision {
            cycle_id: snapshot.cycle_id,
            action,
            size_bps: 800,
            direction,
            stop_loss_pct: 2.5,
            take_profit_pct: 5.0,
            trader_summary: summary.into(),
            asset: snapshot.asset,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;
    use xvision_core::market::{IndicatorPanel, Ohlcv, OnchainPanel};
    use xvision_core::trading::{AssetSymbol, Regime};

    fn fixture_snapshot_with_rsi(rsi: Option<f64>) -> MarketSnapshot {
        MarketSnapshot {
            cycle_id: Uuid::new_v4(),
            asset: AssetSymbol::Btc,
            timestamp: Utc::now(),
            price: 70_000.0,
            volume_24h: None,
            recent_bars: vec![Ohlcv {
                timestamp: Utc::now(),
                open: 69_500.0,
                high: 70_200.0,
                low: 69_400.0,
                close: 70_000.0,
                volume: 1_000_000_000.0,
            }],
            indicators: IndicatorPanel {
                rsi_14: rsi,
                ..Default::default()
            },
            onchain: OnchainPanel::default(),
            regime: Regime::Chop,
            horizon_hours: 24,
        }
    }

    #[tokio::test]
    async fn decide_returns_expected_shape_oversold() {
        let snap = fixture_snapshot_with_rsi(Some(25.0));
        let strat = RsiMeanReversion::new();
        let dec = strat.decide(&snap).await.expect("oversold must return Some");
        assert_eq!(dec.cycle_id, snap.cycle_id, "cycle_id must propagate");
        assert_eq!(dec.action, Action::Buy);
        assert_eq!(dec.direction, Direction::Long);
        assert_eq!(dec.size_bps, 800);
        assert!((dec.stop_loss_pct - 2.5).abs() < f32::EPSILON);
        assert!((dec.take_profit_pct - 5.0).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn decide_returns_expected_shape_overbought() {
        let snap = fixture_snapshot_with_rsi(Some(75.0));
        let strat = RsiMeanReversion::new();
        let dec = strat.decide(&snap).await.expect("overbought must return Some");
        assert_eq!(dec.action, Action::Sell);
        assert_eq!(dec.direction, Direction::Short);
        assert_eq!(dec.size_bps, 800);
    }

    #[tokio::test]
    async fn edge_case_rsi_none_returns_none() {
        let snap = fixture_snapshot_with_rsi(None);
        let strat = RsiMeanReversion::new();
        assert!(
            strat.decide(&snap).await.is_none(),
            "missing RSI must return None"
        );
    }

    #[tokio::test]
    async fn edge_case_neutral_rsi_returns_none() {
        let snap = fixture_snapshot_with_rsi(Some(50.0));
        let strat = RsiMeanReversion::new();
        assert!(
            strat.decide(&snap).await.is_none(),
            "neutral RSI must return None"
        );
    }
}

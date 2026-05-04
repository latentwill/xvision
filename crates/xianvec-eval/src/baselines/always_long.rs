//! AlwaysLong baseline — emits Buy Long 500 bps on every snapshot.
//! Useful as a constant-bull null hypothesis.

use std::collections::BTreeMap;

use async_trait::async_trait;
use xianvec_core::market::MarketSnapshot;
use xianvec_core::trading::{Action, Direction, TraderDecision};

use crate::strategy::Strategy;

pub struct AlwaysLong;

#[async_trait]
impl Strategy for AlwaysLong {
    fn name(&self) -> &'static str {
        "always_long"
    }

    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
        Some(TraderDecision {
            setup_id: snapshot.setup_id,
            action: Action::Buy,
            size_bps: 500,
            direction: Direction::Long,
            stop_loss_pct: 2.0,
            take_profit_pct: 4.0,
            trader_summary: "AlwaysLong: unconditional long entry at 500 bps.".into(),
            active_vectors: BTreeMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;
    use xianvec_core::market::{IndicatorPanel, OnchainPanel, Ohlcv};
    use xianvec_core::trading::{AssetSymbol, Regime};

    fn fixture_snapshot() -> MarketSnapshot {
        MarketSnapshot {
            setup_id: Uuid::new_v4(),
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
            indicators: IndicatorPanel::default(),
            onchain: OnchainPanel::default(),
            regime: Regime::Bull,
            horizon_hours: 24,
        }
    }

    #[tokio::test]
    async fn decide_returns_expected_shape() {
        let snap = fixture_snapshot();
        let strat = AlwaysLong;
        let dec = strat.decide(&snap).await.expect("must always return Some");
        assert_eq!(dec.setup_id, snap.setup_id, "setup_id must propagate");
        assert_eq!(dec.action, Action::Buy);
        assert_eq!(dec.direction, Direction::Long);
        assert_eq!(dec.size_bps, 500);
        assert!((dec.stop_loss_pct - 2.0).abs() < f32::EPSILON);
        assert!((dec.take_profit_pct - 4.0).abs() < f32::EPSILON);
        assert!(dec.active_vectors.is_empty());
    }

    #[tokio::test]
    async fn edge_case_always_returns_some() {
        let strat = AlwaysLong;
        for _ in 0..5 {
            let snap = fixture_snapshot();
            assert!(
                strat.decide(&snap).await.is_some(),
                "AlwaysLong must emit on every snapshot"
            );
        }
    }
}

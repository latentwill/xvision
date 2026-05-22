//! BuyAndHold baseline — fires one Buy Long on the first snapshot, then
//! returns `None` for all subsequent calls. Acts as the static long benchmark.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use xvision_core::market::MarketSnapshot;
use xvision_core::trading::{Action, Direction, TraderDecision};

use crate::algorithm::Algorithm;

pub struct BuyAndHold {
    entered: AtomicBool,
}

impl BuyAndHold {
    pub fn new() -> Self {
        Self {
            entered: AtomicBool::new(false),
        }
    }
}

impl Default for BuyAndHold {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Algorithm for BuyAndHold {
    fn name(&self) -> &'static str {
        "buy_and_hold"
    }

    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
        // Only enter once — use compare_exchange to avoid TOCTOU on concurrent
        // harness calls (though the harness is single-threaded per arm).
        if self
            .entered
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Some(TraderDecision {
                cycle_id: snapshot.cycle_id,
                action: Action::Buy,
                size_bps: 1500,
                direction: Direction::Long,
                stop_loss_pct: 5.0,
                take_profit_pct: 10.0,
                trader_summary: "BuyAndHold: static long entry — buy once, hold forever.".into(),
                asset: snapshot.asset,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;
    use xvision_core::market::{IndicatorPanel, Ohlcv, OnchainPanel};
    use xvision_core::trading::{AssetSymbol, Regime};

    fn fixture_snapshot() -> MarketSnapshot {
        let cycle_id = Uuid::new_v4();
        MarketSnapshot {
            cycle_id,
            asset: AssetSymbol::Btc,
            timestamp: Utc::now(),
            price: 70_000.0,
            volume_24h: Some(28_000_000_000.0),
            recent_bars: vec![Ohlcv {
                timestamp: Utc::now(),
                open: 69_500.0,
                high: 70_200.0,
                low: 69_400.0,
                close: 70_000.0,
                volume: 1_000_000_000.0,
            }],
            indicators: IndicatorPanel {
                rsi_14: Some(52.0),
                ..Default::default()
            },
            onchain: OnchainPanel::default(),
            regime: Regime::Bull,
            horizon_hours: 24,
        }
    }

    #[tokio::test]
    async fn decide_returns_expected_shape() {
        let snap = fixture_snapshot();
        let strat = BuyAndHold::new();
        let dec = strat.decide(&snap).await.expect("first call must return Some");
        assert_eq!(dec.cycle_id, snap.cycle_id, "cycle_id must propagate");
        assert_eq!(dec.action, Action::Buy);
        assert_eq!(dec.direction, Direction::Long);
        assert_eq!(dec.size_bps, 1500);
        assert!((dec.stop_loss_pct - 5.0).abs() < f32::EPSILON);
        assert!((dec.take_profit_pct - 10.0).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn edge_case_only_fires_once() {
        let strat = BuyAndHold::new();
        let snap = fixture_snapshot();
        assert!(strat.decide(&snap).await.is_some(), "first call must be Some");
        assert!(strat.decide(&snap).await.is_none(), "second call must be None");
        assert!(strat.decide(&snap).await.is_none(), "third call must be None");
    }
}

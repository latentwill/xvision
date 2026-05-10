//! MacdMomentum baseline — zero-line crossover on the MACD histogram.
//!
//! Signal:
//! - `macd_hist > 0 && macd > macd_signal && prev_hist <= 0` → Buy Long 800 bps
//! - `macd_hist < 0 && macd < macd_signal && prev_hist >= 0` → Sell Short 800 bps
//! - Otherwise or when any indicator is None → None
//!
//! Interior mutability: **`Mutex<Option<f64>>`** for previous bar's `macd_hist`.
//! Same rationale as MaCrossover: uncontended in harness, clear semantics.

use std::sync::Mutex;

use async_trait::async_trait;
use xianvec_core::market::MarketSnapshot;
use xianvec_core::trading::{Action, Direction, TraderDecision};

use crate::strategy::Strategy;

pub struct MacdMomentum {
    prev_hist: Mutex<Option<f64>>,
}

impl MacdMomentum {
    pub fn new() -> Self {
        Self {
            prev_hist: Mutex::new(None),
        }
    }
}

impl Default for MacdMomentum {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for MacdMomentum {
    fn name(&self) -> &'static str {
        "macd_momentum"
    }

    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
        let macd = snapshot.indicators.macd?;
        let macd_signal = snapshot.indicators.macd_signal?;
        let macd_hist = snapshot.indicators.macd_hist?;

        let mut prev_guard = self.prev_hist.lock().expect("MacdMomentum prev_hist poisoned");
        let prev_hist = *prev_guard;

        // Update state before returning — always record this bar's hist.
        *prev_guard = Some(macd_hist);

        let prev = prev_hist?; // warmup: no previous bar to compare

        let bullish_cross = macd_hist > 0.0 && macd > macd_signal && prev <= 0.0;
        let bearish_cross = macd_hist < 0.0 && macd < macd_signal && prev >= 0.0;

        if bullish_cross {
            Some(TraderDecision {
                cycle_id: snapshot.cycle_id,
                action: Action::Buy,
                size_bps: 800,
                direction: Direction::Long,
                stop_loss_pct: 2.0,
                take_profit_pct: 4.0,
                trader_summary: "MacdMomentum: MACD hist crossed above zero — long momentum.".into(),
            })
        } else if bearish_cross {
            Some(TraderDecision {
                cycle_id: snapshot.cycle_id,
                action: Action::Sell,
                size_bps: 800,
                direction: Direction::Short,
                stop_loss_pct: 2.0,
                take_profit_pct: 4.0,
                trader_summary: "MacdMomentum: MACD hist crossed below zero — short momentum.".into(),
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
    use xianvec_core::market::{IndicatorPanel, OnchainPanel, Ohlcv};
    use xianvec_core::trading::{AssetSymbol, Regime};

    fn fixture_snapshot_with_macd(
        macd: Option<f64>,
        macd_signal: Option<f64>,
        macd_hist: Option<f64>,
    ) -> MarketSnapshot {
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
                macd,
                macd_signal,
                macd_hist,
                ..Default::default()
            },
            onchain: OnchainPanel::default(),
            regime: Regime::Bull,
            horizon_hours: 24,
        }
    }

    #[tokio::test]
    async fn decide_returns_expected_shape_bullish_cross() {
        let strat = MacdMomentum::new();

        // Warmup: hist was negative
        let neg_snap = fixture_snapshot_with_macd(Some(-0.5), Some(-0.2), Some(-0.1));
        assert!(strat.decide(&neg_snap).await.is_none(), "warmup returns None");

        // Bullish cross: hist just turned positive, macd > signal
        let bull_snap = fixture_snapshot_with_macd(Some(1.0), Some(0.5), Some(0.3));
        let dec = strat
            .decide(&bull_snap)
            .await
            .expect("bullish cross must return Some");
        assert_eq!(dec.cycle_id, bull_snap.cycle_id, "cycle_id must propagate");
        assert_eq!(dec.action, Action::Buy);
        assert_eq!(dec.direction, Direction::Long);
        assert_eq!(dec.size_bps, 800);
    }

    #[tokio::test]
    async fn decide_returns_expected_shape_bearish_cross() {
        let strat = MacdMomentum::new();

        // Warmup: hist was positive
        let pos_snap = fixture_snapshot_with_macd(Some(0.5), Some(0.2), Some(0.1));
        assert!(strat.decide(&pos_snap).await.is_none(), "warmup returns None");

        // Bearish cross: hist just turned negative, macd < signal
        let bear_snap = fixture_snapshot_with_macd(Some(-1.0), Some(-0.5), Some(-0.3));
        let dec = strat
            .decide(&bear_snap)
            .await
            .expect("bearish cross must return Some");
        assert_eq!(dec.action, Action::Sell);
        assert_eq!(dec.direction, Direction::Short);
        assert_eq!(dec.size_bps, 800);
    }

    #[tokio::test]
    async fn edge_case_macd_none_returns_none() {
        let strat = MacdMomentum::new();
        let snap = fixture_snapshot_with_macd(None, None, None);
        assert!(
            strat.decide(&snap).await.is_none(),
            "missing MACD indicators must return None"
        );
    }

    #[tokio::test]
    async fn edge_case_warmup_first_bar_returns_none() {
        let strat = MacdMomentum::new();
        // Even with valid data, first bar is warmup
        let snap = fixture_snapshot_with_macd(Some(1.0), Some(0.5), Some(0.3));
        assert!(
            strat.decide(&snap).await.is_none(),
            "first bar must be warmup None"
        );
    }
}

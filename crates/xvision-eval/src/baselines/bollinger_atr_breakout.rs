//! BollingerATRBreakout baseline — Bollinger Band breakout confirmed by ATR.
//!
//! Signal:
//! - `close > bb_upper && atr_14 / close > min_atr_pct` → Buy Long
//! - `close < bb_lower && atr_14 / close > min_atr_pct` → Sell Short
//! - Otherwise or when any indicator is None → None
//!
//! Position sizing and risk levels are ATR-derived so risk stays proportional
//! to current market noise.

use async_trait::async_trait;
use xvision_core::market::MarketSnapshot;
use xvision_core::trading::{Action, Direction, TraderDecision};

use crate::algorithm::Algorithm;

pub struct BollingerATRBreakout {
    pub min_atr_pct: f64,
    pub atr_mult_sl: f64,
    pub atr_mult_tp: f64,
    pub size_bps: u32,
}

impl BollingerATRBreakout {
    pub fn new() -> Self {
        Self {
            min_atr_pct: 0.008,
            atr_mult_sl: 1.5,
            atr_mult_tp: 3.0,
            size_bps: 600,
        }
    }
}

impl Default for BollingerATRBreakout {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Algorithm for BollingerATRBreakout {
    fn name(&self) -> &'static str {
        "bollinger_atr_breakout"
    }

    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
        let close = snapshot.price;
        let bb_upper = snapshot.indicators.bb_upper?;
        let bb_lower = snapshot.indicators.bb_lower?;
        let atr = snapshot.indicators.atr_14?;

        let atr_pct = atr / close;
        if atr_pct < self.min_atr_pct {
            return None;
        }

        let stop = (atr * self.atr_mult_sl) as f32;
        let target = (atr * self.atr_mult_tp) as f32;

        if close > bb_upper {
            Some(TraderDecision {
                cycle_id: snapshot.cycle_id,
                action: Action::Buy,
                size_bps: self.size_bps,
                direction: Direction::Long,
                stop_loss_pct: stop,
                take_profit_pct: target,
                trader_summary: format!(
                    "BollingerATRBreakout: close {:.2} broke above upper band {:.2} with ATR {:.2} ({:.2}%) — long.",
                    close, bb_upper, atr, atr_pct * 100.0
                ),
                asset: None,
            })
        } else if close < bb_lower {
            Some(TraderDecision {
                cycle_id: snapshot.cycle_id,
                action: Action::Sell,
                size_bps: self.size_bps,
                direction: Direction::Short,
                stop_loss_pct: stop,
                take_profit_pct: target,
                trader_summary: format!(
                    "BollingerATRBreakout: close {:.2} broke below lower band {:.2} with ATR {:.2} ({:.2}%) — short.",
                    close, bb_lower, atr, atr_pct * 100.0
                ),
                asset: None,
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

    fn fixture_snapshot(
        price: f64,
        bb_upper: Option<f64>,
        bb_lower: Option<f64>,
        atr_14: Option<f64>,
    ) -> MarketSnapshot {
        MarketSnapshot {
            cycle_id: Uuid::new_v4(),
            asset: AssetSymbol::Btc,
            timestamp: Utc::now(),
            price,
            volume_24h: None,
            recent_bars: vec![Ohlcv {
                timestamp: Utc::now(),
                open: price * 0.99,
                high: price * 1.01,
                low: price * 0.98,
                close: price,
                volume: 1_000_000_000.0,
            }],
            indicators: IndicatorPanel {
                bb_upper,
                bb_lower,
                atr_14,
                ..Default::default()
            },
            onchain: OnchainPanel::default(),
            regime: Regime::Bull,
            horizon_hours: 24,
        }
    }

    #[tokio::test]
    async fn decide_returns_long_on_upper_breakout() {
        let strat = BollingerATRBreakout::new();
        // price 70_000, upper 69_500, atr 700 (1.0%) → above min_atr_pct 0.8%
        let snap = fixture_snapshot(70_000.0, Some(69_500.0), Some(68_000.0), Some(700.0));
        let dec = strat.decide(&snap).await.expect("upper breakout must return Some");
        assert_eq!(dec.action, Action::Buy);
        assert_eq!(dec.direction, Direction::Long);
        assert_eq!(dec.size_bps, 600);
        assert!((dec.stop_loss_pct - 1050.0).abs() < f32::EPSILON * 2.0);
        assert!((dec.take_profit_pct - 2100.0).abs() < f32::EPSILON * 2.0);
    }

    #[tokio::test]
    async fn decide_returns_short_on_lower_breakout() {
        let strat = BollingerATRBreakout::new();
        let snap = fixture_snapshot(67_000.0, Some(69_000.0), Some(67_500.0), Some(600.0));
        let dec = strat.decide(&snap).await.expect("lower breakout must return Some");
        assert_eq!(dec.action, Action::Sell);
        assert_eq!(dec.direction, Direction::Short);
    }

    #[tokio::test]
    async fn decide_returns_none_when_inside_bands() {
        let strat = BollingerATRBreakout::new();
        let snap = fixture_snapshot(68_500.0, Some(69_000.0), Some(68_000.0), Some(600.0));
        assert!(strat.decide(&snap).await.is_none(), "inside bands must return None");
    }

    #[tokio::test]
    async fn decide_returns_none_when_atr_too_low() {
        let strat = BollingerATRBreakout::new();
        // atr 300 / price 70_000 = 0.43% < 0.8%
        let snap = fixture_snapshot(70_000.0, Some(69_500.0), Some(68_000.0), Some(300.0));
        assert!(strat.decide(&snap).await.is_none(), "low atr must return None");
    }

    #[tokio::test]
    async fn edge_case_missing_indicators_returns_none() {
        let strat = BollingerATRBreakout::new();
        let snap = fixture_snapshot(70_000.0, None, None, None);
        assert!(strat.decide(&snap).await.is_none(), "missing indicators must return None");
    }
}

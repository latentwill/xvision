//! MaCrossover baseline — SMA(fast) × SMA(slow) crossover signal.
//!
//! `IndicatorPanel` only exposes sma_20/50/200; this baseline computes its
//! own rolling SMAs from `snapshot.recent_bars` on every call.
//!
//! Interior mutability: **`Mutex<Option<(f32, f32)>>`** storing the previous
//! bar's (sma_fast, sma_slow). Chosen over bit-packed AtomicU64 because:
//! the Mutex version is immediately legible and avoids unsafe f32→u32
//! transmute tricks; the lock is uncontended in the single-threaded harness.
//!
//! Crossover detection:
//! - prev_fast <= prev_slow AND curr_fast > curr_slow → Buy Long (golden cross)
//! - prev_fast >= prev_slow AND curr_fast < curr_slow → Sell Short (death cross)
//! - No prior bar (warmup) or same relative position → None

use std::sync::Mutex;

use async_trait::async_trait;
use xvision_core::market::MarketSnapshot;
use xvision_core::trading::{Action, Direction, TraderDecision};

use crate::algorithm::Algorithm;

pub struct MaCrossover {
    pub fast_window: usize,
    pub slow_window: usize,
    /// Previous bar's (sma_fast, sma_slow). None on first call.
    prev: Mutex<Option<(f64, f64)>>,
}

impl MaCrossover {
    pub fn new(fast_window: usize, slow_window: usize) -> Self {
        Self {
            fast_window,
            slow_window,
            prev: Mutex::new(None),
        }
    }
}

/// Compute a simple moving average over the last `window` bars' close prices.
/// Returns `None` if fewer than `window` bars are available.
fn sma(bars: &[xvision_core::market::Ohlcv], window: usize) -> Option<f64> {
    if bars.len() < window {
        return None;
    }
    let slice = &bars[bars.len() - window..];
    let sum: f64 = slice.iter().map(|b| b.close).sum();
    Some(sum / window as f64)
}

#[async_trait]
impl Algorithm for MaCrossover {
    fn name(&self) -> &'static str {
        "ma_crossover"
    }

    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
        let curr_fast = sma(&snapshot.recent_bars, self.fast_window)?;
        let curr_slow = sma(&snapshot.recent_bars, self.slow_window)?;

        let mut prev_guard = self.prev.lock().expect("MaCrossover prev state poisoned");
        let decision = match *prev_guard {
            None => {
                // Warmup: store current values, no signal yet.
                *prev_guard = Some((curr_fast, curr_slow));
                return None;
            }
            Some((prev_fast, prev_slow)) => {
                let golden_cross = prev_fast <= prev_slow && curr_fast > curr_slow;
                let death_cross = prev_fast >= prev_slow && curr_fast < curr_slow;

                if golden_cross {
                    Some(TraderDecision {
                        cycle_id: snapshot.cycle_id,
                        action: Action::Buy,
                        size_bps: 1000,
                        direction: Direction::Long,
                        stop_loss_pct: 2.0,
                        take_profit_pct: 4.0,
                        trader_summary: format!(
                            "MaCrossover: SMA{} crossed above SMA{} — golden cross long.",
                            self.fast_window, self.slow_window
                        ),
                        asset: snapshot.asset,
                    })
                } else if death_cross {
                    Some(TraderDecision {
                        cycle_id: snapshot.cycle_id,
                        action: Action::Sell,
                        size_bps: 1000,
                        direction: Direction::Short,
                        stop_loss_pct: 2.0,
                        take_profit_pct: 4.0,
                        trader_summary: format!(
                            "MaCrossover: SMA{} crossed below SMA{} — death cross short.",
                            self.fast_window, self.slow_window
                        ),
                        asset: snapshot.asset,
                    })
                } else {
                    None
                }
            }
        };

        *prev_guard = Some((curr_fast, curr_slow));
        decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;
    use xvision_core::market::{IndicatorPanel, Ohlcv, OnchainPanel};
    use xvision_core::trading::{AssetSymbol, Regime};

    /// Build a snapshot with `n` bars each at `close_price`.
    fn snapshot_with_bars(n: usize, close_price: f64) -> MarketSnapshot {
        let bars: Vec<Ohlcv> = (0..n)
            .map(|_| Ohlcv {
                timestamp: Utc::now(),
                open: close_price,
                high: close_price,
                low: close_price,
                close: close_price,
                volume: 1_000.0,
            })
            .collect();
        MarketSnapshot {
            cycle_id: Uuid::new_v4(),
            asset: AssetSymbol::Btc,
            timestamp: Utc::now(),
            price: close_price,
            volume_24h: None,
            recent_bars: bars,
            indicators: IndicatorPanel::default(),
            onchain: OnchainPanel::default(),
            regime: Regime::Bull,
            horizon_hours: 24,
        }
    }

    /// Build a snapshot whose bars represent a golden cross: last `fast`
    /// bars priced higher than the preceding bars.
    fn snapshot_golden_cross() -> MarketSnapshot {
        // 90 bars: first 60 at 100.0, last 30 at 200.0
        // sma_30  = mean of last 30 = 200.0
        // sma_90  = mean of all 90  = (60*100 + 30*200)/90 = 133.3
        // → sma_30 > sma_90
        let mut bars: Vec<Ohlcv> = (0..60)
            .map(|_| Ohlcv {
                timestamp: Utc::now(),
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 1_000.0,
            })
            .collect();
        bars.extend((0..30).map(|_| Ohlcv {
            timestamp: Utc::now(),
            open: 200.0,
            high: 200.0,
            low: 200.0,
            close: 200.0,
            volume: 1_000.0,
        }));
        MarketSnapshot {
            cycle_id: Uuid::new_v4(),
            asset: AssetSymbol::Btc,
            timestamp: Utc::now(),
            price: 200.0,
            volume_24h: None,
            recent_bars: bars,
            indicators: IndicatorPanel::default(),
            onchain: OnchainPanel::default(),
            regime: Regime::Bull,
            horizon_hours: 24,
        }
    }

    #[tokio::test]
    async fn decide_returns_expected_shape_golden_cross() {
        let strat = MaCrossover::new(30, 90);

        // First call: both SMAs equal (90 bars at 100) → warmup, no signal
        let flat_snap = snapshot_with_bars(90, 100.0);
        let warmup = strat.decide(&flat_snap).await;
        assert!(warmup.is_none(), "first call must be warmup None");

        // Second call: golden cross setup
        let cross_snap = snapshot_golden_cross();
        let dec = strat
            .decide(&cross_snap)
            .await
            .expect("golden cross must return Some");
        assert_eq!(dec.cycle_id, cross_snap.cycle_id, "cycle_id must propagate");
        assert_eq!(dec.action, Action::Buy);
        assert_eq!(dec.direction, Direction::Long);
        assert_eq!(dec.size_bps, 1000);
    }

    #[tokio::test]
    async fn edge_case_warmup_returns_none_when_insufficient_bars() {
        let strat = MaCrossover::new(30, 90);
        // Only 50 bars — not enough for slow_window=90
        let snap = snapshot_with_bars(50, 100.0);
        assert!(
            strat.decide(&snap).await.is_none(),
            "insufficient bars must return None"
        );
    }

    #[tokio::test]
    async fn edge_case_no_crossover_returns_none() {
        let strat = MaCrossover::new(30, 90);
        // Warmup
        let flat = snapshot_with_bars(90, 100.0);
        strat.decide(&flat).await;
        // Same price → no crossover
        let still_flat = snapshot_with_bars(90, 100.0);
        assert!(
            strat.decide(&still_flat).await.is_none(),
            "no crossover must return None"
        );
    }
}

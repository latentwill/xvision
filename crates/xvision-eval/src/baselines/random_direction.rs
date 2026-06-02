//! RandomDirection baseline — fair coin-flip between Buy Long and Sell Short
//! on each snapshot. Seeded via `StdRng` for deterministic backtests.
//!
//! Interior mutability: `Mutex<StdRng>` — the RNG state advances each call,
//! so it cannot be `&self`-compatible without a cell. `Mutex` is idiomatic for
//! non-Copy state that must be mutated through a shared reference.

use std::sync::Mutex;

use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use xvision_core::market::MarketSnapshot;
use xvision_core::trading::{Action, Direction, TraderDecision};

use crate::algorithm::Algorithm;

pub struct RandomDirection {
    pub rng_seed: u64,
    rng: Mutex<StdRng>,
}

impl RandomDirection {
    pub fn new(rng_seed: u64) -> Self {
        Self {
            rng_seed,
            rng: Mutex::new(StdRng::seed_from_u64(rng_seed)),
        }
    }
}

#[async_trait]
impl Algorithm for RandomDirection {
    fn name(&self) -> &'static str {
        "random_direction"
    }

    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
        let go_long = {
            let mut rng = self.rng.lock().expect("RandomDirection RNG poisoned");
            rng.gen::<bool>()
        };

        let (action, direction) = if go_long {
            (Action::Buy, Direction::Long)
        } else {
            (Action::Sell, Direction::Short)
        };

        Some(TraderDecision {
            cycle_id: snapshot.cycle_id,
            action,
            size_bps: 100,
            direction,
            stop_loss_pct: 2.0,
            take_profit_pct: 3.0,
            trader_summary: "RandomDirection: coin-flip long/short at 100 bps.".into(),
            asset: snapshot.asset,
            trailing_stop_pct: None,
            breakeven_trigger_pct: None,
            breakeven_offset_pct: None,
            fade_sl_bars: None,
            fade_sl_start_pct: None,
            fade_sl_end_pct: None,
            max_bars_held: None,
            sl_atr_mult: None,
            tp_atr_mult: None,
            tp1_pct: None,
            tp1_close_fraction: None,
            tp2_pct: None,
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

    fn fixture_snapshot() -> MarketSnapshot {
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
            indicators: IndicatorPanel::default(),
            onchain: OnchainPanel::default(),
            regime: Regime::Chop,
            horizon_hours: 24,
        }
    }

    #[tokio::test]
    async fn decide_returns_expected_shape() {
        let snap = fixture_snapshot();
        let strat = RandomDirection::new(42);
        let dec = strat.decide(&snap).await.expect("must always return Some");
        assert_eq!(dec.cycle_id, snap.cycle_id, "cycle_id must propagate");
        assert_eq!(dec.size_bps, 100);
        assert!((dec.stop_loss_pct - 2.0).abs() < f32::EPSILON);
        assert!((dec.take_profit_pct - 3.0).abs() < f32::EPSILON);
        // action/direction must be a valid long or short pair
        let is_long = dec.action == Action::Buy && dec.direction == Direction::Long;
        let is_short = dec.action == Action::Sell && dec.direction == Direction::Short;
        assert!(is_long || is_short, "must be Long or Short pair");
    }

    #[tokio::test]
    async fn edge_case_determinism_same_seed() {
        // Two instances with the same seed must produce identical first 5 decisions.
        let snap = fixture_snapshot();
        let strat_a = RandomDirection::new(99);
        let strat_b = RandomDirection::new(99);

        let mut decisions_a = Vec::new();
        let mut decisions_b = Vec::new();
        for _ in 0..5 {
            decisions_a.push(strat_a.decide(&snap).await.unwrap().direction);
            decisions_b.push(strat_b.decide(&snap).await.unwrap().direction);
        }

        assert_eq!(
            decisions_a, decisions_b,
            "same seed must yield identical decision sequence"
        );
    }
}

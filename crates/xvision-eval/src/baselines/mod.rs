//! Phase 7 baselines — null and classical-technical strategies that consume
//! `MarketSnapshot` and emit `TraderDecision`-shaped outputs.
//!
//! All baselines implement [`crate::algorithm::Algorithm`] with `cycle_id`
//! propagated from the incoming snapshot.
//!
//! ## v1 baseline set (7 strategies)
//! | Baseline            | Signal                                    |
//! |---------------------|-------------------------------------------|
//! | BuyAndHold          | Buy once, hold — static long benchmark    |
//! | AlwaysLong          | Buy every bar at 500 bps                  |
//! | AlwaysShort         | Sell every bar at 500 bps                 |
//! | RandomDirection     | Fair coin-flip long/short, seed-stable    |
//! | RsiMeanReversion    | RSI-14 < 30 → long, > 70 → short         |
//! | MaCrossover         | SMA(fast) × SMA(slow) crossover           |
//! | MacdMomentum        | MACD hist zero-line crossover             |
//! | BollingerATRBreakout| BB breakout confirmed by ATR              |
//!
//! ## v1.1 follow-ups (not implemented — defer)
//! - Bollinger Band squeeze / expansion breakout
//! - Donchian channel breakout (N-bar high/low)
//! - Fibonacci retracement entry
//!
//! ## Out of scope
//! - Onchain baselines (Nansen smart-money, funding-rate fader, stablecoin
//!   inflow, liquidation cascade) — Phase 7.5; data sourcing is separate.
//! - XGBoost ML baseline — Phase 7.5+.

pub mod always_long;
pub mod always_short;
pub mod bar_baselines;
pub mod bollinger_atr_breakout;
pub mod buy_and_hold;
pub mod ma_crossover;
pub mod macd_momentum;
pub mod random_direction;
pub mod rsi_mean_reversion;

pub use always_long::AlwaysLong;
pub use always_short::AlwaysShort;
pub use bar_baselines::{compute_baselines, BaselineResult, BaselinesReport, RelativeTo};
pub use bollinger_atr_breakout::BollingerATRBreakout;
pub use buy_and_hold::BuyAndHold;
pub use ma_crossover::MaCrossover;
pub use macd_momentum::MacdMomentum;
pub use random_direction::RandomDirection;
pub use rsi_mean_reversion::RsiMeanReversion;

use crate::algorithm::Algorithm;

/// Construct the canonical v1 baseline set in evaluation order.
///
/// The returned `Vec` is always length 8 with distinct `name()` strings:
/// `["buy_and_hold", "always_long", "always_short", "random_direction",
///   "rsi_mean_reversion", "ma_crossover", "macd_momentum",
///   "bollinger_atr_breakout"]`
///
/// `seed` controls the `RandomDirection` RNG — pass a fixed value for
/// reproducible backtests.
pub fn default_v1_set(seed: u64) -> Vec<Box<dyn Algorithm>> {
    vec![
        Box::new(BuyAndHold::new()),
        Box::new(AlwaysLong),
        Box::new(AlwaysShort),
        Box::new(RandomDirection::new(seed)),
        Box::new(RsiMeanReversion::new()),
        Box::new(MaCrossover::new(30, 90)),
        Box::new(MacdMomentum::new()),
        Box::new(BollingerATRBreakout::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_v1_set_has_eight_baselines() {
        let set = default_v1_set(42);
        assert_eq!(set.len(), 8, "must return exactly 8 baselines");
    }

    #[test]
    fn default_v1_set_names_are_distinct() {
        let set = default_v1_set(42);
        let names: HashSet<&'static str> = set.iter().map(|s| s.name()).collect();
        assert_eq!(
            names.len(),
            8,
            "all 8 baseline names must be distinct; got: {:?}",
            names
        );
    }

    #[test]
    fn default_v1_set_name_order() {
        let set = default_v1_set(42);
        let names: Vec<&'static str> = set.iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            vec![
                "buy_and_hold",
                "always_long",
                "always_short",
                "random_direction",
                "rsi_mean_reversion",
                "ma_crossover",
                "macd_momentum",
                "bollinger_atr_breakout",
            ]
        );
    }
}

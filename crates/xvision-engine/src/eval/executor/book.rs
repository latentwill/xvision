//! Pooled multi-asset portfolio accounting for the eval executors.
//! One capital pool, per-asset positions, shared realized PnL.
//! equity = initial + realized + Σ position[a] * (mark[a] - entry[a]).

use std::collections::BTreeMap;
use xvision_core::trading::AssetSymbol;

#[derive(Debug, Clone, Copy)]
struct Leg { position: f64, entry_price: f64 } // +long / -short units

#[derive(Debug, Clone)]
pub struct PortfolioBook {
    initial: f64,
    realized: f64,
    legs: BTreeMap<AssetSymbol, Leg>,
}

impl PortfolioBook {
    pub fn new(initial: f64) -> Self {
        Self { initial, realized: 0.0, legs: BTreeMap::new() }
    }
    pub fn position(&self, a: AssetSymbol) -> f64 { self.legs.get(&a).map_or(0.0, |l| l.position) }
    pub fn entry_price(&self, a: AssetSymbol) -> f64 { self.legs.get(&a).map_or(0.0, |l| l.entry_price) }
    pub fn set_position(&mut self, a: AssetSymbol, position: f64, entry_price: f64) {
        if position == 0.0 { self.legs.remove(&a); }
        else { self.legs.insert(a, Leg { position, entry_price }); }
    }
    pub fn add_realized(&mut self, pnl: f64) { self.realized += pnl; }
    pub fn realized(&self) -> f64 { self.realized }
    /// Mark-to-market equity. `marks[a]` is the price to value asset `a` at;
    /// assets absent from `marks` contribute zero unrealized (treated flat-mark).
    pub fn equity(&self, marks: &BTreeMap<AssetSymbol, f64>) -> f64 {
        let unrealized: f64 = self.legs.iter()
            .map(|(a, l)| marks.get(a).map_or(0.0, |m| l.position * (m - l.entry_price)))
            .sum();
        self.initial + self.realized + unrealized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::trading::AssetSymbol::{Btc, Eth};

    #[test]
    fn equity_sums_pooled_realized_plus_per_asset_marks() {
        let mut b = PortfolioBook::new(100_000.0);
        b.set_position(Btc, 1.0, 50_000.0);
        b.set_position(Eth, 2.0, 2_000.0);
        b.add_realized(500.0);
        let marks = std::collections::BTreeMap::from([(Btc, 51_000.0), (Eth, 2_100.0)]);
        assert_eq!(b.equity(&marks), 100_000.0 + 500.0 + 1_000.0 + 200.0);
    }

    #[test]
    fn flat_book_equity_is_initial_plus_realized() {
        let mut b = PortfolioBook::new(100_000.0);
        b.add_realized(-250.0);
        assert_eq!(b.equity(&std::collections::BTreeMap::new()), 99_750.0);
    }

    #[test]
    fn set_position_zero_clears_leg() {
        let mut b = PortfolioBook::new(100_000.0);
        b.set_position(Btc, 1.0, 50_000.0);
        assert_eq!(b.position(Btc), 1.0);
        b.set_position(Btc, 0.0, 0.0);
        assert_eq!(b.position(Btc), 0.0);
    }
}

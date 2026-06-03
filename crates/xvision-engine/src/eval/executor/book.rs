//! Pooled multi-asset portfolio accounting for the eval executors.
//! One capital pool, per-asset positions, shared realized PnL.
//! equity = initial + realized + Σ position[a] * (mark[a] - entry[a]).

use std::collections::BTreeMap;
use xvision_core::trading::AssetSymbol;

#[derive(Debug, Clone, Copy)]
struct Leg {
    position: f64, // +long / -short units
    entry_price: f64,
    /// Last price this leg was marked at. Initialized to `entry_price` on
    /// `set_position` and updated via `mark`. Used as the unrealized
    /// fallback when an asset is absent from the `marks` map at a
    /// timestamp (e.g. a misaligned timeline gap), so the pooled equity
    /// carries the leg's last seen value instead of snapping to entry.
    last_mark: f64,
}

#[derive(Debug, Clone)]
pub struct PortfolioBook {
    initial: f64,
    realized: f64,
    legs: BTreeMap<AssetSymbol, Leg>,
}

impl PortfolioBook {
    pub fn new(initial: f64) -> Self {
        Self {
            initial,
            realized: 0.0,
            legs: BTreeMap::new(),
        }
    }
    pub fn position(&self, a: AssetSymbol) -> f64 {
        self.legs.get(&a).map_or(0.0, |l| l.position)
    }
    pub fn entry_price(&self, a: AssetSymbol) -> f64 {
        self.legs.get(&a).map_or(0.0, |l| l.entry_price)
    }
    pub fn set_position(&mut self, a: AssetSymbol, position: f64, entry_price: f64) {
        if position == 0.0 {
            self.legs.remove(&a);
        } else {
            self.legs.insert(
                a,
                Leg {
                    position,
                    entry_price,
                    last_mark: entry_price,
                },
            );
        }
    }
    /// Update the open leg's last-seen mark for asset `a`. No-op when there
    /// is no open leg for `a`. Call this at each timestamp for every asset
    /// that HAS a bar, so an asset absent at a later timestamp can fall back
    /// to its stored `last_mark` in `equity` instead of marking to entry.
    pub fn mark(&mut self, a: AssetSymbol, price: f64) {
        if let Some(leg) = self.legs.get_mut(&a) {
            leg.last_mark = price;
        }
    }
    pub fn add_realized(&mut self, pnl: f64) {
        self.realized += pnl;
    }
    pub fn realized(&self) -> f64 {
        self.realized
    }
    /// Number of assets currently holding a non-flat position. A leg is only
    /// present in `legs` while its position is non-zero (`set_position` with
    /// `0.0` removes it), so the leg count is the open-position count. Used by
    /// the `max_concurrent_positions` risk veto.
    pub fn open_position_count(&self) -> usize {
        self.legs.len()
    }
    /// Mark-to-market equity. `marks[a]` is the price to value asset `a` at;
    /// an asset absent from `marks` falls back to its stored `last_mark`
    /// (the last price it was marked at via `mark`/`set_position`) rather
    /// than contributing zero unrealized. This keeps the pooled NAV
    /// continuous across timeline gaps where an asset has no bar.
    pub fn equity(&self, marks: &BTreeMap<AssetSymbol, f64>) -> f64 {
        let unrealized: f64 = self
            .legs
            .iter()
            .map(|(a, l)| {
                let m = marks.get(a).copied().unwrap_or(l.last_mark);
                l.position * (m - l.entry_price)
            })
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

    #[test]
    fn absent_mark_falls_back_to_last_mark_not_entry() {
        // Open a leg at entry, advance its last-seen mark to a higher
        // price, then value the book with NO marks for that asset. The
        // unrealized must reflect the last mark (+1_000), not zero/entry.
        let mut b = PortfolioBook::new(100_000.0);
        b.set_position(Btc, 1.0, 50_000.0);
        b.mark(Btc, 51_000.0);
        let empty = std::collections::BTreeMap::new();
        assert_eq!(
            b.equity(&empty),
            100_000.0 + 1_000.0,
            "absent asset must carry its last mark's unrealized, not snap to entry"
        );
        // A fresh leg with no `mark` call falls back to entry (zero
        // unrealized) — last_mark initialized to entry_price.
        b.set_position(Eth, 2.0, 2_000.0);
        assert_eq!(
            b.equity(&empty),
            100_000.0 + 1_000.0 + 0.0,
            "un-marked leg falls back to entry (zero unrealized)"
        );
    }

    #[test]
    fn mark_is_noop_for_absent_leg() {
        let mut b = PortfolioBook::new(100_000.0);
        b.mark(Btc, 51_000.0); // no leg yet — must not panic / create a leg
        assert_eq!(b.position(Btc), 0.0);
        assert_eq!(b.equity(&std::collections::BTreeMap::new()), 100_000.0);
    }
}

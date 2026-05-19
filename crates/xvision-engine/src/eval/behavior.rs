//! Per-run action-distribution and behaviour-summary derivation.
//!
//! Computed on-demand from the persisted `DecisionRow` slice for a run.
//! All fields are best-effort: any field that cannot be reliably computed
//! from the available columns is omitted from the output struct (the field
//! is an `Option` that stays `None`) rather than returning a fabricated zero.
//!
//! ## What is computable today
//!
//! | Field              | Source                                   | Available |
//! |--------------------|------------------------------------------|-----------|
//! | `action_counts`    | `DecisionRow::action` string counts      | Yes       |
//! | `trades_opened`    | `long_open + short_open` count           | Yes       |
//! | `flat_rate`        | `(flat + hold) / total`                  | Yes       |
//! | `direct_flips`     | consecutive long→short or short→long     | Yes       |
//! | `avg_bars_held`    | need fill_price + position tracking      | Yes*      |
//! | `worst_trade_pct`  | `pnl_realized` rows from fill side       | Yes†      |
//! | `best_trade_pct`   | same                                     | Yes†      |
//!
//! *`avg_bars_held` is counted as the number of decision indices between an
//! opening action and the next closing action (`flat` or opposite open).
//! "Bars" here means decision steps, not time bars.
//!
//! †`worst_trade_pct` / `best_trade_pct` require `pnl_realized` to be
//! non-null AND a reference equity (fill_price × fill_size) to be non-null.
//! When neither is available we leave both as `None`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::eval::store::DecisionRow;

/// Action-kind counts keyed by canonical string:
/// `long_open`, `short_open`, `flat`, `hold`.
/// Any unrecognised action string is also included verbatim so the consumer
/// can detect schema drift rather than silently losing data.
pub type ActionCounts = BTreeMap<String, u64>;

/// Derived behaviour summary for one eval run.
/// All fields are `Option` — omitted when not computable from the schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorSummary {
    /// Counts of each canonical action type.
    pub action_counts: ActionCounts,
    /// Number of `long_open` + `short_open` decisions.
    pub trades_opened: u64,
    /// Fraction of decisions that were `hold` or `flat` (0.0–1.0).
    /// `None` when there are no decisions.
    pub flat_rate: Option<f64>,
    /// Count of direct position flips (long→short or short→long without
    /// an intervening `flat`).
    pub direct_flips: u64,
    /// Average number of decision steps a position was held, across all
    /// completed trades (open → close/flip).
    /// `None` when no completed trades exist.
    pub avg_bars_held: Option<f64>,
    /// Worst single-trade PnL as a percentage of equity at trade entry.
    /// `None` when `fill_price * fill_size` or `pnl_realized` is unavailable.
    pub worst_trade_pct: Option<f64>,
    /// Best single-trade PnL as a percentage of equity at trade entry.
    /// `None` when same conditions apply as `worst_trade_pct`.
    pub best_trade_pct: Option<f64>,
}

/// Derive a `BehaviorSummary` from a slice of `DecisionRow`s for a single run.
/// The rows must be ordered by `decision_index` ascending (as the store returns
/// them).
pub fn derive_behavior_summary(rows: &[DecisionRow]) -> BehaviorSummary {
    let action_counts = count_actions(rows);
    let trades_opened = action_counts.get("long_open").copied().unwrap_or(0)
        + action_counts.get("short_open").copied().unwrap_or(0);

    let flat_rate = if rows.is_empty() {
        None
    } else {
        let passive = action_counts.get("hold").copied().unwrap_or(0)
            + action_counts.get("flat").copied().unwrap_or(0);
        Some(passive as f64 / rows.len() as f64)
    };

    let direct_flips = count_direct_flips(rows);
    let avg_bars_held = avg_bars_held(rows);
    let (worst_trade_pct, best_trade_pct) = trade_pct_extremes(rows);

    BehaviorSummary {
        action_counts,
        trades_opened,
        flat_rate,
        direct_flips,
        avg_bars_held,
        worst_trade_pct,
        best_trade_pct,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn count_actions(rows: &[DecisionRow]) -> ActionCounts {
    let mut map = BTreeMap::new();
    for row in rows {
        *map.entry(row.action.clone()).or_insert(0u64) += 1;
    }
    map
}

/// Count consecutive position flips: a flip is when the action goes directly
/// from `long_open` to `short_open` (or vice-versa) without a `flat` or
/// `hold` in between.
///
/// We track the "current position side" (Long | Short | None) as we scan
/// through decisions; only `long_open` / `short_open` change it.  A flip is
/// when the new side differs from the current side.
fn count_direct_flips(rows: &[DecisionRow]) -> u64 {
    #[derive(PartialEq)]
    enum Side {
        Long,
        Short,
    }

    let mut current: Option<Side> = None;
    let mut flips = 0u64;

    for row in rows {
        match row.action.as_str() {
            "long_open" => {
                if current == Some(Side::Short) {
                    flips += 1;
                }
                current = Some(Side::Long);
            }
            "short_open" => {
                if current == Some(Side::Long) {
                    flips += 1;
                }
                current = Some(Side::Short);
            }
            "flat" => {
                current = None;
            }
            // "hold" — keep the same position; does not reset or flip.
            _ => {}
        }
    }
    flips
}

/// Average number of decision steps a position is held.
///
/// We scan decisions in order.  When we see `long_open` or `short_open` we
/// note the opening index.  When the position closes (next `flat`, next
/// opposite open, or end-of-slice) we record the duration and accumulate.
///
/// Returns `None` when there are no opened positions.
fn avg_bars_held(rows: &[DecisionRow]) -> Option<f64> {
    let mut durations: Vec<u64> = Vec::new();
    let mut open_at: Option<(u32, &str)> = None; // (decision_index, side)

    for row in rows {
        match row.action.as_str() {
            side @ ("long_open" | "short_open") => {
                // If we were already open, close the prior trade before
                // opening the new one (flip or re-entry).
                if let Some((start, _)) = open_at {
                    durations.push((row.decision_index - start) as u64);
                }
                open_at = Some((row.decision_index, side));
            }
            "flat" => {
                if let Some((start, _)) = open_at.take() {
                    durations.push((row.decision_index - start) as u64);
                }
            }
            _ => {}
        }
    }

    if durations.is_empty() {
        None
    } else {
        let total: u64 = durations.iter().sum();
        Some(total as f64 / durations.len() as f64)
    }
}

/// Return the (worst, best) single-trade PnL percentages.
///
/// We define a "trade PnL %" as `pnl_realized / abs(fill_price * fill_size)`.
/// Only rows where both `pnl_realized` and `fill_price * fill_size` are
/// non-zero are considered.  Returns `(None, None)` when no such rows exist.
fn trade_pct_extremes(rows: &[DecisionRow]) -> (Option<f64>, Option<f64>) {
    let pcts: Vec<f64> = rows
        .iter()
        .filter_map(|r| {
            let pnl = r.pnl_realized?;
            let price = r.fill_price?;
            let size = r.fill_size?;
            let notional = (price * size).abs();
            if notional == 0.0 {
                return None;
            }
            Some(pnl / notional * 100.0)
        })
        .collect();

    if pcts.is_empty() {
        (None, None)
    } else {
        let worst = pcts.iter().cloned().fold(f64::INFINITY, f64::min);
        let best = pcts.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (Some(worst), Some(best))
    }
}

// ---------------------------------------------------------------------------
// Tests (RED-GREEN discipline — tests written before implementation)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_row(decision_index: u32, action: &str) -> DecisionRow {
        DecisionRow {
            run_id: "run1".into(),
            decision_index,
            timestamp: Utc::now(),
            asset: "ETH/USD".into(),
            action: action.into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
        }
    }

    fn make_fill_row(decision_index: u32, action: &str, fill_price: f64, fill_size: f64, pnl: f64) -> DecisionRow {
        DecisionRow {
            run_id: "run1".into(),
            decision_index,
            timestamp: Utc::now(),
            asset: "ETH/USD".into(),
            action: action.into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: Some(fill_price),
            fill_size: Some(fill_size),
            fee: None,
            pnl_realized: Some(pnl),
        }
    }

    // -----------------------------------------------------------------------
    // count_actions
    // -----------------------------------------------------------------------

    #[test]
    fn action_counts_empty() {
        let counts = count_actions(&[]);
        assert!(counts.is_empty());
    }

    #[test]
    fn action_counts_all_four_kinds() {
        let rows = vec![
            make_row(0, "long_open"),
            make_row(1, "hold"),
            make_row(2, "hold"),
            make_row(3, "flat"),
            make_row(4, "short_open"),
            make_row(5, "hold"),
        ];
        let counts = count_actions(&rows);
        assert_eq!(counts["long_open"], 1);
        assert_eq!(counts["short_open"], 1);
        assert_eq!(counts["flat"], 1);
        assert_eq!(counts["hold"], 3);
    }

    #[test]
    fn action_counts_unknown_action_preserved() {
        let rows = vec![make_row(0, "mystery")];
        let counts = count_actions(&rows);
        assert_eq!(counts["mystery"], 1);
    }

    // -----------------------------------------------------------------------
    // count_direct_flips
    // -----------------------------------------------------------------------

    #[test]
    fn no_flips_when_empty() {
        assert_eq!(count_direct_flips(&[]), 0);
    }

    #[test]
    fn no_flips_with_flat_between() {
        let rows = vec![
            make_row(0, "long_open"),
            make_row(1, "hold"),
            make_row(2, "flat"),
            make_row(3, "short_open"),
        ];
        assert_eq!(count_direct_flips(&rows), 0);
    }

    #[test]
    fn one_flip_long_to_short() {
        let rows = vec![
            make_row(0, "long_open"),
            make_row(1, "hold"),
            make_row(2, "short_open"), // direct flip
        ];
        assert_eq!(count_direct_flips(&rows), 1);
    }

    #[test]
    fn one_flip_short_to_long() {
        let rows = vec![
            make_row(0, "short_open"),
            make_row(1, "short_open"), // re-entry, not a flip
            make_row(2, "long_open"),  // flip
        ];
        assert_eq!(count_direct_flips(&rows), 1);
    }

    #[test]
    fn two_flips() {
        let rows = vec![
            make_row(0, "long_open"),
            make_row(1, "short_open"), // flip 1
            make_row(2, "long_open"),  // flip 2
        ];
        assert_eq!(count_direct_flips(&rows), 2);
    }

    // -----------------------------------------------------------------------
    // avg_bars_held
    // -----------------------------------------------------------------------

    #[test]
    fn avg_bars_held_no_open() {
        let rows = vec![make_row(0, "flat"), make_row(1, "hold")];
        assert_eq!(avg_bars_held(&rows), None);
    }

    #[test]
    fn avg_bars_held_single_trade() {
        // open at 0, flat at 4 → held 4 bars
        let rows = vec![
            make_row(0, "long_open"),
            make_row(1, "hold"),
            make_row(2, "hold"),
            make_row(3, "hold"),
            make_row(4, "flat"),
        ];
        assert_eq!(avg_bars_held(&rows), Some(4.0));
    }

    #[test]
    fn avg_bars_held_two_trades() {
        // trade 1: open 0 → flat 3 (3 bars)
        // trade 2: open 5 → flat 9 (4 bars)
        // average = 3.5
        let rows = vec![
            make_row(0, "long_open"),
            make_row(1, "hold"),
            make_row(2, "hold"),
            make_row(3, "flat"),
            make_row(4, "hold"),
            make_row(5, "short_open"),
            make_row(6, "hold"),
            make_row(7, "hold"),
            make_row(8, "hold"),
            make_row(9, "flat"),
        ];
        assert_eq!(avg_bars_held(&rows), Some(3.5));
    }

    #[test]
    fn avg_bars_held_unclosed_trade_still_counted() {
        // open at 0, no close → duration not counted (trade still open)
        let rows = vec![
            make_row(0, "long_open"),
            make_row(1, "hold"),
            make_row(2, "hold"),
        ];
        // Unclosed trades are NOT counted (they're in-progress)
        assert_eq!(avg_bars_held(&rows), None);
    }

    // -----------------------------------------------------------------------
    // trade_pct_extremes
    // -----------------------------------------------------------------------

    #[test]
    fn trade_pct_extremes_no_fills() {
        let rows = vec![make_row(0, "long_open"), make_row(1, "flat")];
        assert_eq!(trade_pct_extremes(&rows), (None, None));
    }

    #[test]
    fn trade_pct_extremes_single_trade() {
        // fill_price=100, fill_size=1.0, pnl=10 → pct = 10/100 * 100 = 10%
        let rows = vec![make_fill_row(0, "flat", 100.0, 1.0, 10.0)];
        let (worst, best) = trade_pct_extremes(&rows);
        assert_eq!(worst, Some(10.0));
        assert_eq!(best, Some(10.0));
    }

    #[test]
    fn trade_pct_extremes_two_trades() {
        // trade 1: fill_price=100, fill_size=1.0, pnl=-5 → pct = -5%
        // trade 2: fill_price=200, fill_size=0.5, pnl=20 → pct = 20/100 * 100 = 20%
        let rows = vec![
            make_fill_row(0, "flat", 100.0, 1.0, -5.0),
            make_fill_row(1, "flat", 200.0, 0.5, 20.0),
        ];
        let (worst, best) = trade_pct_extremes(&rows);
        assert_eq!(worst, Some(-5.0));
        assert_eq!(best, Some(20.0));
    }

    // -----------------------------------------------------------------------
    // derive_behavior_summary (integration)
    // -----------------------------------------------------------------------

    #[test]
    fn behavior_summary_empty_rows() {
        let s = derive_behavior_summary(&[]);
        assert!(s.action_counts.is_empty());
        assert_eq!(s.trades_opened, 0);
        assert_eq!(s.flat_rate, None);
        assert_eq!(s.direct_flips, 0);
        assert_eq!(s.avg_bars_held, None);
        assert_eq!(s.worst_trade_pct, None);
        assert_eq!(s.best_trade_pct, None);
    }

    #[test]
    fn behavior_summary_typical_run() {
        // Typical: 1 long open, 6 short open, 35 flat, 7 hold → total 49
        let mut rows = Vec::new();
        rows.push(make_row(0, "long_open"));
        for i in 1..8 {
            rows.push(make_row(i, "short_open"));
        }
        for i in 8..43 {
            rows.push(make_row(i, "flat"));
        }
        for i in 43..50 {
            rows.push(make_row(i, "hold"));
        }
        let s = derive_behavior_summary(&rows);
        assert_eq!(s.action_counts["long_open"], 1);
        assert_eq!(s.action_counts["short_open"], 7);
        assert_eq!(s.action_counts["flat"], 35);
        assert_eq!(s.action_counts["hold"], 7);
        assert_eq!(s.trades_opened, 8);
        // passive = flat(35) + hold(7) = 42 out of 50
        let expected_flat_rate = 42.0 / 50.0;
        assert!((s.flat_rate.unwrap() - expected_flat_rate).abs() < 1e-10);
    }
}

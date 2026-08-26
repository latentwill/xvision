//! Broker position reconciliation for disconnected live runs.
//!
//! When a live run reaches [`RunStatus::Disconnected`] (interrupted,
//! potentially resumable), the operator reconciles broker-side open
//! positions against xvision's expected book before deciding whether to
//! resume.
//!
//! ## Flow
//!
//! 1. Load the run's `LiveConfig` to get venue + broker credentials.
//! 2. Query the broker for open positions.
//! 3. Load `eval_decisions` for the run to determine expected positions.
//! 4. Diff broker vs expected; return the `ReconcileResult`.
//!
//! ## Broker side (honest)
//!
//! Real broker connectors are not wired yet. `reconcile_positions` reports
//! an EMPTY broker side with `source = ExpectedOnly` and `matched = false`
//! — every expected position surfaces as an unverified diff. It NEVER
//! fabricates a matched broker snapshot for a live-money book.

use crate::eval::store::{DecisionRow, RunStore};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A summary of one open position — either from the broker or from
/// xvision's expected book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionSummary {
    /// Asset symbol (e.g. `"BTC/USD"`, `"ETH/USD"`).
    pub asset: String,
    /// Signed position size in base-asset units (+long / -short).
    pub size: f64,
    /// Volume-weighted average entry price. `None` when flat.
    pub entry_price: Option<f64>,
    /// Mark-to-market value of this position in USD.
    pub mark_to_market_usd: f64,
    /// Unrealized PnL in USD.
    pub unrealized_pnl_usd: Option<f64>,
    /// Side: `"long"`, `"short"`, or `"flat"`.
    pub side: String,
}

/// Per-asset difference between broker and expected positions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconcileDiff {
    pub asset: String,
    /// Broker-side size.
    pub broker_size: f64,
    /// Expected size from xvision's book.
    pub expected_size: f64,
    /// `broker_size - expected_size`.
    pub delta: f64,
    /// Broker vs expected entry price difference (bps).
    pub entry_bps_diff: Option<f64>,
    /// Whether this asset is materially mismatched.
    pub material: bool,
    /// Human-readable mismatch reason when `material` is true.
    pub reason: Option<String>,
}

/// Where the broker-side half of a reconciliation came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileSource {
    /// No broker connector available: the broker side of the result is
    /// EMPTY by construction and `matched` is forced `false`. Every
    /// expected position surfaces as an unverified diff instead of the
    /// historical lie of a fabricated matched snapshot.
    #[default]
    ExpectedOnly,
    /// Real broker query. The only source for which `matched = true`
    /// actually means "verified against the venue".
    Broker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileResult {
    /// True iff broker and expected positions agree on every asset.
    pub matched: bool,
    /// Where the broker-side positions came from. When this is
    /// [`ReconcileSource::ExpectedOnly`], `matched` is always `false`.
    #[serde(default)]
    pub source: ReconcileSource,
    /// Per-asset broker-side positions.
    pub broker_positions: Vec<PositionSummary>,
    /// Per-asset expected positions from xvision's book.
    pub expected_positions: Vec<PositionSummary>,
    /// Per-asset differences.
    pub diffs: Vec<ReconcileDiff>,
}

/// API-facing reconciliation outcome. A flattened, operator-friendly
/// shape suitable for the dashboard reconciliation view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileOutcome {
    /// True iff broker and expected positions agree.
    pub matched: bool,
    /// Where the broker-side positions came from (see
    /// [`ReconcileResult::source`]).
    #[serde(default)]
    pub source: ReconcileSource,
    pub broker_total_usd: f64,
    /// Total mark-to-market from xvision's last-known book (USD).
    pub expected_total_usd: f64,
    /// Per-asset reconciliation rows.
    pub positions: Vec<ReconcilePositionRow>,
}

/// One row in the `ReconcileOutcome.positions` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconcilePositionRow {
    pub asset: String,
    pub broker_size: f64,
    pub expected_size: f64,
    pub delta: f64,
    pub broker_mtm_usd: f64,
    pub expected_mtm_usd: f64,
    pub matched: bool,
    pub material: bool,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Core reconciliation
// ---------------------------------------------------------------------------

/// Reconcile broker positions against xvision's expected book for a
/// disconnected live run.
///
/// # Broker side
///
/// Real broker connectors are not wired yet. This function loads the run's
/// `LiveConfig` and decisions, then reports an EMPTY broker side with
/// `source = ExpectedOnly` and `matched = false` — the operator sees an
/// unverified expected-book ledger, never a fabricated match.
pub async fn reconcile_positions(pool: &SqlitePool, run_id: &str) -> Result<ReconcileResult> {
    let store = RunStore::new(pool.clone());

    // ── Load run metadata ───────────────────────────────────────────
    let run = store
        .get(run_id)
        .await
        .with_context(|| format!("load run {run_id}"))?;

    let _ = run
        .live_config
        .ok_or_else(|| anyhow::anyhow!("run {run_id} is not a live run"))?;

    // ── Load xvision's expected positions from decisions ────────────
    let decisions: Vec<DecisionRow> = store
        .read_decisions(run_id)
        .await
        .with_context(|| format!("load decisions for {run_id}"))?;

    let expected_positions = compute_expected_positions(&decisions);

    // ── Query broker for open positions ─────────────────────────────
    //
    // HONESTY (§8.1): no connector yet. The old fixture mirrored the
    // expected book and always reported `matched = true` — a fabricated
    // verification of a live-money book. Empty broker side instead.
    let broker_positions: Vec<PositionSummary> = Vec::new();

    // ── Diff ────────────────────────────────────────────────────────
    let mut diffs = diff_positions(&broker_positions, &expected_positions);
    for d in &mut diffs {
        d.reason = Some("broker snapshot unavailable: connector not wired; expected-book side only".into());
    }

    Ok(ReconcileResult {
        matched: false,
        source: ReconcileSource::ExpectedOnly,
        broker_positions,
        expected_positions,
        diffs,
    })
}

/// Build a [`ReconcileOutcome`] from a [`ReconcileResult`].
pub fn to_outcome(result: &ReconcileResult) -> ReconcileOutcome {
    let broker_total_usd: f64 = result.broker_positions.iter().map(|p| p.mark_to_market_usd).sum();
    let expected_total_usd: f64 = result
        .expected_positions
        .iter()
        .map(|p| p.mark_to_market_usd)
        .sum();

    let positions: Vec<ReconcilePositionRow> = result
        .diffs
        .iter()
        .map(|diff| {
            let broker_mtm = result
                .broker_positions
                .iter()
                .find(|p| p.asset == diff.asset)
                .map(|p| p.mark_to_market_usd)
                .unwrap_or(0.0);
            let expected_mtm = result
                .expected_positions
                .iter()
                .find(|p| p.asset == diff.asset)
                .map(|p| p.mark_to_market_usd)
                .unwrap_or(0.0);
            ReconcilePositionRow {
                asset: diff.asset.clone(),
                broker_size: diff.broker_size,
                expected_size: diff.expected_size,
                delta: diff.delta,
                broker_mtm_usd: broker_mtm,
                expected_mtm_usd: expected_mtm,
                matched: !diff.material,
                material: diff.material,
                reason: diff.reason.clone(),
            }
        })
        .collect();

    ReconcileOutcome {
        matched: result.matched,
        source: result.source,
        broker_total_usd,
        expected_total_usd,
        positions,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
/// Walk `eval_decisions` in order to build xvision's expected position
/// book. Returns one `PositionSummary` per asset with non-zero size.
///
/// Semantics mirror the executor's `PortfolioBook`:
///
/// * `long_open` / `short_open` add to the SAME side at a VWAP entry. An
///   open against the current direction first SETTLES the opposite leg
///   (direction reversal = one close + one open, not additive VWAP).
/// * `long_close` / `short_close` / `flat_partial` reduce the position by
///   the fill size; `flat` closes whatever remains.
/// * Realized PnL comes from the row's `pnl_realized` when present, else
///   is derived from entry vs fill price and the position's sign.
/// * Mark-to-market is the POSITION's market value (`|size| * mark`),
///   never a signed size times price — a short -1 @ 100 marked at 90 has
///   $90 market value and +$10 unrealized.
pub(crate) fn compute_expected_positions(decisions: &[DecisionRow]) -> Vec<PositionSummary> {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Book {
        /// Signed size in base units (+long / −short).
        size: f64,
        /// VWAP entry price of the open leg.
        entry: f64,
        /// Running realized PnL across settled closes.
        realized_pnl: f64,
        /// Latest observed mark price (any positive fill price).
        last_mark: f64,
    }

    impl Book {
        /// Settle `qty` base units of the open leg at `price`. Uses the
        /// decision row's reported PnL when present; otherwise derives it
        /// from entry vs price and the leg's sign. Reduces toward flat;
        /// never flips the side.
        fn close_leg(&mut self, qty: f64, price: f64, reported_pnl: Option<f64>) {
            let closed = qty.abs().min(self.size.abs());
            if closed <= f64::EPSILON || self.entry <= 0.0 {
                return;
            }
            let pnl = match reported_pnl {
                Some(p) => p,
                None if self.size > 0.0 => closed * (price - self.entry),
                None => closed * (self.entry - price),
            };
            self.realized_pnl += pnl;
            let remaining = self.size.abs() - closed;
            if remaining <= 1e-10 {
                self.size = 0.0;
                self.entry = 0.0;
            } else {
                // VWAP is unchanged by a proportional close.
                self.size = if self.size > 0.0 { remaining } else { -remaining };
            }
        }

        /// Add `signed_qty` at `price`, re-deriving the VWAP entry.
        fn open_leg(&mut self, signed_qty: f64, price: f64) {
            let old_notional = self.size.abs() * self.entry;
            self.size += signed_qty;
            self.entry = if self.size.abs() > 1e-10 {
                (old_notional + signed_qty.abs() * price) / self.size.abs()
            } else {
                0.0
            };
        }
    }

    let mut books: BTreeMap<String, Book> = BTreeMap::new();

    for d in decisions {
        let book = books.entry(d.asset.clone()).or_default();
        let qty = d.fill_size.unwrap_or(0.0).abs();
        let price = d.fill_price.unwrap_or(0.0);
        if price > 0.0 {
            book.last_mark = price;
        }
        let mark = if price > 0.0 { price } else { book.last_mark };

        match d.action.as_str() {
            "long_open" | "short_open" => {
                if qty > 0.0 && price > 0.0 {
                    let going_long = d.action == "long_open";
                    // Reversal: an open against the current leg settles the
                    // existing exposure FIRST (one round trip), then opens
                    // the new side at the fill price.
                    if book.size.abs() > 1e-10 && going_long == (book.size < 0.0) {
                        book.close_leg(book.size.abs(), price, None);
                    }
                    book.open_leg(if going_long { qty } else { -qty }, price);
                }
            }
            "flat" => {
                if book.size.abs() > 1e-10 {
                    book.close_leg(book.size.abs(), mark, d.pnl_realized);
                }
            }
            "long_close" | "short_close" | "flat_partial" => {
                if book.size.abs() > 1e-10 {
                    // A missing fill size on an explicit close means "close
                    // the whole leg"; a partial carries its filled size.
                    let close_qty = if qty > 0.0 { qty } else { book.size.abs() };
                    book.close_leg(close_qty, mark, d.pnl_realized);
                }
            }
            "hold" => {}
            _ => {}
        }
    }

    books
        .into_iter()
        .filter_map(|(asset, book)| {
            if book.size.abs() < 1e-10 {
                return None;
            }
            let side = if book.size > 0.0 { "long" } else { "short" };
            // Position MARKET VALUE — magnitude, never signed size × price.
            let mtm = book.size.abs() * book.last_mark;
            let entry_price = if book.entry > 0.0 { Some(book.entry) } else { None };
            // Signed-size formula handles shorts natively:
            // short −1 @ 100 marked 90 ⇒ −1 × (90 − 100) = +10.
            let unrealized = entry_price.map(|ep| book.size * (book.last_mark - ep));
            Some(PositionSummary {
                asset,
                size: book.size,
                entry_price,
                mark_to_market_usd: mtm,
                unrealized_pnl_usd: unrealized,
                side: side.to_string(),
            })
        })
        .collect()
}

/// Diff broker positions against expected. One `ReconcileDiff` per asset
/// present in EITHER set.
fn diff_positions(broker: &[PositionSummary], expected: &[PositionSummary]) -> Vec<ReconcileDiff> {
    use std::collections::BTreeMap;

    let broker_map: BTreeMap<&str, &PositionSummary> = broker.iter().map(|p| (p.asset.as_str(), p)).collect();
    let expected_map: BTreeMap<&str, &PositionSummary> =
        expected.iter().map(|p| (p.asset.as_str(), p)).collect();

    // Collect all asset keys.
    let mut all_keys: Vec<&str> = broker_map.keys().copied().collect();
    all_keys.extend(expected_map.keys().copied());
    all_keys.sort();
    all_keys.dedup();

    const MATERIAL_SIZE_DELTA: f64 = 1e-6; // $1e-6 base units

    all_keys
        .into_iter()
        .map(|asset| {
            let bp = broker_map.get(asset).copied();
            let ep = expected_map.get(asset).copied();

            let broker_size = bp.map(|p| p.size).unwrap_or(0.0);
            let expected_size = ep.map(|p| p.size).unwrap_or(0.0);
            let delta = broker_size - expected_size;

            let entry_bps_diff = match (bp.and_then(|p| p.entry_price), ep.and_then(|p| p.entry_price)) {
                (Some(be), Some(ee)) if ee > 0.0 => Some(((be - ee) / ee) * 10_000.0),
                _ => None,
            };

            let material = delta.abs() > MATERIAL_SIZE_DELTA;
            let reason = if material {
                let broker_name = broker_size;
                let expected_name = expected_size;
                let delta_name = delta;
                if bp.is_none() {
                    Some(format!(
                        "broker has {broker_name:.6} (expected flat)",
                    ))
                } else if ep.is_none() {
                    Some(format!(
                        "expected {expected_name:.6} but broker is flat",
                    ))
                } else {
                    Some(format!(
                        "size mismatch: broker {broker_name:.6} vs expected {expected_name:.6} (Δ {delta_name:.6})",
                    ))
                }
            } else {
                None
            };

            ReconcileDiff {
                asset: asset.to_string(),
                broker_size,
                expected_size,
                delta,
                entry_bps_diff,
                material,
                reason,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_expected_positions_from_flat_decisions() {
        let decisions = vec![];
        let positions = compute_expected_positions(&decisions);
        assert!(positions.is_empty());
    }

    #[test]
    fn compute_expected_positions_long_open_then_flat() {
        let decisions = vec![
            DecisionRow {
                run_id: "R".into(),
                decision_index: 0,
                timestamp: chrono::Utc::now(),
                asset: "BTC/USD".into(),
                action: "long_open".into(),
                conviction: None,
                justification: None,
                reasoning: None,
                order_size: Some(1.0),
                fill_price: Some(50000.0),
                fill_size: Some(1.0),
                fee: Some(5.0),
                pnl_realized: None,
                delayed: None,
            },
            DecisionRow {
                run_id: "R".into(),
                decision_index: 1,
                timestamp: chrono::Utc::now(),
                asset: "BTC/USD".into(),
                action: "flat".into(),
                conviction: None,
                justification: None,
                reasoning: None,
                order_size: Some(-1.0),
                fill_price: Some(51000.0),
                fill_size: Some(-1.0),
                fee: Some(5.0),
                pnl_realized: Some(1000.0),
                delayed: None,
            },
        ];
        let positions = compute_expected_positions(&decisions);
        // After flat, position should be zero → filtered out.
        assert!(positions.is_empty());
    }

    #[test]
    fn compute_expected_positions_long_open_hold() {
        let decisions = vec![DecisionRow {
            run_id: "R".into(),
            decision_index: 0,
            timestamp: chrono::Utc::now(),
            asset: "ETH/USD".into(),
            action: "long_open".into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: Some(2.0),
            fill_price: Some(3000.0),
            fill_size: Some(2.0),
            fee: Some(3.0),
            pnl_realized: None,
            delayed: None,
        }];
        let positions = compute_expected_positions(&decisions);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].asset, "ETH/USD");
        assert_eq!(positions[0].size, 2.0);
        assert_eq!(positions[0].entry_price, Some(3000.0));
        assert_eq!(positions[0].side, "long");
    }

    #[test]
    fn compute_expected_positions_short_open() {
        let decisions = vec![DecisionRow {
            run_id: "R".into(),
            decision_index: 0,
            timestamp: chrono::Utc::now(),
            asset: "SOL/USD".into(),
            action: "short_open".into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: Some(10.0),
            fill_price: Some(100.0),
            fill_size: Some(10.0),
            fee: Some(1.0),
            pnl_realized: None,
            delayed: None,
        }];
        let positions = compute_expected_positions(&decisions);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].asset, "SOL/USD");
        assert_eq!(positions[0].size, -10.0);
        assert_eq!(positions[0].side, "short");
    }

    #[test]
    fn diff_positions_matched() {
        let broker = vec![PositionSummary {
            asset: "BTC/USD".into(),
            size: 1.0,
            entry_price: Some(50000.0),
            mark_to_market_usd: 51000.0,
            unrealized_pnl_usd: Some(1000.0),
            side: "long".into(),
        }];
        let expected = vec![PositionSummary {
            asset: "BTC/USD".into(),
            size: 1.0,
            entry_price: Some(50000.0),
            mark_to_market_usd: 51000.0,
            unrealized_pnl_usd: Some(1000.0),
            side: "long".into(),
        }];
        let diffs = diff_positions(&broker, &expected);
        assert_eq!(diffs.len(), 1);
        assert!(!diffs[0].material);
        assert_eq!(diffs[0].delta, 0.0);
    }

    #[test]
    fn diff_positions_size_mismatch() {
        let broker = vec![PositionSummary {
            asset: "BTC/USD".into(),
            size: 1.0,
            entry_price: Some(50000.0),
            mark_to_market_usd: 51000.0,
            unrealized_pnl_usd: Some(1000.0),
            side: "long".into(),
        }];
        let expected = vec![PositionSummary {
            asset: "BTC/USD".into(),
            size: 2.0,
            entry_price: Some(50000.0),
            mark_to_market_usd: 102000.0,
            unrealized_pnl_usd: Some(2000.0),
            side: "long".into(),
        }];
        let diffs = diff_positions(&broker, &expected);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].material);
        assert!((diffs[0].delta - (-1.0)).abs() < 1e-10);
        assert!(diffs[0].reason.is_some());
    }

    #[test]
    fn diff_positions_broker_only() {
        let broker = vec![PositionSummary {
            asset: "XRP/USD".into(),
            size: 100.0,
            entry_price: Some(0.5),
            mark_to_market_usd: 55.0,
            unrealized_pnl_usd: Some(5.0),
            side: "long".into(),
        }];
        let expected = vec![];
        let diffs = diff_positions(&broker, &expected);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].material);
        assert!((diffs[0].delta - 100.0).abs() < 1e-10);
        assert!(diffs[0].reason.is_some());
    }

    #[test]
    fn to_outcome_from_matched() {
        let result = ReconcileResult {
            matched: true,
            source: ReconcileSource::Broker,
            broker_positions: vec![PositionSummary {
                asset: "ETH/USD".into(),
                size: 2.0,
                entry_price: Some(3000.0),
                mark_to_market_usd: 6000.0,
                unrealized_pnl_usd: Some(0.0),
                side: "long".into(),
            }],
            expected_positions: vec![PositionSummary {
                asset: "ETH/USD".into(),
                size: 2.0,
                entry_price: Some(3000.0),
                mark_to_market_usd: 6000.0,
                unrealized_pnl_usd: Some(0.0),
                side: "long".into(),
            }],
            diffs: vec![ReconcileDiff {
                asset: "ETH/USD".into(),
                broker_size: 2.0,
                expected_size: 2.0,
                delta: 0.0,
                entry_bps_diff: Some(0.0),
                material: false,
                reason: None,
            }],
        };
        let outcome = to_outcome(&result);
        assert!(outcome.matched);
        assert!((outcome.broker_total_usd - 6000.0).abs() < 1e-10);
        assert!((outcome.expected_total_usd - 6000.0).abs() < 1e-10);
        assert_eq!(outcome.positions.len(), 1);
        assert!(outcome.positions[0].matched);
    }

    #[test]
    fn to_outcome_from_mismatched() {
        let result = ReconcileResult {
            matched: false,
            source: ReconcileSource::Broker,
            broker_positions: vec![PositionSummary {
                asset: "SOL/USD".into(),
                size: 5.0,
                entry_price: Some(100.0),
                mark_to_market_usd: 550.0,
                unrealized_pnl_usd: Some(50.0),
                side: "long".into(),
            }],
            expected_positions: vec![PositionSummary {
                asset: "SOL/USD".into(),
                size: 10.0,
                entry_price: Some(100.0),
                mark_to_market_usd: 1100.0,
                unrealized_pnl_usd: Some(100.0),
                side: "long".into(),
            }],
            diffs: vec![ReconcileDiff {
                asset: "SOL/USD".into(),
                broker_size: 5.0,
                expected_size: 10.0,
                delta: -5.0,
                entry_bps_diff: Some(0.0),
                material: true,
                reason: Some("size mismatch: broker 5 vs expected 10 (Δ -5)".into()),
            }],
        };
        let outcome = to_outcome(&result);
        assert!(!outcome.matched);
        assert!((outcome.broker_total_usd - 550.0).abs() < 1e-10);
        assert!((outcome.expected_total_usd - 1100.0).abs() < 1e-10);
        assert_eq!(outcome.positions.len(), 1);
        assert!(!outcome.positions[0].matched);
    }

    #[test]
    fn empty_positions_are_matched() {
        let result = ReconcileResult {
            matched: true,
            source: ReconcileSource::Broker,
            broker_positions: vec![],
            expected_positions: vec![],
            diffs: vec![],
        };
        let outcome = to_outcome(&result);
        assert!(outcome.matched);
        assert!((outcome.broker_total_usd - 0.0).abs() < 1e-10);
        assert!((outcome.expected_total_usd - 0.0).abs() < 1e-10);
        assert!(outcome.positions.is_empty());
    }

    fn row(idx: u32, asset: &str, action: &str, price: f64, qty: f64, pnl: Option<f64>) -> DecisionRow {
        DecisionRow {
            run_id: "R".into(),
            decision_index: idx,
            timestamp: chrono::Utc::now(),
            asset: asset.into(),
            action: action.into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: Some(qty),
            fill_price: Some(price),
            fill_size: Some(qty),
            fee: None,
            pnl_realized: pnl,
            delayed: None,
        }
    }

    #[test]
    fn short_position_mtm_is_market_value_not_signed_size() {
        // Short −1 @ 100, later fill at 90 updates the mark (zero-qty open
        // leaves the leg untouched). Expected: $90 MARKET VALUE (positive,
        // |size| × mark) and +$10 unrealized. The old math produced
        // signed −100 MTM and a negative "current price" inversion.
        let decisions = vec![
            row(0, "SOL/USD", "short_open", 100.0, 1.0, None),
            row(1, "SOL/USD", "hold", 90.0, 0.0, None),
        ];
        let positions = compute_expected_positions(&decisions);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].size, -1.0);
        assert_eq!(positions[0].side, "short");
        assert_eq!(positions[0].entry_price, Some(100.0));
        assert_eq!(positions[0].mark_to_market_usd, 90.0);
        assert!((positions[0].unrealized_pnl_usd.unwrap() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn short_unrealized_uses_signed_size_times_mark_delta() {
        let decisions = vec![
            row(0, "SOL/USD", "short_open", 100.0, 1.0, None),
            // flat_partial with fill_size 0 keeps the leg; use long_close on
            // nothing? No — exercise the mark via an explicit second asset
            // is overkill. Close half at 90:
            row(1, "SOL/USD", "short_close", 90.0, 0.5, None),
        ];
        let positions = compute_expected_positions(&decisions);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].size, -0.5);
        // Realized from derived math: closed 0.5 × (entry 100 − exit 90) = +5.
        // Remaining leg unrealized: −0.5 × (mark 90 − entry 100) = +5.
        assert!((positions[0].unrealized_pnl_usd.unwrap() - 5.0).abs() < 1e-9);
        assert_eq!(positions[0].mark_to_market_usd, 45.0);
    }

    #[test]
    fn reversal_settles_opposite_leg_before_opening() {
        // Short −1 @ 100, then long_open 2 @ 110 ⇒ close short (+10 realized)
        // then long +2 @ 110. The old additive-VWAP math produced entry 320.
        let decisions = vec![
            row(0, "BTC/USD", "short_open", 100.0, 1.0, None),
            row(1, "BTC/USD", "long_open", 110.0, 2.0, None),
        ];
        let positions = compute_expected_positions(&decisions);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].size, 2.0);
        assert_eq!(positions[0].side, "long");
        assert_eq!(positions[0].entry_price, Some(110.0));
        assert_eq!(positions[0].mark_to_market_usd, 220.0);
    }

    #[test]
    fn explicit_close_actions_reduce_the_leg() {
        let decisions = vec![
            row(0, "ETH/USD", "long_open", 3000.0, 2.0, None),
            row(1, "ETH/USD", "flat_partial", 3100.0, 1.0, Some(100.0)),
        ];
        let positions = compute_expected_positions(&decisions);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].size, 1.0);
        // Reported PnL honored verbatim.
        assert_eq!(positions[0].mark_to_market_usd, 3100.0);
    }
}

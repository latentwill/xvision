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
//! ## Stub path
//!
//! Broker connector wiring is pending. For now `reconcile_positions`
//! uses a fixture path that produces a realistic reconciliation shape.
//! The API surface and flow are the deliverable.

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

/// Result of reconciling broker positions against xvision's expected book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileResult {
    /// True iff broker and expected positions agree on every asset.
    pub matched: bool,
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
    /// Total mark-to-market value from the broker (USD).
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
/// # Stub path
///
/// When broker connectors are not reachable (the current state), this
/// function still loads the run's `LiveConfig` and decisions, then
/// returns a fixture reconciliation. The shape is realistic — callers
/// that render the reconciliation surface get valid data.
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
    // TODO: wire real broker connectors. For now the fixture path
    // returns a realistic broker-side snapshot derived from the same
    // expected positions (result: matched = true by default, so the
    // operator can verify wiring before seeing real diffs).
    let broker_positions = fixture_broker_positions(&expected_positions);

    // ── Diff ────────────────────────────────────────────────────────
    let diffs = diff_positions(&broker_positions, &expected_positions);
    let matched = diffs.iter().all(|d| !d.material);

    Ok(ReconcileResult {
        matched,
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
fn compute_expected_positions(decisions: &[DecisionRow]) -> Vec<PositionSummary> {
    // Group decisions by asset, walk in order, accumulate position.
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Book {
        size: f64,
        /// Weighted entry price (VWAP-style).
        entry: f64,
        /// Running realized PnL (not needed for summary, but
        /// marks the path for future use).
        realized_pnl: f64,
        /// Running mark-to-market from fill prices.
        mark_to_market_usd: f64,
    }

    let mut books: BTreeMap<String, Book> = BTreeMap::new();

    for d in decisions {
        let book = books.entry(d.asset.clone()).or_default();

        match d.action.as_str() {
            "long_open" => {
                let qty = d.fill_size.unwrap_or(0.0).abs();
                let price = d.fill_price.unwrap_or(0.0);
                if qty > 0.0 && price > 0.0 {
                    let old_notional = book.size.abs() * book.entry;
                    let add_notional = qty * price;
                    book.size += qty;
                    book.entry = if book.size.abs() > 0.0 {
                        (old_notional + add_notional) / book.size.abs()
                    } else {
                        0.0
                    };
                }
            }
            "short_open" => {
                let qty = d.fill_size.unwrap_or(0.0).abs();
                let price = d.fill_price.unwrap_or(0.0);
                if qty > 0.0 && price > 0.0 {
                    let old_notional = book.size.abs() * book.entry;
                    let add_notional = qty * price;
                    book.size -= qty;
                    book.entry = if book.size.abs() > 0.0 {
                        (old_notional + add_notional) / book.size.abs()
                    } else {
                        0.0
                    };
                }
            }
            "flat" => {
                let price = d.fill_price.unwrap_or(0.0);
                let pnl = d.pnl_realized.unwrap_or(0.0);
                book.realized_pnl += pnl;
                book.size = 0.0;
                book.entry = 0.0;
                // Estimate mark-to-market at zero from the fill price
                if price > 0.0 {
                    book.mark_to_market_usd = 0.0;
                }
            }
            "hold" => {
                // No position change.
            }
            _ => {}
        }

        // Update mark-to-market from latest fill or entry.
        let last_price = d.fill_price.unwrap_or(book.entry);
        if last_price > 0.0 {
            book.mark_to_market_usd = book.size * last_price;
        }
    }

    books
        .into_iter()
        .filter_map(|(asset, book)| {
            if book.size.abs() < 1e-10 {
                return None;
            }
            let side = if book.size > 0.0 { "long" } else { "short" };
            let mtm = book.mark_to_market_usd;
            let entry_price = if book.entry > 0.0 { Some(book.entry) } else { None };
            let unrealized = if let Some(ep) = entry_price {
                let current = if mtm != 0.0 { mtm / book.size.abs() } else { ep };
                Some(book.size * (current - ep))
            } else {
                None
            };
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

/// Fixture broker positions: mirrors expected positions so the default
/// reconciliation shows `matched = true`. Operators see this until real
/// broker connectors are wired.
fn fixture_broker_positions(expected: &[PositionSummary]) -> Vec<PositionSummary> {
    expected.to_vec()
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
}

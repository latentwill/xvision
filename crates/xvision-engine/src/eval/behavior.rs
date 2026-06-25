//! Behavior summary — derived from `DecisionRow` data on read.
//!
//! No DB writes; call `derive_behavior_summary` over a pre-loaded
//! `&[DecisionRow]` slice. The derivation is intentionally small and
//! auditable so operators can understand exactly what each field means.
//!
//! ## Field definitions
//!
//! * `flat_rate` — fraction of decisions whose action is `flat` or `hold`.
//! * `trades_opened` — count of `long_open` + `short_open`.
//! * `direct_flips` — consecutive `long_open → short_open` or
//!   `short_open → long_open` without a `flat` in between (per asset).
//! * `avg_bars_held` — mean bar-count between an open and the next `flat`
//!   on the same asset. Computed independently per asset and then averaged.
//! * `reentries_after_loss` — `long_open` or `short_open` immediately
//!   following a `flat` whose `pnl_realized < 0`.
//! * `exits_on_invalidation` — `flat` decisions with `pnl_realized < 0`
//!   (closed at a loss).
//! * `primary_failure_mode` — see `primary_failure_mode()`.
//! * `action_counts` — per-action distribution over the decisions table:
//!   `long_open`, `short_open`, `flat`, `hold`, `long_close`, `short_close`.
//!   Together they account for every row whose action enum we know;
//!   unknown action strings are ignored.
//! * `repeated_opens` — number of open decisions whose immediately prior
//!   decision on the same asset was *also* an open in the same direction
//!   without an intervening close. Tracks "the bot keeps stacking into the
//!   same side" — distinct from `direct_flips` which counts opposite-side
//!   open-after-open transitions. A `hold` between two same-direction opens
//!   does **not** break the chain (hold is no-op for position state); a
//!   `flat`, `long_close`, or `short_close` does. Computed per asset.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::eval::store::DecisionRow;

/// Per-action decision counts for a run.
///
/// Covers every action enum the engine emits today: the four open/flat/hold
/// variants and the two directional-close variants. Unknown action strings
/// are ignored — the surface intentionally does not carry an `other`
/// catch-all because the action set is a versioned wire enum, not free text.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionCounts {
    pub long_open: u32,
    pub short_open: u32,
    pub flat: u32,
    pub hold: u32,
    pub long_close: u32,
    pub short_close: u32,
}

impl ActionCounts {
    /// Count of decisions that opened a position: long_open + short_open.
    pub fn opens(&self) -> u32 {
        self.long_open + self.short_open
    }
    /// Count of decisions that closed a position via either explicit-close
    /// variant: long_close + short_close. Does **not** include `flat`,
    /// which the trader emits as a position-clear primitive.
    pub fn closes(&self) -> u32 {
        self.long_close + self.short_close
    }
    /// Round-trip count: opens + closes. Used by report surfaces as the
    /// "trade count" headline (every open and every directional close is one
    /// trade leg the executor would have routed).
    pub fn trades(&self) -> u32 {
        self.opens() + self.closes()
    }
}

/// Behavior summary derived from a run's decision rows.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorSummary {
    /// Fraction of all decisions that are `flat` or `hold` (0.0–1.0).
    pub flat_rate: f64,
    /// Count of `long_open` + `short_open` decisions.
    pub trades_opened: u32,
    /// Consecutive open-direction flips without a flat between them.
    pub direct_flips: u32,
    /// Mean bars between an `open` and the next `flat` for the same asset.
    /// `None` when there are no completed round-trips.
    pub avg_bars_held: Option<f64>,
    /// Opens immediately following a `flat` with `pnl_realized < 0`.
    pub reentries_after_loss: u32,
    /// `flat` decisions with `pnl_realized < 0` (closed at a loss).
    pub exits_on_invalidation: u32,
    /// Heuristic label for the most likely failure mode.
    pub primary_failure_mode: String,
    /// Per-action decision counts. Appended 2026-05-22 for the
    /// `cli-report-actions-and-tokens` track. `Default` is the all-zeroes
    /// counts struct, so older callers deserializing payloads without this
    /// field still parse — `#[serde(default)]` keeps the JSON shape
    /// backward-compatible.
    #[serde(default)]
    pub action_counts: ActionCounts,
    /// Number of same-direction "stacking" opens on the same asset without
    /// an intervening close (see module doc).
    #[serde(default)]
    pub repeated_opens: u32,
}

/// Derive a `BehaviorSummary` from a slice of decision rows.
///
/// The slice should be ordered by `decision_index ASC` (which is how
/// `RunStore::read_decisions` returns it). The function is pure —
/// no DB calls, no side effects.
pub fn derive_behavior_summary(decisions: &[DecisionRow]) -> BehaviorSummary {
    if decisions.is_empty() {
        return BehaviorSummary {
            flat_rate: 0.0,
            trades_opened: 0,
            direct_flips: 0,
            avg_bars_held: None,
            reentries_after_loss: 0,
            exits_on_invalidation: 0,
            primary_failure_mode: primary_failure_mode(0.0, 0, 0, 0, None).to_string(),
            action_counts: ActionCounts::default(),
            repeated_opens: 0,
        };
    }

    let total = decisions.len() as f64;

    // --- flat_rate ---
    let flat_or_hold_count = decisions
        .iter()
        .filter(|d| matches!(d.action.as_str(), "flat" | "hold"))
        .count();
    let flat_rate = flat_or_hold_count as f64 / total;

    // --- trades_opened ---
    let trades_opened = decisions
        .iter()
        .filter(|d| matches!(d.action.as_str(), "long_open" | "short_open"))
        .count() as u32;

    // --- direct_flips (per asset) ---
    // A direct flip is a long_open immediately following a short_open (or
    // vice versa) on the same asset, with no flat in between.
    let direct_flips = count_direct_flips(decisions);

    // --- avg_bars_held (per asset) ---
    let avg_bars_held = compute_avg_bars_held(decisions);

    // --- reentries_after_loss ---
    // An open that immediately follows a flat with pnl_realized < 0.
    let reentries_after_loss = count_reentries_after_loss(decisions);

    // --- exits_on_invalidation ---
    let exits_on_invalidation = decisions
        .iter()
        .filter(|d| d.action == "flat" && d.pnl_realized.is_some_and(|p| p < 0.0))
        .count() as u32;

    let pfm = primary_failure_mode(
        flat_rate,
        trades_opened,
        direct_flips,
        exits_on_invalidation,
        reentries_after_loss,
    );

    let action_counts = count_actions(decisions);
    let repeated_opens = count_repeated_opens(decisions);

    BehaviorSummary {
        flat_rate,
        trades_opened,
        direct_flips,
        avg_bars_held,
        reentries_after_loss,
        exits_on_invalidation,
        primary_failure_mode: pfm.to_string(),
        action_counts,
        repeated_opens,
    }
}

/// Tally per-action decisions in a single pass. Unknown action strings are
/// ignored — see [`ActionCounts`] doc.
fn count_actions(decisions: &[DecisionRow]) -> ActionCounts {
    let mut c = ActionCounts::default();
    for d in decisions {
        match d.action.as_str() {
            "long_open" => c.long_open += 1,
            "short_open" => c.short_open += 1,
            "flat" => c.flat += 1,
            "hold" => c.hold += 1,
            "long_close" => c.long_close += 1,
            "short_close" => c.short_close += 1,
            _ => {}
        }
    }
    c
}

/// Count "stacking" opens (see module doc).
///
/// Per asset, track whether the most recent non-hold/non-open decision left
/// the position open in some direction. Each subsequent same-direction open
/// without an intervening close (`flat`, `long_close`, `short_close`)
/// increments the counter. `hold` is a no-op for position state and does
/// not break the chain; opposite-direction opens are counted by
/// `direct_flips`, not here.
fn count_repeated_opens(decisions: &[DecisionRow]) -> u32 {
    let mut last_open: HashMap<&str, &str> = HashMap::new();
    let mut count = 0u32;

    for d in decisions {
        match d.action.as_str() {
            "flat" | "long_close" | "short_close" => {
                last_open.remove(d.asset.as_str());
            }
            open @ ("long_open" | "short_open") => {
                if let Some(&prev) = last_open.get(d.asset.as_str()) {
                    if prev == open {
                        count += 1;
                    }
                }
                last_open.insert(d.asset.as_str(), open);
            }
            _ => {
                // hold and any unknown action: leave state alone.
            }
        }
    }
    count
}

/// Count consecutive open-direction flips on the same asset.
///
/// Scans decisions per asset. Tracks the last open direction; whenever the
/// next open is in the opposite direction without an intervening flat, that
/// is a direct flip.
fn count_direct_flips(decisions: &[DecisionRow]) -> u32 {
    // per-asset: last open action seen (no flat between it and now)
    let mut last_open: HashMap<&str, &str> = HashMap::new();
    let mut flips = 0u32;

    for d in decisions {
        match d.action.as_str() {
            "flat" => {
                // Only flat resets the "last open" for this asset — the
                // next open after a flat does NOT count as a direct flip.
                // `hold` is no-op: position unchanged, so a later opposite
                // open still counts as a direct flip per the module docs
                // ("opposite opens without a flat in between").
                last_open.remove(d.asset.as_str());
            }
            open @ ("long_open" | "short_open") => {
                if let Some(&prev) = last_open.get(d.asset.as_str()) {
                    if prev != open {
                        flips += 1;
                    }
                }
                last_open.insert(d.asset.as_str(), open);
            }
            _ => {}
        }
    }
    flips
}

/// Compute the mean number of decision steps between an open and the next
/// flat on the same asset. Returns `None` when no complete round-trips exist.
fn compute_avg_bars_held(decisions: &[DecisionRow]) -> Option<f64> {
    // per-asset: (decision_index of the most recent open)
    let mut open_at: HashMap<&str, u32> = HashMap::new();
    let mut durations: Vec<u32> = Vec::new();

    for d in decisions {
        match d.action.as_str() {
            "long_open" | "short_open" => {
                open_at.insert(d.asset.as_str(), d.decision_index);
            }
            "flat" => {
                if let Some(&opened_idx) = open_at.get(d.asset.as_str()) {
                    let bars = d.decision_index.saturating_sub(opened_idx);
                    durations.push(bars);
                    open_at.remove(d.asset.as_str());
                }
            }
            _ => {}
        }
    }

    if durations.is_empty() {
        return None;
    }
    let sum: u32 = durations.iter().sum();
    Some(sum as f64 / durations.len() as f64)
}

/// Count opens that immediately follow a flat with negative realized PnL
/// *on the same asset*. A losing flat on BTC followed by an open on ETH is
/// not a reentry.
fn count_reentries_after_loss(decisions: &[DecisionRow]) -> u32 {
    // per-asset: whether the most recent flat for that asset closed at a loss
    let mut last_flat_was_loss: HashMap<&str, bool> = HashMap::new();
    let mut count = 0u32;

    for d in decisions {
        match d.action.as_str() {
            "flat" => {
                last_flat_was_loss.insert(d.asset.as_str(), d.pnl_realized.is_some_and(|p| p < 0.0));
            }
            "long_open" | "short_open" => {
                if last_flat_was_loss.get(d.asset.as_str()).copied().unwrap_or(false) {
                    count += 1;
                }
                // After accounting for the reentry on this asset, clear its
                // loss flag — a subsequent open on the same asset without an
                // intervening losing flat is not a reentry-after-loss.
                last_flat_was_loss.insert(d.asset.as_str(), false);
            }
            "hold" => {
                // hold does not change the per-asset loss flag; we want to
                // catch opens that come after a flat+loss even with holds
                // in between, on the same asset.
            }
            _ => {}
        }
    }
    count
}

/// Heuristic primary failure mode label.
///
/// Rules are evaluated in priority order; the first match wins.
/// All rules are kept in this one function so the complete logic is
/// auditable in a single place.
///
/// | Rule             | Condition                                          |
/// |------------------|----------------------------------------------------|
/// | `late_entries`   | reentries_after_loss / max(1, trades) > 0.4        |
/// | `churn`          | direct_flips / max(1, trades) > 0.2                |
/// | `no_edge`        | trades > 0 AND exits_on_invalidation / max(1, trades) > 0.5 |
/// | `over_flat`      | flat_rate > 0.85                                   |
/// | `none_obvious`   | fallthrough                                        |
fn primary_failure_mode(
    flat_rate: f64,
    trades_opened: u32,
    direct_flips: u32,
    exits_on_invalidation: u32,
    reentries_after_loss: impl Into<Option<u32>>,
) -> &'static str {
    let trades = trades_opened.max(1) as f64;
    let reentries = reentries_after_loss.into().unwrap_or(0) as f64;

    if reentries / trades > 0.4 {
        return "late_entries";
    }
    if direct_flips as f64 / trades > 0.2 {
        return "churn";
    }
    if trades_opened > 0 && exits_on_invalidation as f64 / trades > 0.5 {
        return "no_edge";
    }
    if flat_rate > 0.85 {
        return "over_flat";
    }
    "none_obvious"
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn d(index: u32, asset: &str, action: &str, pnl: Option<f64>) -> DecisionRow {
        DecisionRow {
            run_id: "test-run".to_string(),
            decision_index: index,
            timestamp: Utc::now(),
            asset: asset.to_string(),
            action: action.to_string(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: pnl,
            delayed: None,
        }
    }

    // T1: empty decisions → zeroed summary, none_obvious
    #[test]
    fn test_empty_decisions() {
        let s = derive_behavior_summary(&[]);
        assert_eq!(s.flat_rate, 0.0);
        assert_eq!(s.trades_opened, 0);
        assert_eq!(s.direct_flips, 0);
        assert!(s.avg_bars_held.is_none());
        assert_eq!(s.reentries_after_loss, 0);
        assert_eq!(s.exits_on_invalidation, 0);
        // even empty falls through to none_obvious
        assert_eq!(s.primary_failure_mode, "none_obvious");
    }

    // T2: all hold → flat_rate = 1.0, trades_opened = 0, over_flat
    #[test]
    fn test_all_hold() {
        let decisions: Vec<DecisionRow> = (0..10).map(|i| d(i, "BTC", "hold", None)).collect();
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.flat_rate, 1.0);
        assert_eq!(s.trades_opened, 0);
        assert_eq!(s.primary_failure_mode, "over_flat");
    }

    // T3: single round-trip open → hold → flat
    #[test]
    fn test_single_round_trip() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "hold", None),
            d(2, "BTC", "hold", None),
            d(3, "BTC", "flat", Some(10.0)),
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.trades_opened, 1);
        assert_eq!(s.direct_flips, 0);
        assert_eq!(s.exits_on_invalidation, 0);
        // 3 bars held (index 3 − index 0)
        assert_eq!(s.avg_bars_held, Some(3.0));
        assert_eq!(s.reentries_after_loss, 0);
    }

    // T4: direct flip — long_open immediately followed by short_open on same asset
    #[test]
    fn test_direct_flip() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "short_open", None), // direct flip
            d(2, "BTC", "flat", Some(-5.0)),
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.direct_flips, 1);
        // 2 trades_opened, 1 flip → ratio 0.5 > 0.2 → churn
        assert_eq!(s.primary_failure_mode, "churn");
    }

    // T5: reentry after loss
    #[test]
    fn test_reentry_after_loss() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "flat", Some(-10.0)), // loss
            d(2, "BTC", "long_open", None),   // reentry
            d(3, "BTC", "flat", Some(5.0)),
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.reentries_after_loss, 1);
        assert_eq!(s.exits_on_invalidation, 1);
        // reentries/trades = 1/2 = 0.5 > 0.4 → late_entries
        assert_eq!(s.primary_failure_mode, "late_entries");
    }

    // T6: failure mode — late_entries bucket
    #[test]
    fn test_failure_mode_late_entries() {
        // reentries/trades > 0.4
        assert_eq!(primary_failure_mode(0.3, 5, 0, 1, Some(3_u32)), "late_entries");
    }

    // T7: failure mode — churn bucket
    #[test]
    fn test_failure_mode_churn() {
        // no high reentry rate, but flips/trades > 0.2
        assert_eq!(primary_failure_mode(0.3, 10, 3, 2, Some(0_u32)), "churn");
    }

    // T8: failure mode — no_edge bucket
    #[test]
    fn test_failure_mode_no_edge() {
        // exits_on_invalidation/trades > 0.5
        assert_eq!(primary_failure_mode(0.3, 4, 0, 3, Some(0_u32)), "no_edge");
    }

    // T9: failure mode — over_flat bucket
    #[test]
    fn test_failure_mode_over_flat() {
        // flat_rate > 0.85
        assert_eq!(primary_failure_mode(0.90, 1, 0, 0, Some(0_u32)), "over_flat");
    }

    // T10: failure mode — none_obvious fallthrough
    #[test]
    fn test_failure_mode_none_obvious() {
        assert_eq!(primary_failure_mode(0.50, 5, 0, 1, Some(0_u32)), "none_obvious");
    }

    // T11: avg_bars_held with no complete round trips → None
    #[test]
    fn test_avg_bars_held_no_round_trips() {
        let decisions = vec![d(0, "BTC", "long_open", None), d(1, "BTC", "hold", None)];
        let s = derive_behavior_summary(&decisions);
        assert!(s.avg_bars_held.is_none());
    }

    // T12: avg_bars_held multi-asset
    #[test]
    fn test_avg_bars_held_multi_asset() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(2, "BTC", "flat", Some(1.0)), // 2 bars on BTC
            d(0, "ETH", "short_open", None),
            d(4, "ETH", "flat", Some(1.0)), // 4 bars on ETH
        ];
        let s = derive_behavior_summary(&decisions);
        // avg of [2, 4] = 3.0
        assert_eq!(s.avg_bars_held, Some(3.0));
    }

    // Regression: `hold` between two opposite opens must NOT reset
    // the direct-flip state. The module docs define direct_flips as
    // opposite opens "without a flat in between" — hold is no-op.
    #[test]
    fn test_direct_flip_survives_hold_between_opens() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "hold", None),
            d(2, "BTC", "short_open", None), // still a direct flip (no flat between)
            d(3, "BTC", "flat", Some(-1.0)),
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.direct_flips, 1);
    }

    // action_counts: every known enum variant is tallied; unknown strings
    // (defensive, since the wire enum is closed) are ignored.
    #[test]
    fn test_action_counts_tallies_each_variant() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "hold", None),
            d(2, "BTC", "long_close", Some(5.0)),
            d(3, "BTC", "short_open", None),
            d(4, "BTC", "short_close", Some(-1.0)),
            d(5, "BTC", "flat", Some(0.0)),
            d(6, "BTC", "hold", None),
            d(7, "BTC", "weird_unknown", None),
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.action_counts.long_open, 1);
        assert_eq!(s.action_counts.short_open, 1);
        assert_eq!(s.action_counts.long_close, 1);
        assert_eq!(s.action_counts.short_close, 1);
        assert_eq!(s.action_counts.flat, 1);
        assert_eq!(s.action_counts.hold, 2);
        assert_eq!(s.action_counts.opens(), 2);
        assert_eq!(s.action_counts.closes(), 2);
        assert_eq!(s.action_counts.trades(), 4);
    }

    // repeated_opens: same-direction open after open on the same asset,
    // with no close in between, counts once per subsequent stack.
    #[test]
    fn test_repeated_opens_counts_same_direction_stacking() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "long_open", None), // +1
            d(2, "BTC", "long_open", None), // +2
            d(3, "BTC", "flat", Some(0.0)),
            d(4, "BTC", "long_open", None), // not counted (after flat)
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.repeated_opens, 2);
    }

    // repeated_opens: a `hold` between two same-direction opens does not
    // break the chain (hold is no-op for position state).
    #[test]
    fn test_repeated_opens_survives_hold_between_opens() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "hold", None),
            d(2, "BTC", "long_open", None), // +1
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.repeated_opens, 1);
    }

    // repeated_opens: opposite-direction opens are counted by direct_flips,
    // not repeated_opens.
    #[test]
    fn test_repeated_opens_excludes_direct_flips() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "short_open", None), // direct flip, not a repeat
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.repeated_opens, 0);
        assert_eq!(s.direct_flips, 1);
    }

    // repeated_opens: an explicit close (long_close / short_close) breaks
    // the chain just like a flat does.
    #[test]
    fn test_repeated_opens_close_breaks_chain() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "long_close", Some(5.0)),
            d(2, "BTC", "long_open", None), // not counted (chain broken)
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.repeated_opens, 0);
    }

    // Regression: reentries_after_loss must be tracked per-asset. A losing
    // flat on BTC followed by an open on ETH is NOT a reentry — different
    // asset, different position.
    #[test]
    fn test_reentry_after_loss_is_per_asset() {
        let decisions = vec![
            d(0, "BTC", "long_open", None),
            d(1, "BTC", "flat", Some(-10.0)), // BTC loss
            d(2, "ETH", "long_open", None),   // ETH open — NOT a reentry
            d(3, "ETH", "flat", Some(5.0)),
        ];
        let s = derive_behavior_summary(&decisions);
        assert_eq!(s.reentries_after_loss, 0);
        // 1 loss-flat on BTC: exits_on_invalidation = 1
        assert_eq!(s.exits_on_invalidation, 1);
    }
}

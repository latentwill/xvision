//! Stage 3 event + summary types.
//!
//! These are the shapes the engine emits to `events.jsonl` and to the
//! run export per the Filter v1 spec (`docs/superpowers/specs/2026-05-21-filter-v1.md`,
//! §Export shape). The crate stays engine-independent: types here are
//! pure data + a deterministic aggregator, with conversion helpers that
//! turn a Stage-2 [`crate::runtime::FilterEvalOutcome`] into the public
//! event shape so the engine-side per-bar hook can call one function.
//!
//! Wire-shape stability: `FilterEventV1` keeps the `_v1` suffix so a
//! v2 successor can land alongside without breaking the v1 reader path.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::runtime::{ActivationDecision, ConditionResult, FilterEvalOutcome, Transition};
use crate::types::FilterId;

/// Why a would-have-triggered bar produced no LLM wakeup.
///
/// `None` on the event means "no suppression"; the event was either a
/// real trip (`triggered = true`) or a non-active outcome
/// (Inactive / Warming / Hold) where there was nothing to suppress.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressedReason {
    /// `wake_when_in_position` blocked the trip while a position was open.
    InPosition,
    /// Within the cooldown window after the most recent trip.
    Cooldown,
    /// Filter's `max_wakeups_per_day` ceiling had been reached for the
    /// bar's UTC day.
    DailyCap,
}

/// One per-bar emission for a filter evaluated in FilterGated mode.
///
/// Emitted to the same `events.jsonl` channel that carries the V2D
/// `memory_*` events and the V2E trace events. Read by the run export,
/// the run-detail panel (Stage 4), and the golden regression fixtures
/// (Stage 5).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterEventV1 {
    /// Schema version. Always `1` for v1; bumped when the shape
    /// changes incompatibly.
    pub schema_version: u32,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub bar_timestamp: DateTime<Utc>,
    pub filter_id: FilterId,
    /// True iff the runtime returned `Active { transition: Trip }` — the
    /// bar an LLM dispatch fired. `false` for Inactive / Warming / Hold
    /// and for all suppressed outcomes.
    pub triggered: bool,
    /// Set when a would-have-triggered bar was suppressed by a runtime
    /// gate. `None` means no suppression was active — either a real
    /// trip or a no-op bar.
    #[serde(default)]
    pub suppressed_reason: Option<SuppressedReason>,
    /// Indices (into `ConditionTree::conditions()`) of leaves that
    /// evaluated `true` on this bar. Empty during warmup.
    pub conditions_passed: Vec<u32>,
    /// Indices of leaves that evaluated `false`. Empty during warmup.
    pub conditions_failed: Vec<u32>,
    /// Sparse map of `IndicatorRef.to_string()` → bar value
    /// (e.g. `"ema_20" → 42_133.12`). Empty during warmup. Includes
    /// only the indicators referenced by this filter.
    pub indicator_snapshot: BTreeMap<String, f64>,
}

impl FilterEventV1 {
    /// Wire-shape constant; matches the `schema_version` field set by
    /// the engine emit site.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Build an event from a runtime outcome + the engine-supplied bar
    /// timestamp + a pre-resolved indicator snapshot.
    ///
    /// The engine owns the snapshot construction because the
    /// `IndicatorEngine` lives on the engine side of the per-bar hook;
    /// passing it in here keeps `xvision-filters` agnostic about how
    /// the snapshot is materialised.
    pub fn from_outcome(
        filter_id: FilterId,
        bar_timestamp: DateTime<Utc>,
        outcome: &FilterEvalOutcome,
        indicator_snapshot: BTreeMap<String, f64>,
    ) -> Self {
        let (triggered, suppressed_reason) = classify(&outcome.decision);
        let (conditions_passed, conditions_failed) = split_conditions(&outcome.conditions_passed);
        Self {
            schema_version: Self::SCHEMA_VERSION,
            bar_timestamp,
            filter_id,
            triggered,
            suppressed_reason,
            conditions_passed,
            conditions_failed,
            indicator_snapshot,
        }
    }
}

/// Per-run rollup of `FilterEventV1` rows. One per filter per run.
///
/// Lands in the eval run summary export alongside the existing
/// token-economics block.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterSummary {
    pub filter_id: FilterId,
    pub bars_scanned: u32,
    pub wakeups: u32,
    pub suppressed_in_position: u32,
    pub suppressed_cooldown: u32,
    pub suppressed_daily_cap: u32,
    /// `bars_scanned - wakeups`. In FilterGated mode every non-wakeup
    /// bar is an LLM call that EveryBar would have paid for; this
    /// counts the savings.
    pub llm_calls_saved: u32,
    /// `llm_calls_saved * AVG_BRIEFING_TOKEN_COST`. v1 uses the global
    /// constant from `crate::AVG_BRIEFING_TOKEN_COST`; v1.5 will make
    /// this per-strategy-measured.
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub estimated_tokens_saved: u64,
}

impl FilterSummary {
    pub fn new(filter_id: FilterId) -> Self {
        Self {
            filter_id,
            bars_scanned: 0,
            wakeups: 0,
            suppressed_in_position: 0,
            suppressed_cooldown: 0,
            suppressed_daily_cap: 0,
            llm_calls_saved: 0,
            estimated_tokens_saved: 0,
        }
    }

    /// Aggregate a slice of events (one filter's worth) into a summary.
    ///
    /// Caller is responsible for filtering the events to a single
    /// `filter_id` — the function does not validate this. The
    /// `filter_id` argument is the canonical id written into the
    /// returned summary.
    pub fn from_events(filter_id: FilterId, events: &[FilterEventV1]) -> Self {
        let mut s = Self::new(filter_id);
        for e in events {
            s.bars_scanned = s.bars_scanned.saturating_add(1);
            if e.triggered {
                s.wakeups = s.wakeups.saturating_add(1);
            }
            match e.suppressed_reason {
                Some(SuppressedReason::InPosition) => {
                    s.suppressed_in_position = s.suppressed_in_position.saturating_add(1);
                }
                Some(SuppressedReason::Cooldown) => {
                    s.suppressed_cooldown = s.suppressed_cooldown.saturating_add(1);
                }
                Some(SuppressedReason::DailyCap) => {
                    s.suppressed_daily_cap = s.suppressed_daily_cap.saturating_add(1);
                }
                None => {}
            }
        }
        s.llm_calls_saved = s.bars_scanned.saturating_sub(s.wakeups);
        s.estimated_tokens_saved =
            u64::from(s.llm_calls_saved).saturating_mul(crate::AVG_BRIEFING_TOKEN_COST);
        s
    }
}

fn classify(d: &ActivationDecision) -> (bool, Option<SuppressedReason>) {
    match d {
        ActivationDecision::Active {
            transition: Transition::Trip,
        } => (true, None),
        ActivationDecision::Active {
            transition: Transition::Hold,
        }
        | ActivationDecision::Inactive
        | ActivationDecision::Warming { .. } => (false, None),
        ActivationDecision::Cooldown { .. } => (false, Some(SuppressedReason::Cooldown)),
        ActivationDecision::CappedForDay { .. } => (false, Some(SuppressedReason::DailyCap)),
        ActivationDecision::SuppressedInPosition => (false, Some(SuppressedReason::InPosition)),
    }
}

fn split_conditions(results: &[ConditionResult]) -> (Vec<u32>, Vec<u32>) {
    let mut passed = Vec::with_capacity(results.len());
    let mut failed = Vec::with_capacity(results.len());
    for (i, r) in results.iter().enumerate() {
        let i = i as u32;
        if r.passed {
            passed.push(i);
        } else {
            failed.push(i);
        }
    }
    (passed, failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).unwrap()
    }

    fn fid() -> FilterId {
        FilterId::new("01H_filter_test")
    }

    fn event(ts_sec: i64, triggered: bool, suppressed: Option<SuppressedReason>) -> FilterEventV1 {
        FilterEventV1 {
            schema_version: FilterEventV1::SCHEMA_VERSION,
            bar_timestamp: ts(ts_sec),
            filter_id: fid(),
            triggered,
            suppressed_reason: suppressed,
            conditions_passed: vec![],
            conditions_failed: vec![],
            indicator_snapshot: BTreeMap::new(),
        }
    }

    #[test]
    fn classify_maps_all_activation_decision_variants() {
        assert_eq!(
            classify(&ActivationDecision::Active {
                transition: Transition::Trip
            }),
            (true, None)
        );
        assert_eq!(
            classify(&ActivationDecision::Active {
                transition: Transition::Hold
            }),
            (false, None)
        );
        assert_eq!(classify(&ActivationDecision::Inactive), (false, None));
        assert_eq!(
            classify(&ActivationDecision::Warming { bars_left: 5 }),
            (false, None)
        );
        assert_eq!(
            classify(&ActivationDecision::SuppressedInPosition),
            (false, Some(SuppressedReason::InPosition))
        );
        assert_eq!(
            classify(&ActivationDecision::Cooldown { bars_left: 2 }),
            (false, Some(SuppressedReason::Cooldown))
        );
        assert_eq!(
            classify(&ActivationDecision::CappedForDay { wakeups_today: 3 }),
            (false, Some(SuppressedReason::DailyCap))
        );
    }

    #[test]
    fn summary_from_events_counts_each_bucket_once() {
        let events = vec![
            event(0, true, None),                                  // wakeup
            event(60, false, None),                                // inactive/hold/warming
            event(120, false, Some(SuppressedReason::InPosition)), // in position
            event(180, false, Some(SuppressedReason::Cooldown)),   // cooldown
            event(240, false, Some(SuppressedReason::Cooldown)),   // cooldown
            event(300, false, Some(SuppressedReason::DailyCap)),   // daily cap
            event(360, true, None),                                // wakeup
        ];

        let s = FilterSummary::from_events(fid(), &events);

        assert_eq!(s.bars_scanned, 7);
        assert_eq!(s.wakeups, 2);
        assert_eq!(s.suppressed_in_position, 1);
        assert_eq!(s.suppressed_cooldown, 2);
        assert_eq!(s.suppressed_daily_cap, 1);
    }

    #[test]
    fn summary_reconciles_counts() {
        // Plan §Stage 3 acceptance: every event is counted in exactly
        // one bucket. The "other" count (inactive/warming/hold) is the
        // residual: bars_scanned - wakeups - all suppression buckets.
        let events = vec![
            event(0, true, None),
            event(60, false, None),
            event(120, false, Some(SuppressedReason::Cooldown)),
            event(180, false, Some(SuppressedReason::DailyCap)),
            event(240, false, Some(SuppressedReason::InPosition)),
        ];
        let s = FilterSummary::from_events(fid(), &events);
        let other = s.bars_scanned
            - s.wakeups
            - s.suppressed_in_position
            - s.suppressed_cooldown
            - s.suppressed_daily_cap;
        assert_eq!(other, 1, "exactly one inactive/warming/hold bar");
    }

    #[test]
    fn summary_llm_calls_saved_equals_bars_minus_wakeups() {
        let mut events = Vec::new();
        for i in 0..100 {
            // 5 wakeups out of 100 cadence-gated bars.
            events.push(event(i, i < 5, None));
        }
        let s = FilterSummary::from_events(fid(), &events);
        assert_eq!(s.bars_scanned, 100);
        assert_eq!(s.wakeups, 5);
        assert_eq!(s.llm_calls_saved, 95);
        assert_eq!(s.estimated_tokens_saved, 95 * crate::AVG_BRIEFING_TOKEN_COST);
    }

    #[test]
    fn summary_empty_events_zero_everything() {
        let s = FilterSummary::from_events(fid(), &[]);
        assert_eq!(s.bars_scanned, 0);
        assert_eq!(s.wakeups, 0);
        assert_eq!(s.llm_calls_saved, 0);
        assert_eq!(s.estimated_tokens_saved, 0);
        assert_eq!(s.suppressed_in_position, 0);
        assert_eq!(s.suppressed_cooldown, 0);
        assert_eq!(s.suppressed_daily_cap, 0);
    }

    #[test]
    fn event_serde_roundtrip_preserves_shape() {
        // The `_v1` suffix on FilterEventV1 means readers can pin the
        // shape. Roundtrip through JSON and verify byte-equal struct.
        let mut snapshot = BTreeMap::new();
        snapshot.insert("ema_20".to_string(), 42_133.12);
        snapshot.insert("rsi_14".to_string(), 68.5);
        let original = FilterEventV1 {
            schema_version: 1,
            bar_timestamp: ts(1_716_000_000),
            filter_id: fid(),
            triggered: true,
            suppressed_reason: None,
            conditions_passed: vec![0, 2],
            conditions_failed: vec![1],
            indicator_snapshot: snapshot,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: FilterEventV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn event_suppressed_reason_is_null_when_none() {
        let e = event(0, true, None);
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["suppressed_reason"], serde_json::Value::Null);
    }

    #[test]
    fn event_suppressed_reason_serialises_as_snake_case_string() {
        let e = event(0, false, Some(SuppressedReason::Cooldown));
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["suppressed_reason"], serde_json::json!("cooldown"));
    }

    #[test]
    fn from_outcome_threads_runtime_decision_into_event() {
        let outcome = FilterEvalOutcome {
            decision: ActivationDecision::Cooldown { bars_left: 2 },
            conditions_passed: vec![
                ConditionResult { passed: true },
                ConditionResult { passed: false },
                ConditionResult { passed: true },
            ],
            tree_true: true,
        };
        let mut snap = BTreeMap::new();
        snap.insert("ema_20".to_string(), 100.0);
        let evt = FilterEventV1::from_outcome(fid(), ts(0), &outcome, snap.clone());
        assert!(!evt.triggered);
        assert_eq!(evt.suppressed_reason, Some(SuppressedReason::Cooldown));
        assert_eq!(evt.conditions_passed, vec![0, 2]);
        assert_eq!(evt.conditions_failed, vec![1]);
        assert_eq!(evt.indicator_snapshot, snap);
        assert_eq!(evt.schema_version, FilterEventV1::SCHEMA_VERSION);
    }

    #[test]
    fn from_outcome_trip_sets_triggered_true_and_no_suppression() {
        let outcome = FilterEvalOutcome {
            decision: ActivationDecision::Active {
                transition: Transition::Trip,
            },
            conditions_passed: vec![ConditionResult { passed: true }],
            tree_true: true,
        };
        let evt = FilterEventV1::from_outcome(fid(), ts(0), &outcome, BTreeMap::new());
        assert!(evt.triggered);
        assert_eq!(evt.suppressed_reason, None);
    }
}

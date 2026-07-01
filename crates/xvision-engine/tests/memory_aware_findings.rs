//! Tests for the memory-aware findings extractor.
//!
//! Verifies the contract acceptance criteria:
//!
//! 1. A run with a known bad-outcome decision driven by a known stale
//!    memory item emits exactly one `warning` finding with the expected
//!    kind + body naming the item id.
//! 2. Good outcomes emit nothing by default (avoids positive-noise);
//!    one `info` finding when the operator opts in.
//! 3. Finding body names the memory item id(s) responsible.
//!
//! These tests exercise the pure detector directly with seeded
//! `DecisionRow` + `MemoryRecallEvent` fixtures. The detector is
//! independent of the SQL projection (which lives in the dashboard
//! handler and is covered by its own tests), so the unit-level
//! coverage here is the load-bearing surface for the contract.

use chrono::Utc;

use xvision_engine::eval::findings::memory::{
    detect_memory_aware_findings, MemoryFindingOptions, KIND_MEMORY_BAD_OUTCOME, KIND_MEMORY_GOOD_OUTCOME,
    PRODUCED_BY_MEMORY,
};
use xvision_engine::eval::findings::Severity;
use xvision_engine::eval::store::DecisionRow;
use xvision_observability::events::{MemoryRecallEvent, MemoryRecallItem};

fn bad_decision(run_id: &str, idx: u32) -> DecisionRow {
    DecisionRow {
        run_id: run_id.to_string(),
        decision_index: idx,
        timestamp: Utc::now(),
        asset: "BTC/USDT".to_string(),
        // Bad outcome scenario: agent went long, position closed at a loss.
        action: "long_open".to_string(),
        conviction: Some(0.78),
        justification: Some("Prior pattern says RSI cross at this level usually fades.".to_string()),
        reasoning: None,
        order_size: Some(1.0),
        fill_price: Some(50_000.0),
        fill_size: Some(1.0),
        fee: Some(0.5),
        // Negative realized pnl → judge_outcome -> Bad.
        pnl_realized: Some(-25.0),
        delayed: None,
    }
}

fn good_decision(run_id: &str, idx: u32) -> DecisionRow {
    DecisionRow {
        run_id: run_id.to_string(),
        decision_index: idx,
        timestamp: Utc::now(),
        asset: "BTC/USDT".to_string(),
        action: "long_open".to_string(),
        conviction: Some(0.62),
        justification: Some("Pattern matched; opened long.".to_string()),
        reasoning: None,
        order_size: Some(1.0),
        fill_price: Some(50_000.0),
        fill_size: Some(1.0),
        fee: Some(0.5),
        pnl_realized: Some(40.0),
        delayed: None,
    }
}

fn recall_event(
    run_id: &str,
    decision_id: i64,
    namespace: &str,
    items: &[(&str, &str)],
) -> MemoryRecallEvent {
    MemoryRecallEvent {
        run_id: run_id.to_string(),
        flywheel_cycle_id: None,
        decision_id,
        namespace: namespace.to_string(),
        items: items
            .iter()
            .map(|(id, preview)| MemoryRecallItem {
                id: (*id).to_string(),
                score: 0.85,
                text_preview: (*preview).to_string(),
            })
            .collect(),
    }
}

// ── Acceptance #1: bad outcome + stale recall → one warning ──────────────────

#[test]
fn bad_outcome_with_stale_recall_emits_one_warning_naming_item_id() {
    let run_id = "run-mem-bad";
    let decisions = vec![bad_decision(run_id, 4)];

    // Known stale memory item that was recalled into this decision.
    let recalls = vec![recall_event(
        run_id,
        4,
        "agent:01HZTRADER",
        &[(
            "stale-pattern-01",
            "RSI cross at 30 → reversal (last seen 2024-08)",
        )],
    )];

    let findings = detect_memory_aware_findings(&decisions, &recalls, MemoryFindingOptions::default());

    assert_eq!(
        findings.len(),
        1,
        "expected exactly 1 warning finding for the bad-outcome decision, got: {:?}",
        findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
    );

    let f = &findings[0];
    assert_eq!(f.kind, KIND_MEMORY_BAD_OUTCOME);
    assert_eq!(f.severity, Severity::Warning);
    assert_eq!(f.run_id, run_id);
    assert_eq!(f.produced_by_check.as_deref(), Some(PRODUCED_BY_MEMORY));

    // Body must name the stale memory item id so an operator can trace
    // the finding back to the responsible pattern.
    assert!(
        f.summary.contains("stale-pattern-01"),
        "summary must name the recalled memory item id: {}",
        f.summary
    );
    assert!(
        f.summary.contains("Decision 4"),
        "summary must name the decision index: {}",
        f.summary
    );

    // Structured evidence must carry the item id list for programmatic
    // consumers (deep-link UI, downstream pipelines).
    let ids = f
        .evidence
        .get("memory_item_ids")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].as_str(), Some("stale-pattern-01"));
}

// ── Acceptance #2: good outcome — opt-in info, default off ───────────────────

#[test]
fn good_outcome_driven_by_memory_emits_nothing_when_opt_in_is_off() {
    let run_id = "run-mem-good-default";
    let decisions = vec![good_decision(run_id, 1)];
    let recalls = vec![recall_event(
        run_id,
        1,
        "global",
        &[("helpful-pattern-01", "after consolidation, breakout")],
    )];

    // Default options — emit_good_outcomes is false. Expect zero findings.
    let findings = detect_memory_aware_findings(&decisions, &recalls, MemoryFindingOptions::default());
    assert!(
        findings.is_empty(),
        "good outcomes must not emit findings under default options, got: {:?}",
        findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
    );
}

#[test]
fn good_outcome_driven_by_memory_emits_info_when_opt_in_is_on() {
    let run_id = "run-mem-good-optin";
    let decisions = vec![good_decision(run_id, 1)];
    let recalls = vec![recall_event(
        run_id,
        1,
        "global",
        &[("helpful-pattern-01", "after consolidation, breakout")],
    )];

    let findings = detect_memory_aware_findings(
        &decisions,
        &recalls,
        MemoryFindingOptions {
            emit_good_outcomes: true,
        },
    );

    assert_eq!(
        findings.len(),
        1,
        "expected one info finding when good-outcome opt-in is on, got: {:?}",
        findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
    );
    let f = &findings[0];
    assert_eq!(f.kind, KIND_MEMORY_GOOD_OUTCOME);
    assert_eq!(f.severity, Severity::Info);
    assert!(
        f.summary.contains("helpful-pattern-01"),
        "info body must name the memory item id: {}",
        f.summary
    );
}

// ── Negative-space coverage ───────────────────────────────────────────────────

#[test]
fn run_with_bad_outcome_but_no_recall_emits_nothing() {
    // No memory items were recalled into the decision — we can't blame
    // any pattern. Finding must NOT fire.
    let run_id = "run-no-recall";
    let decisions = vec![bad_decision(run_id, 1)];
    let findings = detect_memory_aware_findings(&decisions, &[], MemoryFindingOptions::default());
    assert!(
        findings.is_empty(),
        "no recall events means nothing to attribute, got: {:?}",
        findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
    );
}

#[test]
fn mixed_run_emits_only_for_bad_outcomes_when_opt_in_off() {
    // Realistic run shape: a bad decision with a recalled stale pattern
    // plus a good decision with a recalled helpful pattern plus an
    // inconclusive (no-fill) decision. Under default options, expect
    // exactly one warning for the bad one.
    let run_id = "run-mixed";
    let mut decisions = vec![bad_decision(run_id, 1)];
    decisions.push(good_decision(run_id, 2));
    // Inconclusive: pnl is None.
    decisions.push(DecisionRow {
        run_id: run_id.to_string(),
        decision_index: 3,
        timestamp: Utc::now(),
        asset: "BTC/USDT".to_string(),
        action: "flat".to_string(),
        conviction: Some(0.0),
        justification: Some("no-op".to_string()),
        reasoning: None,
        order_size: None,
        fill_price: None,
        fill_size: None,
        fee: None,
        pnl_realized: None,
        delayed: None,
    });

    let recalls = vec![
        recall_event(run_id, 1, "agent:t", &[("stale-A", "old")]),
        recall_event(run_id, 2, "agent:t", &[("good-B", "fresh")]),
        recall_event(run_id, 3, "agent:t", &[("inconclusive-C", "noop")]),
    ];

    let findings = detect_memory_aware_findings(&decisions, &recalls, MemoryFindingOptions::default());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, KIND_MEMORY_BAD_OUTCOME);
    assert!(findings[0].summary.contains("stale-A"));
}

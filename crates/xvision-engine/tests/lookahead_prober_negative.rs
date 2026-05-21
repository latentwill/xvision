//! Negative cases for the `lookahead_suspected` finding kind.
//!
//! Tests that non-lookahead scenarios produce the expected `Finding` shape
//! (e.g. `pass_2_action = null` when Pass 2 returns None).
//!
//! Two-pass prober algorithm tests live in `xvision-eval/src/prober/lookahead.rs`.
//! These engine-level tests focus on the `Finding` contract as consumed by
//! downstream (CLI, dashboard, findings extractor).

use xvision_engine::eval::findings::{Finding, KIND_LOOKAHEAD_SUSPECTED};

/// When `pass_2_action` is None (Pass 2 didn't fire), the finding records
/// the situation but should NOT be flagged as an active lookahead.
///
/// Note: `Finding::lookahead_suspected` is always created when the PROBER
/// decides to emit — the finding's `is_lookahead()` semantics live on
/// `LookaheadFinding` in xvision-eval. At the `Finding` level, the presence
/// of the finding IS the prober's verdict. `pass_2_action = null` means
/// "Pass 2 didn't fire" which still indicates a potential issue (the signal
/// fired in Pass 1 but not Pass 2, with no evidence of future-bar access —
/// likely a warmup state artifact rather than true lookahead). The prober
/// only creates this finding when Pass 2 produced the SAME action.
///
/// This test documents that the constructor handles `pass_2_action = None`
/// without panicking.
#[test]
fn finding_with_pass2_none_is_valid() {
    let f = Finding::lookahead_suspected("run_01", "cycle_negative", None, "buy", None, 5);
    assert_eq!(f.kind, KIND_LOOKAHEAD_SUSPECTED);
    assert!(
        f.evidence["pass_2_action"].is_null(),
        "pass_2_action must be null"
    );
    assert_eq!(f.evidence["pass_1_action"], "buy");
}

/// Two findings for different cycle_ids are independent (no shared state).
#[test]
fn two_findings_are_independent() {
    let f1 = Finding::lookahead_suspected("run_01", "cycle_A", None, "buy", Some("buy"), 0);
    let f2 = Finding::lookahead_suspected("run_01", "cycle_B", None, "sell", Some("sell"), 1);
    assert_ne!(f1.id, f2.id);
    assert_ne!(f1.evidence["cycle_id"], f2.evidence["cycle_id"]);
    assert_ne!(f1.evidence["pass_1_action"], f2.evidence["pass_1_action"]);
}

/// `eval_review_id`, `review_type`, `confidence` default to None
/// (legacy/review fields not populated by the prober).
#[test]
fn review_fields_are_none() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", Some("buy"), 0);
    assert!(f.eval_review_id.is_none());
    assert!(f.review_type.is_none());
    assert!(f.confidence.is_none());
}

/// Summary string contains the cycle_id and snapshot index.
#[test]
fn summary_contains_cycle_and_snapshot_info() {
    let f = Finding::lookahead_suspected("run_01", "cycle_XYZ", None, "buy", Some("buy"), 99);
    assert!(
        f.summary.contains("cycle_XYZ"),
        "summary must mention the cycle_id"
    );
    assert!(
        f.summary.contains("99"),
        "summary must mention the snapshot index"
    );
}

/// KIND_LOOKAHEAD_SUSPECTED constant is stable (regression guard).
#[test]
fn kind_constant_is_stable() {
    assert_eq!(
        xvision_engine::eval::findings::KIND_LOOKAHEAD_SUSPECTED,
        "lookahead_suspected"
    );
}

/// PRODUCED_BY_LOOKAHEAD_PROBER constant is stable (regression guard).
#[test]
fn produced_by_constant_is_stable() {
    assert_eq!(
        xvision_engine::eval::findings::PRODUCED_BY_LOOKAHEAD_PROBER,
        "prober:lookahead"
    );
}

/// A Finding with `kind = "lookahead_suspected"` deserializes back into the
/// correct kind (open-enum compatibility).
#[test]
fn lookahead_kind_survives_open_enum_serde() {
    let json = r#"{
        "id": "01JTEST0000000000000000000",
        "run_id": "run_01",
        "kind": "lookahead_suspected",
        "severity": "critical",
        "summary": "Test lookahead finding",
        "evidence": {},
        "extracted_at": "2026-05-21T00:00:00Z",
        "schema_version": "1"
    }"#;
    let f: Finding = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(f.kind, "lookahead_suspected");
}

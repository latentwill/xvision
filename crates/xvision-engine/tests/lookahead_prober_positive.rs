//! Positive case: `Finding::lookahead_suspected` produces correct structure
//! and the `KIND_LOOKAHEAD_SUSPECTED` kind is correctly registered.
//!
//! Note: the two-pass prober logic itself is tested in `xvision-eval`'s own
//! unit tests (`src/prober/lookahead.rs`). These engine-level tests verify the
//! `Finding` constructor and the kind/severity/evidence contract that the CLI
//! (`xvn eval probe-lookahead`) depends on.

use xvision_engine::eval::findings::{
    Finding, Severity, KIND_LOOKAHEAD_SUSPECTED, PRODUCED_BY_LOOKAHEAD_PROBER,
};

// ---------------------------------------------------------------------------
// Finding::lookahead_suspected constructor
// ---------------------------------------------------------------------------

/// Positive: `lookahead_suspected` finding is Critical severity.
#[test]
fn lookahead_finding_is_critical() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", Some("my_indicator"), "buy", Some("buy"), 0);
    assert_eq!(f.severity, Severity::Critical);
}

/// Positive: `kind` field is `"lookahead_suspected"`.
#[test]
fn lookahead_finding_kind_is_correct() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", Some("buy"), 0);
    assert_eq!(f.kind, KIND_LOOKAHEAD_SUSPECTED);
    assert_eq!(f.kind, "lookahead_suspected");
}

/// Positive: `run_id` is propagated.
#[test]
fn lookahead_finding_run_id_propagated() {
    let f = Finding::lookahead_suspected("run_fixture_99", "cycle_x", None, "sell", None, 3);
    assert_eq!(f.run_id, "run_fixture_99");
}

/// Positive: evidence blob carries required fields, plus typed Finding
/// fields carry the V2E trace-surface foundation values
/// (produced_by_check + evidence_cycle_ids).
#[test]
fn lookahead_finding_evidence_has_required_fields() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", Some("my_indicator"), "buy", Some("buy"), 7);
    let ev = &f.evidence;
    assert_eq!(ev["cycle_id"], "cycle_abc");
    assert_eq!(ev["indicator_name"], "my_indicator");
    assert_eq!(ev["pass_1_action"], "buy");
    assert_eq!(ev["pass_2_action"], "buy");
    assert_eq!(ev["snapshot_index"], 7);
    assert_eq!(f.produced_by_check.as_deref(), Some(PRODUCED_BY_LOOKAHEAD_PROBER));
    assert_eq!(f.produced_by_check.as_deref(), Some("prober:lookahead"));
}

/// Positive: typed `evidence_cycle_ids` field carries `[cycle_id]`.
#[test]
fn lookahead_finding_evidence_cycle_ids_is_stub() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", Some("buy"), 0);
    let ids = f.evidence_cycle_ids.as_ref().expect("evidence_cycle_ids must be Some");
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0], "cycle_abc");
}

/// Positive: when `indicator_name` is None, evidence has JSON null.
#[test]
fn lookahead_finding_indicator_name_none_is_null() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", Some("buy"), 0);
    assert!(f.evidence["indicator_name"].is_null());
}

/// Positive: when `pass_2_action` is None, evidence has JSON null.
#[test]
fn lookahead_finding_pass2_none_is_null() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", None, 0);
    assert!(f.evidence["pass_2_action"].is_null());
}

/// Positive: ULID id is non-empty.
#[test]
fn lookahead_finding_id_is_nonempty_ulid() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", Some("buy"), 0);
    assert!(!f.id.is_empty());
    assert_eq!(f.id.len(), 26, "ULID strings are 26 characters");
}

/// Positive: two calls produce distinct ids (ULID monotonicity).
#[test]
fn lookahead_finding_distinct_ids_per_call() {
    let f1 = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", Some("buy"), 0);
    let f2 = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", Some("buy"), 0);
    assert_ne!(f1.id, f2.id, "each finding must have a distinct ULID");
}

/// Positive: `title` and `description` and `recommendation` are set.
#[test]
fn lookahead_finding_optional_fields_are_set() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", Some("buy"), 0);
    assert!(f.title.is_some(), "title must be set");
    assert!(f.description.is_some(), "description must be set");
    assert!(f.recommendation.is_some(), "recommendation must be set");
    assert!(f.created_at.is_some(), "created_at must be set");
}

/// Positive: `schema_version` matches the current FINDING_SCHEMA_VERSION
/// from the trace-surface foundation.
#[test]
fn lookahead_finding_schema_version() {
    let f = Finding::lookahead_suspected("run_01", "cycle_abc", None, "buy", Some("buy"), 0);
    assert_eq!(f.schema_version, xvision_engine::eval::findings::FINDING_SCHEMA_VERSION);
}

/// Positive: round-trip through JSON preserves all fields.
#[test]
fn lookahead_finding_roundtrips_json() {
    let f = Finding::lookahead_suspected(
        "run_roundtrip",
        "cycle_roundtrip",
        Some("rsi_14"),
        "sell",
        Some("sell"),
        12,
    );
    let json = serde_json::to_string(&f).expect("must serialize");
    let back: Finding = serde_json::from_str(&json).expect("must deserialize");
    assert_eq!(f, back);
}

/// Positive: sell action propagates correctly.
#[test]
fn lookahead_finding_sell_action() {
    let f = Finding::lookahead_suspected("run_01", "c", None, "sell", Some("sell"), 1);
    assert_eq!(f.evidence["pass_1_action"], "sell");
    assert_eq!(f.evidence["pass_2_action"], "sell");
}

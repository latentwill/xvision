//! V2E eval-cost-model-per-bar-and-volume-share — VolumeShare model tests.
//!
//! Tests:
//! - Cap binding emits `volume_share_excess` finding once per binding cycle.

use xvision_engine::eval::findings::make_volume_share_excess_finding;

// ── Finding construction ───────────────────────────────────────────────────

#[test]
fn volume_share_excess_finding_has_correct_kind() {
    let finding = make_volume_share_excess_finding(
        "run-01TEST",
        42,      // cycle_id (decision_idx)
        30.0,    // requested_qty
        1_000.0, // bar_volume
        25.0,    // cap_binding_qty
        0.025,   // fill_share
    );
    assert_eq!(finding.kind, "volume_share_excess");
    assert!(!finding.id.is_empty());
    assert_eq!(finding.run_id, "run-01TEST");
}

#[test]
fn volume_share_excess_finding_evidence_fields() {
    let finding = make_volume_share_excess_finding("run-02", 7, 50.0, 2_000.0, 50.0, 0.025);
    let evidence = &finding.evidence;
    assert_eq!(
        evidence["requested_qty"].as_f64().unwrap(),
        50.0,
        "requested_qty mismatch"
    );
    assert_eq!(
        evidence["bar_volume"].as_f64().unwrap(),
        2_000.0,
        "bar_volume mismatch"
    );
    // produced_by_check and evidence_cycle_ids live on the typed Finding fields
    // (V2E trace-surface foundation), not embedded in the evidence blob.
    assert_eq!(finding.produced_by_check.as_deref(), Some("sim:volume_cap"));
    let cycle_ids = finding.evidence_cycle_ids.as_ref().unwrap();
    assert_eq!(cycle_ids.len(), 1);
    assert_eq!(cycle_ids[0], "7");
}

#[test]
fn volume_share_excess_finding_serde_round_trip() {
    let finding = make_volume_share_excess_finding("run-03", 1, 10.0, 500.0, 12.5, 0.025);
    let json = serde_json::to_string(&finding).unwrap();
    let back: xvision_engine::eval::Finding = serde_json::from_str(&json).unwrap();
    assert_eq!(back.kind, "volume_share_excess");
    assert_eq!(back.run_id, finding.run_id);
}

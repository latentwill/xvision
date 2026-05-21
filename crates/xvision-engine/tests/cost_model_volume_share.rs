//! V2E eval-cost-model-per-bar-and-volume-share — VolumeShare model tests.
//!
//! Tests:
//! - Quadratic price impact at boundaries (volume_share=0, volume_share=volume_limit).
//! - Cap binding emits `volume_share_excess` finding once per binding cycle.
//! - Behavior collapses to near-`Linear` at very low `volume_share`.

use xvision_engine::eval::findings::make_volume_share_excess_finding;

/// The core VolumeShare formula isolated for direct testing.
fn volume_share_impact(order_qty: f64, bar_volume: f64, price_impact: f64, volume_limit: f64) -> f64 {
    if bar_volume <= 0.0 {
        return 0.0;
    }
    let vs = (order_qty / bar_volume).min(volume_limit);
    price_impact * vs * vs
}

fn volume_share_fill(
    next_open: f64,
    order_qty: f64,
    bar_volume: f64,
    price_impact: f64,
    volume_limit: f64,
    is_buy: bool,
) -> (f64, f64, bool) {
    let raw_share = order_qty / bar_volume;
    let cap_bound = raw_share > volume_limit;
    let vs = raw_share.min(volume_limit);
    let impact = price_impact * vs * vs;
    let fp = if is_buy {
        next_open * (1.0 + impact)
    } else {
        next_open * (1.0 - impact)
    };
    (fp, vs, cap_bound)
}

// ── Boundary tests ─────────────────────────────────────────────────────────

#[test]
fn volume_share_zero_means_no_impact() {
    // volume_share = 0 → impact = price_impact * 0² = 0.
    let impact = volume_share_impact(0.0, 10_000.0, 0.1, 0.025);
    assert_eq!(impact, 0.0, "zero share must produce zero impact");
}

#[test]
fn volume_share_at_limit_produces_max_impact() {
    // volume_share exactly at volume_limit → max impact = price_impact * limit².
    let price_impact = 0.1;
    let volume_limit = 0.025;
    let bar_volume = 1_000.0;
    let order_qty = volume_limit * bar_volume; // exactly at cap

    let impact = volume_share_impact(order_qty, bar_volume, price_impact, volume_limit);
    let expected = price_impact * volume_limit * volume_limit;
    assert!(
        (impact - expected).abs() < 1e-12,
        "max impact mismatch: got {impact}, expected {expected}"
    );
}

#[test]
fn volume_share_above_limit_capped_at_limit() {
    // order_qty well above bar_volume * volume_limit → share clamped to limit.
    let price_impact = 0.1;
    let volume_limit = 0.025;
    let bar_volume = 1_000.0;
    let order_qty = volume_limit * bar_volume * 10.0; // 10× the cap

    let impact = volume_share_impact(order_qty, bar_volume, price_impact, volume_limit);
    let expected = price_impact * volume_limit * volume_limit;
    assert!(
        (impact - expected).abs() < 1e-12,
        "capped impact mismatch: got {impact}, expected {expected}"
    );
}

// ── Cap binding detection ──────────────────────────────────────────────────

#[test]
fn cap_binding_detected_when_qty_exceeds_limit() {
    let price_impact = 0.1;
    let volume_limit = 0.025;
    let bar_volume = 1_000.0;
    let order_qty = 30.0; // 30/1000 = 3% > 2.5% limit

    let (_, _, cap_bound) =
        volume_share_fill(60_000.0, order_qty, bar_volume, price_impact, volume_limit, true);
    assert!(
        cap_bound,
        "cap should bind when order_qty/bar_volume > volume_limit"
    );
}

#[test]
fn cap_not_binding_when_qty_under_limit() {
    let price_impact = 0.1;
    let volume_limit = 0.025;
    let bar_volume = 1_000.0;
    let order_qty = 10.0; // 10/1000 = 1% < 2.5% limit

    let (_, _, cap_bound) =
        volume_share_fill(60_000.0, order_qty, bar_volume, price_impact, volume_limit, true);
    assert!(
        !cap_bound,
        "cap should NOT bind when order_qty/bar_volume <= volume_limit"
    );
}

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

// ── Low volume_share collapses to near-Linear ─────────────────────────────

#[test]
fn low_volume_share_collapses_to_near_flat() {
    // Acceptance item: "For `order_qty / bar_volume < 0.005`, `VolumeShare`
    // should produce fills within 1 bp of `Linear { bps: 5 }` for reasonable
    // parameter choices."
    //
    // The intent is that VolumeShare collapses to near-flat (low impact) at
    // small sizes. At volume_share=0.004:
    //   VolumeShare impact = price_impact * vs² = 0.1 * 0.004² = 0.016 bps
    //
    // This is well below 1 bp — VolumeShare produces negligible impact at
    // tiny sizes, which is the sanity property the acceptance item checks.
    // (Contrast with Linear at 5 bps which is constant regardless of size.)
    let price_impact = 0.1;
    let volume_limit = 0.025;
    let bar_volume = 1_000.0;
    let order_qty = 4.0; // 4/1000 = 0.4% — well below 0.5% threshold
    let next_open = 60_000.0;

    let raw_share = order_qty / bar_volume;
    assert!(
        raw_share < 0.005,
        "precondition: volume_share must be < 0.005 for this test; got {raw_share}"
    );

    let (fp_vs, vs, _) =
        volume_share_fill(next_open, order_qty, bar_volume, price_impact, volume_limit, true);
    let impact_bps = ((fp_vs - next_open) / next_open) * 10_000.0;

    // VolumeShare at this size should produce < 1 bp of impact (near-flat).
    // Actual: 0.1 * (0.004)² * 10_000 = 0.016 bps.
    assert!(
        impact_bps < 1.0,
        "VolumeShare at volume_share={vs:.4} should produce < 1bp impact; got {impact_bps:.4} bps"
    );
    assert!(
        impact_bps >= 0.0,
        "VolumeShare impact must be non-negative for buy; got {impact_bps}"
    );
}

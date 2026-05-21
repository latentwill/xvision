//! CLI smoke test for `xvn eval probe-lookahead`.
//!
//! Verifies that the `Finding::lookahead_suspected` constructor and constants
//! work end-to-end as the CLI would use them (converting `LookaheadFinding`
//! from `xvision-eval` into engine `Finding` objects for output).
//!
//! Parser-level smoke tests (Clap argument parsing) live in the CLI crate's
//! eval subcommand module (`crates/xvision-cli/src/commands/eval/mod.rs`).

use xvision_engine::eval::findings::{Finding, KIND_LOOKAHEAD_SUSPECTED, PRODUCED_BY_LOOKAHEAD_PROBER};

/// Smoke test: construct a batch of `lookahead_suspected` findings as the
/// CLI would after running the prober, and verify the output shape.
#[test]
fn cli_probe_output_shape_smoke() {
    let run_id = "01JTEST0000000000FIXTURE";

    // Simulate 3 probe findings (as if the prober emitted them for 3 bars)
    let cycle_ids = [
        "cycle_01JTEST00000000000A",
        "cycle_01JTEST00000000000B",
        "cycle_01JTEST00000000000C",
    ];

    let findings: Vec<Finding> = cycle_ids
        .iter()
        .enumerate()
        .map(|(i, &cid)| {
            Finding::lookahead_suspected(run_id, cid, Some("future_peek"), "buy", Some("buy"), i)
        })
        .collect();

    assert_eq!(findings.len(), 3, "must have 3 findings");

    // Verify each finding matches the expected shape. produced_by_check and
    // evidence_cycle_ids are typed Finding fields (V2E trace-surface
    // foundation), not embedded in evidence.
    for (i, f) in findings.iter().enumerate() {
        assert_eq!(f.run_id, run_id);
        assert_eq!(f.kind, KIND_LOOKAHEAD_SUSPECTED);
        assert_eq!(f.produced_by_check.as_deref(), Some(PRODUCED_BY_LOOKAHEAD_PROBER));
        assert_eq!(f.evidence["indicator_name"], "future_peek");
        assert_eq!(f.evidence["pass_1_action"], "buy");
        assert_eq!(f.evidence["pass_2_action"], "buy");
        assert_eq!(f.evidence["snapshot_index"], i);
    }

    // Simulate CLI JSON output (serialize all findings)
    let json_output = serde_json::to_string_pretty(&findings).expect("must serialize");
    assert!(!json_output.is_empty());
    assert!(json_output.contains(KIND_LOOKAHEAD_SUSPECTED));
    assert!(json_output.contains(PRODUCED_BY_LOOKAHEAD_PROBER));

    // Simulate CLI human-readable output (count + per-finding summary)
    let human_header = format!("probe-lookahead: {} finding(s)", findings.len());
    assert!(human_header.contains("3"));

    for f in &findings {
        let snap_idx = f.evidence["snapshot_index"].as_u64().unwrap_or(0);
        let line = format!("[{}] snapshot={} {}", f.severity.as_str(), snap_idx, f.summary);
        assert!(line.contains("critical"));
        assert!(line.contains("snapshot="));
    }
}

/// Smoke test: empty findings list produces a valid JSON `[]` output.
#[test]
fn cli_probe_empty_findings_json_output() {
    let empty: Vec<Finding> = vec![];
    let json = serde_json::to_string(&empty).expect("must serialize");
    assert_eq!(json, "[]");
}

/// Smoke test: a single finding can be pretty-printed.
#[test]
fn cli_probe_single_finding_pretty_print() {
    let f = Finding::lookahead_suspected(
        "run_smoke",
        "cycle_smoke",
        Some("rsi_14"),
        "sell",
        Some("sell"),
        5,
    );
    let pretty = serde_json::to_string_pretty(&f).expect("must serialize");
    // Verify key fields appear in the pretty-print output
    assert!(pretty.contains("lookahead_suspected"));
    assert!(pretty.contains("prober:lookahead"));
    assert!(pretty.contains("rsi_14"));
    assert!(pretty.contains("sell"));
}

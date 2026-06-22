//! Tests for `xvn scenario inspect <id> --card`.
//!
//! Uses the binary-invocation pattern from scenario_cli.rs so we exercise the
//! full dispatch path against an isolated `$XVN_HOME`.

use std::process::Command;

use tempfile::tempdir;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

/// Happy-path: inspect a canonical scenario by id with --card and assert that
/// each expected card field appears in the output.
#[test]
fn scenario_inspect_card_prints_expected_fields() {
    let dir = tempdir().unwrap();

    // First create a scenario so we have a known id with a parent (clone it).
    let create = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "SOL 4h inspect test",
            "--from",
            "2025-01-01",
            "--to",
            "2025-01-09",
            "--warmup-bars",
            "200",
            "--json",
        ],
        dir.path(),
    );
    assert!(
        create.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = body["id"].as_str().unwrap();

    // Inspect the new scenario.
    let out = xvn(&["scenario", "inspect", id, "--card"], dir.path());
    assert!(
        out.status.success(),
        "inspect failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let card = String::from_utf8_lossy(&out.stdout);

    // Verify required card fields are present.
    assert!(
        card.contains(&format!("id: {id}")),
        "card must contain scenario id; got:\n{card}"
    );
    assert!(
        card.contains("name: SOL 4h inspect test"),
        "card must contain display name; got:\n{card}"
    );
    // Scenarios are asset-free; the inspect card no longer renders an asset.
    assert!(
        !card.contains("asset:"),
        "card must not contain an asset field (scenarios are asset-free); got:\n{card}"
    );
    assert!(
        card.contains("date_window: 2025-01-01..2025-01-09"),
        "card must contain date_window; got:\n{card}"
    );
    assert!(
        card.contains("warmup_bars: 200"),
        "card must contain warmup_bars; got:\n{card}"
    );
    // No parent — source line should be absent.
    assert!(
        !card.contains("source:"),
        "root scenario must not emit source line; got:\n{card}"
    );
    // previous_runs block must be present (either count or unavailable).
    assert!(
        card.contains("previous_runs"),
        "card must contain previous_runs; got:\n{card}"
    );
}

/// Clone a scenario and verify `source: cloned_from <parent_id>` appears.
#[test]
fn scenario_inspect_card_clone_shows_source() {
    let dir = tempdir().unwrap();

    let create = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "SOL parent for inspect",
            "--from",
            "2025-01-01",
            "--to",
            "2025-01-09",
            "--json",
        ],
        dir.path(),
    );
    assert!(create.status.success());
    let parent_body: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let parent_id = parent_body["id"].as_str().unwrap();

    let clone_out = xvn(
        &["scenario", "clone", parent_id, "--name", "SOL clone for inspect"],
        dir.path(),
    );
    assert!(
        clone_out.status.success(),
        "clone failed: {}",
        String::from_utf8_lossy(&clone_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&clone_out.stdout);
    // stdout: "cloned to <id> (parent: ...)"
    let child_id = stdout
        .split_whitespace()
        .nth(2)
        .expect("expected 'cloned to <id> ...' shape");

    let out = xvn(&["scenario", "inspect", child_id, "--card"], dir.path());
    assert!(
        out.status.success(),
        "inspect failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let card = String::from_utf8_lossy(&out.stdout);

    assert!(
        card.contains(&format!("source: cloned_from {parent_id}")),
        "clone card must show source; got:\n{card}"
    );
}

/// Without --card the command must fail with a usage error.
#[test]
fn scenario_inspect_without_card_flag_is_usage_error() {
    let dir = tempdir().unwrap();

    let create = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "SOL no-card test",
            "--from",
            "2025-01-01",
            "--to",
            "2025-01-09",
            "--json",
        ],
        dir.path(),
    );
    assert!(create.status.success());
    let body: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = body["id"].as_str().unwrap();

    let out = xvn(&["scenario", "inspect", id], dir.path());
    assert!(
        !out.status.success(),
        "expected failure without --card; got success"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--card"),
        "error must mention --card; got:\n{stderr}"
    );
}

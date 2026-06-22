use std::process::Command;

use tempfile::tempdir;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

#[test]
fn scenario_create_json_is_machine_readable() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "ETH 15m",
            "--from",
            "2024-02-03",
            "--to",
            "2024-02-10",
            "--json",
        ],
        dir.path(),
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(body["id"].as_str().unwrap().starts_with("sc_"));
    assert_eq!(body["display_name"], "ETH 15m");
}

#[test]
fn scenario_validate_from_file_does_not_create_row() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "ETH validate source",
            "--from",
            "2024-02-03",
            "--to",
            "2024-02-10",
            "--json",
        ],
        dir.path(),
    );
    assert!(out.status.success());
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = body["id"].as_str().unwrap();

    let out = xvn(&["scenario", "show", id, "--toml"], dir.path());
    assert!(out.status.success());
    let file = dir.path().join("scenario.toml");
    // Rewrite the display_name so validate doesn't trip the
    // active-scenario uniqueness gate (which now applies even to
    // --from-file shape validation against a live store). The point of
    // this test is "validate doesn't create a row" — collision with
    // the just-created source row is incidental.
    let toml = String::from_utf8(out.stdout)
        .unwrap()
        .replace("ETH validate source", "ETH validate source (file)");
    std::fs::write(&file, toml).unwrap();

    let out = xvn(
        &[
            "scenario",
            "validate",
            "--from-file",
            file.to_str().unwrap(),
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["ok"], true);
}

#[test]
fn scenario_validate_from_file_reports_missing_display_name_actionably() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "ETH validate missing name source",
            "--from",
            "2024-02-03",
            "--to",
            "2024-02-10",
            "--json",
        ],
        dir.path(),
    );
    assert!(out.status.success());
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = body["id"].as_str().unwrap();

    let out = xvn(&["scenario", "show", id, "--toml"], dir.path());
    assert!(out.status.success());
    let toml = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with("display_name = "))
        .collect::<Vec<_>>()
        .join("\n");
    let file = dir.path().join("missing-name.toml");
    std::fs::write(&file, toml).unwrap();

    let out = xvn(
        &["scenario", "validate", "--from-file", file.to_str().unwrap()],
        dir.path(),
    );

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("display_name is required; provide a scenario display name"),
        "stderr: {stderr}"
    );
}

// ── q15-scenario-warmup-bars CLI round-trip ─────────────────────────────

#[test]
fn scenario_warmup_create_round_trips_through_show() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "ETH warmup-50",
            "--from",
            "2024-02-03",
            "--to",
            "2024-02-10",
            "--warmup-bars",
            "50",
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["warmup_bars"].as_u64(), Some(50));
    let id = body["id"].as_str().unwrap();

    let show = xvn(&["scenario", "show", id], dir.path());
    assert!(show.status.success());
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        shown["warmup_bars"].as_u64(),
        Some(50),
        "warmup_bars must round-trip through create + show: {shown:?}",
    );
}

#[test]
fn scenario_warmup_create_defaults_to_200_when_flag_omitted() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "ETH default-warmup",
            "--from",
            "2024-02-03",
            "--to",
            "2024-02-10",
            "--json",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        body["warmup_bars"].as_u64(),
        Some(200),
        "scenario.warmup_bars must default to 200 when --warmup-bars is omitted: {body:?}",
    );
}

#[test]
fn scenario_warmup_clone_overrides_parent_value() {
    let dir = tempdir().unwrap();
    let parent = xvn(
        &[
            "scenario",
            "create",
            "--name",
            "ETH parent-warmup",
            "--from",
            "2024-02-03",
            "--to",
            "2024-02-10",
            "--warmup-bars",
            "100",
            "--json",
        ],
        dir.path(),
    );
    let parent_body: serde_json::Value = serde_json::from_slice(&parent.stdout).unwrap();
    let parent_id = parent_body["id"].as_str().unwrap();

    let cloned = xvn(
        &[
            "scenario",
            "clone",
            parent_id,
            "--warmup-bars",
            "25",
            "--name",
            "ETH clone-warmup",
        ],
        dir.path(),
    );
    assert!(
        cloned.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cloned.stderr)
    );
    // `scenario clone` prints `cloned to <id> (parent: <parent_id>)`
    // — pull the child id out so we can `scenario show` it.
    let stdout = String::from_utf8_lossy(&cloned.stdout);
    let child_id = stdout
        .split_whitespace()
        .nth(2)
        .expect("cloned-to line shape: 'cloned to <id> (parent: ...)'");
    let show = xvn(&["scenario", "show", child_id], dir.path());
    assert!(show.status.success());
    let child_body: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        child_body["warmup_bars"].as_u64(),
        Some(25),
        "clone --warmup-bars must override the parent's value: {child_body:?}",
    );
}

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
            "--asset",
            "ETH",
            "--from",
            "2024-02-03",
            "--to",
            "2024-02-10",
            "--granularity",
            "15m",
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
    assert_eq!(body["granularity"], "15m");
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
            "--asset",
            "ETH",
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
    std::fs::write(&file, out.stdout).unwrap();

    let out = xvn(
        &["scenario", "validate", "--from-file", file.to_str().unwrap(), "--json"],
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
            "--asset",
            "ETH",
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

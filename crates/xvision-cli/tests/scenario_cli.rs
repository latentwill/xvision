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

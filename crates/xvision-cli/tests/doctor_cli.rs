use std::process::Command;

use tempfile::tempdir;

#[test]
fn doctor_json_reports_effective_paths_and_templates() {
    let dir = tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["doctor", "--json"])
        .env("XVN_HOME", dir.path())
        .output()
        .expect("xvn doctor --json");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["xvn_home"], dir.path().display().to_string());
    assert!(body["db_path"].as_str().unwrap().ends_with("xvn.db"));
    assert!(body["templates"].as_array().unwrap().len() >= 3);
    assert_eq!(body["remote_target"], "local");
}

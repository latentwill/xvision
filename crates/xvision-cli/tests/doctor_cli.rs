use std::process::Command;

use tempfile::tempdir;

#[test]
fn doctor_json_reports_effective_paths_and_templates() {
    let dir = tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["doctor", "--json"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_REMOTE_URL")
        .output()
        .expect("xvn doctor --json");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["xvn_home"], dir.path().display().to_string());
    assert_eq!(
        body["db_path"].as_str().unwrap(),
        dir.path().join("xvn.db").display().to_string(),
    );
    // Post-2026-05-21 template-registry removal: doctor's `templates`
    // field is an empty array. Wire-format preserved so external
    // consumers keep parsing the JSON; the starter library lives at
    // `$XVN_HOME/strategies/library/` (via `xvn strategies init`).
    assert!(body["templates"].as_array().unwrap().is_empty());
    assert_eq!(body["remote_target"], "local");
}

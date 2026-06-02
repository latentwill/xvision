use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn xvn() -> Command {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
}

#[test]
fn marketplace_list_empty_home() {
    let dir = tempdir().unwrap();
    let out = xvn()
        .args(["marketplace", "list"])
        .env("XVN_HOME", dir.path())
        .env_remove("XVN_MARKETPLACE_FIXTURE")
        .env_remove("MARKETPLACE_DRIVER")
        .output()
        .expect("xvn marketplace list");

    assert!(
        out.status.success(),
        "exit={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("(no listings)"),
        "expected '(no listings)' in stdout: {stdout}"
    );
}

#[test]
fn marketplace_publish_with_mock() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("manifest.json");
    fs::write(
        &manifest_path,
        r#"{"agent_id":"01TEST","version":"1.0","display_name":"Test Strategy"}"#,
    )
    .unwrap();

    let out = xvn()
        .args([
            "marketplace",
            "publish",
            "--agent-id",
            "01TEST",
            "--price",
            "10.0",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
        ])
        .env("XVN_HOME", dir.path())
        .env_remove("MARKETPLACE_DRIVER")
        .output()
        .expect("xvn marketplace publish");

    assert!(
        out.status.success(),
        "exit={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("listing_id="),
        "expected listing_id in stdout: {stdout}"
    );
}

#[test]
fn marketplace_help_exits_zero() {
    let out = xvn()
        .args(["marketplace", "--help"])
        .output()
        .expect("xvn marketplace --help");

    assert!(
        out.status.success(),
        "exit={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("list") || stdout.contains("publish"),
        "expected subcommand list in help: {stdout}"
    );
}

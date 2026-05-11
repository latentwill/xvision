use std::process::Command;
use tempfile::tempdir;

const FIXTURE: &str = include_str!("../../xvision-skills/tests/fixtures/crypto-trader-base.md");

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

#[test]
fn new_ls_attach_roundtrip() {
    let dir = tempdir().unwrap();
    let skill_path = dir.path().join("crypto-trader-base.md");
    std::fs::write(&skill_path, FIXTURE).unwrap();

    // Register skill
    let out = xvn(
        &["skill", "new", "--from-file", skill_path.to_str().unwrap()],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let name = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert_eq!(name, "crypto-trader-base");

    // List
    let out = xvn(&["skill", "ls"], dir.path());
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout)
        .unwrap()
        .contains("crypto-trader-base"));

    // Create a strategy then attach
    let out = xvn(
        &[
            "strategy",
            "new",
            "--template",
            "mean_reversion",
            "--name",
            "skill-cli-test",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    let out = xvn(
        &[
            "skill",
            "attach",
            &id,
            "--slot",
            "trader",
            "--skill",
            "crypto-trader-base",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("attached"), "stdout: {stdout}");

    // Verify the bundle's trader prompt now contains the skill body
    let out = xvn(&["strategy", "show", &id], dir.path());
    assert!(out.status.success());
    let json = String::from_utf8(out.stdout).unwrap();
    assert!(json.contains("crypto trader"), "json: {json}");
}

#[test]
fn skill_new_rejects_missing_frontmatter() {
    let dir = tempdir().unwrap();
    let bad = dir.path().join("bad.md");
    std::fs::write(&bad, "no frontmatter here\n").unwrap();
    let out = xvn(
        &["skill", "new", "--from-file", bad.to_str().unwrap()],
        dir.path(),
    );
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap().to_lowercase();
    assert!(err.contains("frontmatter"), "stderr: {err}");
}

#[test]
fn skill_attach_unknown_slot_fails() {
    let dir = tempdir().unwrap();
    // Register skill + strategy first.
    let p = dir.path().join("crypto-trader-base.md");
    std::fs::write(&p, FIXTURE).unwrap();
    let out = xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    assert!(out.status.success());

    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        dir.path(),
    );
    assert!(out.status.success());
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    let out = xvn(
        &[
            "skill",
            "attach",
            &id,
            "--slot",
            "bogus",
            "--skill",
            "crypto-trader-base",
        ],
        dir.path(),
    );
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap().to_lowercase();
    assert!(err.contains("bogus"), "stderr: {err}");
}

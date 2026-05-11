use std::io::Write;
use std::process::{Command, Stdio};
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

/// Printing-Press follow-up: `xvn skill new --from-file -` reads stdin so
/// agents can pipe LLM output straight in without a tmpfile dance.
#[test]
fn skill_new_reads_from_stdin() {
    let dir = tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["skill", "new", "--from-file", "-"])
        .env("XVN_HOME", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn xvn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(FIXTURE.as_bytes())
        .expect("write fixture to stdin");
    let out = child.wait_with_output().expect("wait_with_output");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let name = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert_eq!(name, "crypto-trader-base");

    // Verify the skill was actually persisted (round-trip via ls).
    let out = xvn(&["skill", "ls"], dir.path());
    assert!(String::from_utf8(out.stdout)
        .unwrap()
        .contains("crypto-trader-base"));
}

/// Printing-Press follow-up: `xvn skill attach --dry-run` performs every
/// load + categorize + mutate step in memory but does NOT save the bundle
/// back. Surfaces the would-be diff as JSON so callers can preview the
/// change.
#[test]
fn skill_attach_dry_run_does_not_persist() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("crypto-trader-base.md");
    std::fs::write(&p, FIXTURE).unwrap();
    let out = xvn(
        &["skill", "new", "--from-file", p.to_str().unwrap()],
        dir.path(),
    );
    assert!(out.status.success());

    let out = xvn(
        &[
            "strategy",
            "new",
            "--template",
            "mean_reversion",
            "--name",
            "dry-run-test",
        ],
        dir.path(),
    );
    assert!(out.status.success());
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    // Snapshot the trader prompt BEFORE the dry-run.
    let before_show = xvn(&["strategy", "show", &id], dir.path());
    let before_json = String::from_utf8(before_show.stdout).unwrap();

    let out = xvn(
        &[
            "skill",
            "attach",
            &id,
            "--slot",
            "trader",
            "--skill",
            "crypto-trader-base",
            "--dry-run",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dry_stdout = String::from_utf8(out.stdout).unwrap();
    // Dry-run output is JSON containing dry_run + would_change.
    assert!(dry_stdout.contains("\"dry_run\""), "stdout: {dry_stdout}");
    assert!(dry_stdout.contains("\"would_change\""));
    assert!(dry_stdout.contains("\"before\""));
    assert!(dry_stdout.contains("\"after\""));
    // The skill body should appear in the after slot.
    assert!(dry_stdout.contains("crypto trader"));

    // Snapshot AFTER the dry-run — must be byte-identical to before.
    let after_show = xvn(&["strategy", "show", &id], dir.path());
    let after_json = String::from_utf8(after_show.stdout).unwrap();
    assert_eq!(
        before_json, after_json,
        "dry-run mutated the bundle on disk"
    );
}

/// Confirming the dry-run still surfaces the `Conflict` exit (7) when
/// the targeted slot is empty — same categorization as the real path.
#[test]
fn skill_attach_dry_run_still_returns_7_on_empty_slot() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("crypto-trader-base.md");
    std::fs::write(&p, FIXTURE).unwrap();
    xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        dir.path(),
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let out = xvn(
        &[
            "skill",
            "attach",
            &id,
            "--slot",
            "intern", // mean_reversion leaves intern empty
            "--skill",
            "crypto-trader-base",
            "--dry-run",
        ],
        dir.path(),
    );
    assert_eq!(
        out.status.code().expect("child exited cleanly"),
        7,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

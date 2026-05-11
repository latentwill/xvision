//! Verify `xvn skill *` returns the expected XvnExit code per scenario.
//! Reads status.code() from the spawned subprocess; doesn't import the
//! XvnExit enum (the contract under test is the *number* on the wire).

use std::process::Command;
use tempfile::tempdir;

const FIXTURE: &str =
    include_str!("../../xvision-skills/tests/fixtures/crypto-trader-base.md");

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

#[test]
fn skill_new_succeeds_returns_0() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("crypto-trader-base.md");
    std::fs::write(&p, FIXTURE).unwrap();
    let out = xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_new_missing_file_returns_2_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["skill", "new", "--from-file", "/tmp/does-not-exist.md"],
        dir.path(),
    );
    assert_eq!(code(&out), 2, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_new_malformed_returns_2_usage() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("bad.md");
    std::fs::write(&p, "no frontmatter").unwrap();
    let out = xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    assert_eq!(code(&out), 2, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_attach_unknown_strategy_returns_4_not_found() {
    let dir = tempdir().unwrap();
    // register a skill so the skill load doesn't fail first
    let p = dir.path().join("s.md");
    std::fs::write(&p, FIXTURE).unwrap();
    xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    let out = xvn(
        &["skill", "attach", "no-such-strategy",
          "--slot", "trader", "--skill", "crypto-trader-base"],
        dir.path(),
    );
    assert_eq!(code(&out), 4, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_attach_unknown_skill_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        dir.path(),
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let out = xvn(
        &["skill", "attach", &id, "--slot", "trader", "--skill", "no-such-skill"],
        dir.path(),
    );
    assert_eq!(code(&out), 4, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_attach_unknown_slot_returns_2_usage() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("s.md");
    std::fs::write(&p, FIXTURE).unwrap();
    xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        dir.path(),
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let out = xvn(
        &["skill", "attach", &id, "--slot", "bogus", "--skill", "crypto-trader-base"],
        dir.path(),
    );
    assert_eq!(code(&out), 2, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_attach_empty_slot_returns_7_conflict() {
    // mean_reversion template fills regime_slot and trader_slot, but
    // intern_slot is None. Attaching to intern should hit "slot is empty".
    let dir = tempdir().unwrap();
    let p = dir.path().join("s.md");
    std::fs::write(&p, FIXTURE).unwrap();
    xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        dir.path(),
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let out = xvn(
        &["skill", "attach", &id, "--slot", "intern", "--skill", "crypto-trader-base"],
        dir.path(),
    );
    assert_eq!(code(&out), 7, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

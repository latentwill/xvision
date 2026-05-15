//! Verify `xvn strategy *` returns the expected XvnExit code per scenario.

use std::process::Command;
use tempfile::tempdir;

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

fn new_strategy_id(home: &std::path::Path) -> String {
    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        home,
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn strategy_templates_returns_0() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "templates"], dir.path());
    assert_eq!(code(&out), 0);
}

#[test]
fn strategy_ls_empty_returns_0() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "ls"], dir.path());
    assert_eq!(code(&out), 0);
}

#[test]
fn strategy_new_unknown_template_returns_2_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["strategy", "new", "--template", "no-such-template", "--name", "x"],
        dir.path(),
    );
    assert_eq!(code(&out), 2);
}

#[test]
fn strategy_show_unknown_id_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "show", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"], dir.path());
    assert_eq!(code(&out), 4);
}

#[test]
fn strategy_validate_unknown_id_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["strategy", "validate", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"],
        dir.path(),
    );
    assert_eq!(code(&out), 4);
}

#[test]
fn strategy_add_agent_unknown_agent_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let id = new_strategy_id(dir.path());
    let out = xvn(
        &[
            "strategy",
            "add-agent",
            &id,
            "missing-agent-id",
            "--role",
            "trader",
        ],
        dir.path(),
    );
    assert_eq!(code(&out), 4, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn strategy_set_pipeline_unknown_kind_returns_2_usage() {
    let dir = tempdir().unwrap();
    let id = new_strategy_id(dir.path());
    let out = xvn(
        &["strategy", "set-pipeline", &id, "--kind", "parallel"],
        dir.path(),
    );
    assert_eq!(code(&out), 2, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn strategy_run_missing_anthropic_key_returns_3_auth() {
    let dir = tempdir().unwrap();
    let id = new_strategy_id(dir.path());

    // Force ANTHROPIC_API_KEY unset — must use a Command that explicitly
    // removes the env var, since the parent process may have it set.
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args([
            "strategy",
            "run",
            &id,
            "--fixture",
            "test-fixture-btc-2024-01",
            "--decisions",
            "1",
        ])
        .env("XVN_HOME", dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .output()
        .expect("xvn invocation");
    assert_eq!(code(&out), 3, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

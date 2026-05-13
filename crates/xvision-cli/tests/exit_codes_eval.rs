//! Verify `xvn eval *` returns the expected XvnExit code per scenario.
//! These tests exercise the verbs that don't need broker / dispatch
//! construction (list, show, scenarios, compare). `eval run` and
//! `eval attest` are deferred — they need richer fixture setup and
//! their own integration test file.

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

#[test]
fn eval_scenarios_returns_0() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "scenarios"], dir.path());
    assert_eq!(code(&out), 0);
}

#[test]
fn eval_list_empty_returns_0() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "list"], dir.path());
    assert_eq!(code(&out), 0);
}

#[test]
fn eval_list_bad_status_returns_2_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "list", "--status", "bogus-status"], dir.path());
    assert_eq!(code(&out), 2);
}

#[test]
fn eval_show_unknown_run_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "show", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"], dir.path());
    assert_eq!(code(&out), 4);
}

#[test]
fn eval_get_alias_unknown_run_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "get", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"], dir.path());
    assert_eq!(code(&out), 4);
}

#[test]
fn eval_compare_single_id_returns_2_clap_usage() {
    // num_args=2.. — clap rejects with exit 2 before reaching engine.
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "compare", "only-one-id"], dir.path());
    assert_eq!(code(&out), 2);
}

#[test]
fn eval_compare_two_unknown_ids_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["eval", "compare",
          "01ZZZZZZZZZZZZZZZZZZZZZZZZ", "02ZZZZZZZZZZZZZZZZZZZZZZZZ"],
        dir.path(),
    );
    assert_eq!(code(&out), 4);
}

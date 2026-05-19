//! Verify `xvn ab-compare` maps caller-fixable validation errors to Usage.

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

fn assert_usage_error(out: &std::process::Output, expected: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(code(out), 2, "stderr: {stderr}");
    assert!(
        stderr.contains(expected),
        "expected stderr to contain {expected:?}, got: {stderr}"
    );
}

#[test]
fn ab_compare_missing_bars_source_returns_2_usage() {
    let dir = tempdir().unwrap();
    let cycles = dir.path().join("cycles.json");
    let output = dir.path().join("result.json");
    std::fs::write(&cycles, "[]").unwrap();

    let out = xvn(
        &[
            "ab-compare",
            "--cycles",
            cycles.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
        dir.path(),
    );
    assert_usage_error(
        &out,
        "must supply either --bars <path> OR --from <date> + --to <date>",
    );
}

#[test]
fn ab_compare_rejects_bars_with_date_window_as_2_usage() {
    let dir = tempdir().unwrap();
    let cycles = dir.path().join("cycles.json");
    let bars = dir.path().join("bars.json");
    let output = dir.path().join("result.json");
    std::fs::write(&cycles, "[]").unwrap();
    std::fs::write(&bars, "[]").unwrap();

    let out = xvn(
        &[
            "ab-compare",
            "--cycles",
            cycles.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--bars",
            bars.to_str().unwrap(),
            "--from",
            "2024-01-01",
            "--to",
            "2024-01-02",
        ],
        dir.path(),
    );
    assert_usage_error(
        &out,
        "--bars is mutually exclusive with --from/--to; pick one source",
    );
}

#[test]
fn ab_compare_invalid_asset_returns_2_usage() {
    let dir = tempdir().unwrap();
    let cycles = dir.path().join("cycles.json");
    let bars = dir.path().join("bars.json");
    let output = dir.path().join("result.json");
    std::fs::write(&cycles, "[]").unwrap();
    std::fs::write(&bars, "[]").unwrap();

    let out = xvn(
        &[
            "ab-compare",
            "--cycles",
            cycles.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--bars",
            bars.to_str().unwrap(),
            "--asset",
            "NOT_A_SYMBOL",
        ],
        dir.path(),
    );
    assert_usage_error(&out, "asset 'NOT_A_SYMBOL' is not in the Alpaca crypto whitelist");
}

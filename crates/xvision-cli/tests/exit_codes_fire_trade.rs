//! Verify `xvn fire-trade` rejects unsafe live-order arguments at parse time.

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

fn assert_usage(args: &[&str], expected_stderr: &str) {
    let dir = tempdir().unwrap();
    let out = xvn(args, dir.path());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(code(&out), 2, "stderr: {stderr}");
    assert!(
        stderr.contains(expected_stderr),
        "expected stderr to contain {expected_stderr:?}, got: {stderr}"
    );
}

#[test]
fn fire_trade_rejects_out_of_range_size_bps() {
    assert_usage(
        &["fire-trade", "--side", "buy", "--size-bps", "2001"],
        "size-bps must be between 0 and 2000",
    );
}

#[test]
fn fire_trade_rejects_zero_stop_loss_pct() {
    assert_usage(
        &[
            "fire-trade",
            "--side",
            "buy",
            "--size-bps",
            "100",
            "--stop-loss-pct",
            "0",
        ],
        "stop-loss-pct must be a finite number between 0.1 and 20",
    );
}

#[test]
fn fire_trade_rejects_non_finite_take_profit_pct() {
    assert_usage(
        &[
            "fire-trade",
            "--side",
            "buy",
            "--size-bps",
            "100",
            "--take-profit-pct",
            "NaN",
        ],
        "take-profit-pct must be a finite number between 0.1 and 50",
    );
}

#[test]
fn fire_trade_rejects_short_summary() {
    assert_usage(
        &[
            "fire-trade",
            "--side",
            "buy",
            "--size-bps",
            "100",
            "--summary",
            "short",
        ],
        "summary must be between 10 and 500 characters",
    );
}

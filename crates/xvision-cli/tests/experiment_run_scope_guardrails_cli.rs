//! CLI-binary tests for `xvn experiment run` scope guardrails
//! (cli-operator-safety-p0 slice 3/3).
//!
//! Focuses on the new flags: `--max-runs`, `--yes`, and the dry-run plan
//! print path. The full happy-path orchestration is covered by
//! `experiment_run.rs` against the testable `run_experiment` helper —
//! these tests only exercise the CLI handler's gate.

use std::process::Command;

use tempfile::tempdir;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

#[test]
fn experiment_run_without_yes_prints_plan_and_exits_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "experiment",
            "run",
            "--name",
            "scope-test",
            "--strategy",
            "01TESTSTRATEGY00000000000000",
            "--scenarios",
            "scenario-a,scenario-b,scenario-c",
        ],
        dir.path(),
    );

    // No --yes → must exit non-zero with Usage and print the plan.
    assert!(!out.status.success(), "expected non-zero exit without --yes");
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected XvnExit::Usage (2), got {:?}",
        out.status.code()
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("==== experiment-run plan ===="),
        "plan header should print to stderr: {stderr}"
    );
    assert!(
        stderr.contains("runs to launch:"),
        "plan should name the run count: {stderr}"
    );
    assert!(
        stderr.contains("scenario-a") && stderr.contains("scenario-b") && stderr.contains("scenario-c"),
        "plan should list every resolved scenario: {stderr}"
    );
    assert!(
        stderr.contains("Re-run with --yes"),
        "exit message should tell the operator how to confirm: {stderr}"
    );
}

#[test]
fn max_runs_caps_scenario_list_in_plan() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "experiment",
            "run",
            "--name",
            "cap-test",
            "--strategy",
            "01TESTSTRATEGY00000000000000",
            "--scenarios",
            "s1,s2,s3,s4,s5",
            "--max-runs",
            "2",
        ],
        dir.path(),
    );

    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("capped from 5 by --max-runs=2"),
        "plan should call out the cap: {stderr}"
    );
    assert!(
        stderr.contains("s1") && stderr.contains("s2"),
        "first two scenarios should be in the plan: {stderr}"
    );
    assert!(
        !stderr.contains("s3") && !stderr.contains("s4") && !stderr.contains("s5"),
        "scenarios beyond the cap should NOT be in the plan: {stderr}"
    );
}

#[test]
fn max_runs_zero_returns_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "experiment",
            "run",
            "--name",
            "zero-test",
            "--strategy",
            "01TESTSTRATEGY00000000000000",
            "--scenarios",
            "s1",
            "--max-runs",
            "0",
            "--yes",
        ],
        dir.path(),
    );
    assert!(!out.status.success(), "max-runs=0 must reject");
    assert_eq!(out.status.code(), Some(2), "expected Usage exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--max-runs must be > 0"), "stderr: {stderr}");
}

#[test]
fn plan_notes_sequential_execution_order() {
    // Operator-visible: confirms the contract that experiment-run is
    // sequential by default. If parallelism is ever added, this test
    // forces the change to update the plan output.
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "experiment",
            "run",
            "--name",
            "order-test",
            "--strategy",
            "01TESTSTRATEGY00000000000000",
            "--scenarios",
            "s1,s2",
        ],
        dir.path(),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("sequential"),
        "plan must declare sequential execution: {stderr}"
    );
}

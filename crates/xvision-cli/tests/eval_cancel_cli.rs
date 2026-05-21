//! Integration tests for `xvn eval cancel`.
//!
//! Seeds Runs via the engine's `RunStore`, invokes the CLI process with
//! various selector combinations, and asserts that the run's status
//! transitions to Cancelled in the DB plus the CLI stdout JSON shape is
//! stable.

use std::process::Command;

use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

async fn seed_run_with_status(home: &std::path::Path, agent_id: &str, status: RunStatus) -> String {
    let ctx = ApiContext::open(
        home,
        Actor::Cli {
            user: "eval-cancel-cli-test".into(),
        },
    )
    .await
    .expect("open ApiContext");
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued(agent_id.into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.expect("seed run");
    if status != RunStatus::Queued {
        store
            .update_status(&id, status, None)
            .await
            .expect("transition run");
    }
    id
}

async fn run_status(home: &std::path::Path, run_id: &str) -> RunStatus {
    let ctx = ApiContext::open(
        home,
        Actor::Cli {
            user: "eval-cancel-cli-test".into(),
        },
    )
    .await
    .expect("open ApiContext");
    let store = RunStore::new(ctx.db.clone());
    let run = store.get(run_id).await.expect("get run");
    run.status
}

#[test]
fn no_selector_returns_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "cancel"], dir.path());
    assert!(!out.status.success(), "expected non-zero exit");
    // Exit code 2 = XvnExit::Usage
    assert_eq!(out.status.code(), Some(2), "expected exit 2 (Usage)");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("at least one of"),
        "stderr should explain missing selectors: {stderr}"
    );
}

#[test]
fn cancel_by_explicit_id_marks_run_cancelled() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let run_id = rt.block_on(async { seed_run_with_status(dir.path(), "agent-X", RunStatus::Running).await });

    let out = xvn(&["eval", "cancel", &run_id, "--json"], dir.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");
    let cancelled_ids = body["cancelled_ids"].as_array().expect("cancelled_ids array");
    assert_eq!(cancelled_ids.len(), 1);
    assert_eq!(cancelled_ids[0].as_str(), Some(run_id.as_str()));
    assert_eq!(body["outcomes"][&run_id], "cancelled");

    let final_status = rt.block_on(async { run_status(dir.path(), &run_id).await });
    assert_eq!(final_status, RunStatus::Cancelled);
}

#[test]
fn cancel_running_filters_to_active_runs_only() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Two active runs (queued + running) — both should be cancelled.
    // One completed run — must be left alone.
    let queued = rt.block_on(async { seed_run_with_status(dir.path(), "agent-Y", RunStatus::Queued).await });
    let running =
        rt.block_on(async { seed_run_with_status(dir.path(), "agent-Z", RunStatus::Running).await });
    let completed =
        rt.block_on(async { seed_run_with_status(dir.path(), "agent-W", RunStatus::Completed).await });

    let out = xvn(&["eval", "cancel", "--running", "--json"], dir.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");
    let cancelled_ids: Vec<&str> = body["cancelled_ids"]
        .as_array()
        .expect("cancelled_ids array")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect();
    assert!(
        cancelled_ids.contains(&queued.as_str()),
        "queued run should be cancelled: {cancelled_ids:?}"
    );
    assert!(
        cancelled_ids.contains(&running.as_str()),
        "running run should be cancelled: {cancelled_ids:?}"
    );
    assert!(
        !cancelled_ids.contains(&completed.as_str()),
        "completed run should NOT be in cancelled_ids: {cancelled_ids:?}"
    );

    // Verify the completed run's status was not touched.
    let final_completed = rt.block_on(async { run_status(dir.path(), &completed).await });
    assert_eq!(final_completed, RunStatus::Completed);
}

#[test]
fn cancel_by_strategy_filters_to_that_agent() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mine =
        rt.block_on(async { seed_run_with_status(dir.path(), "agent-target", RunStatus::Running).await });
    let theirs =
        rt.block_on(async { seed_run_with_status(dir.path(), "agent-other", RunStatus::Running).await });

    let out = xvn(
        &["eval", "cancel", "--strategy", "agent-target", "--json"],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");
    let cancelled_ids: Vec<&str> = body["cancelled_ids"]
        .as_array()
        .expect("cancelled_ids array")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect();
    assert_eq!(cancelled_ids, vec![mine.as_str()]);

    let other_status = rt.block_on(async { run_status(dir.path(), &theirs).await });
    assert_eq!(other_status, RunStatus::Running);
}

#[test]
fn cancel_already_terminal_reports_outcome_without_failing() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let completed =
        rt.block_on(async { seed_run_with_status(dir.path(), "agent-done", RunStatus::Completed).await });

    let out = xvn(&["eval", "cancel", &completed, "--json"], dir.path());
    assert!(
        out.status.success(),
        "cancel of a terminal run should still exit 0 with an outcome: stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");
    assert!(body["cancelled_ids"].as_array().unwrap().is_empty());
    // Engine maps "is already" → Validation error → we map to
    // "already_terminal".
    assert_eq!(body["outcomes"][&completed], "already_terminal");
}

#[test]
fn older_than_with_bad_unit_returns_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "cancel", "--older-than", "5x"], dir.path());
    assert!(!out.status.success(), "expected non-zero exit");
    assert_eq!(out.status.code(), Some(2), "expected exit 2 (Usage)");
}

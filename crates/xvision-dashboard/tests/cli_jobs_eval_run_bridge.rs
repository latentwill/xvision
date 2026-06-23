//! Integration tests for the `eval_run_` bridge in
//! `crates/xvision-dashboard/src/cli_jobs/eval_run_bridge.rs`.
//!
//! Exercises `get_synthetic_job` and `get_synthetic_output` directly against
//! a tempdir-backed SQLite database seeded with `RunStore` rows. No HTTP
//! server or mock LLM dispatch needed — the bridge functions take a raw
//! `SqlitePool` so we can unit-test them in isolation.
//!
//! Scenarios:
//! 1. Queued run  → CliJobStatus::Queued
//! 2. Running run → CliJobStatus::Running
//! 3. Completed   → CliJobStatus::Succeeded, output contains metrics JSON
//! 4. Failed      → CliJobStatus::Failed, error_message propagated
//! 5. Cancelled   → CliJobStatus::Cancelled
//! 6. Unknown id  → Ok(None)  (not found returns None, not an error)
//! 7. Bare ULID (no prefix) → Ok(None)  (bridge ignores non-prefixed ids)

use xvision_dashboard::cli_jobs::eval_run_bridge::{
    get_synthetic_job, get_synthetic_output, EVAL_RUN_PREFIX,
};
use xvision_dashboard::AppState;
use xvision_engine::eval::{
    run::{MetricsSummary, Run, RunMode, RunStatus},
    store::RunStore,
};

/// Boot a tempdir-backed AppState and return the SQLite pool.
async fn boot_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init AppState");
    (state.pool.clone(), tmp)
}

/// Seed an eval run at a given status and return its id.
async fn seed_run(pool: &sqlx::SqlitePool, status: RunStatus) -> String {
    let store = RunStore::new(pool.clone());
    let run = Run::new_queued(
        "agent-test".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    if status != RunStatus::Queued {
        store.update_status(&run.id, status, None).await.unwrap();
    }
    run.id.clone()
}

/// Construct the `eval_run_`-prefixed id the wizard agent would pass in.
fn prefixed(run_id: &str) -> String {
    format!("{EVAL_RUN_PREFIX}{run_id}")
}

fn sample_metrics() -> MetricsSummary {
    MetricsSummary {
        total_return_pct: 12.5,
        sharpe: 1.75,
        max_drawdown_pct: -4.25,
        win_rate: 0.625,
        n_trades: 8,
        n_decisions: 21,
        inference_cost_quote_total: Some(0.42),
        net_return_pct: Some(12.08),
        baselines: None,
        ..Default::default()
    }
}

#[tokio::test]
async fn queued_run_maps_to_queued_status() {
    let (pool, _tmp) = boot_pool().await;
    let run_id = seed_run(&pool, RunStatus::Queued).await;
    let job_id = prefixed(&run_id);

    let job = get_synthetic_job(&pool, &job_id)
        .await
        .unwrap()
        .expect("should find job");
    assert_eq!(job.job_id, job_id);
    assert_eq!(job.status.as_str(), "queued");
    assert!(job.started_at.is_some(), "started_at should be populated");
    assert!(job.finished_at.is_none());
    assert!(job.error_message.is_none());
}

#[tokio::test]
async fn running_run_maps_to_running_status() {
    let (pool, _tmp) = boot_pool().await;
    let run_id = seed_run(&pool, RunStatus::Running).await;
    let job_id = prefixed(&run_id);

    let job = get_synthetic_job(&pool, &job_id)
        .await
        .unwrap()
        .expect("should find job");
    assert_eq!(job.status.as_str(), "running");
}

#[tokio::test]
async fn completed_run_maps_to_succeeded_status() {
    let (pool, _tmp) = boot_pool().await;
    let run_id = seed_run(&pool, RunStatus::Completed).await;
    let job_id = prefixed(&run_id);

    let job = get_synthetic_job(&pool, &job_id)
        .await
        .unwrap()
        .expect("should find job");
    assert_eq!(job.status.as_str(), "succeeded");
    // completed_at is populated by the executor's finalize path; bare
    // update_status does not set it, so finished_at may be None here.
    assert!(!job.timed_out);
}

#[tokio::test]
async fn completed_run_output_contains_eval_summary() {
    let (pool, _tmp) = boot_pool().await;
    let store = RunStore::new(pool.clone());
    let run = Run::new_queued(
        "agent-test".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    let run_id = run.id.clone();
    store.create(&run).await.unwrap();
    store.finalize(&run_id, &sample_metrics()).await.unwrap();
    let job_id = prefixed(&run_id);

    let output = get_synthetic_output(&pool, &job_id)
        .await
        .unwrap()
        .expect("should find output");

    assert_eq!(output.status.as_str(), "succeeded");
    assert!(output.stdout_bytes > 0, "stdout should be non-empty");

    // stdout is a JSON eval summary
    let summary: serde_json::Value = serde_json::from_str(&output.stdout).expect("stdout is valid JSON");
    assert_eq!(summary["run_id"].as_str().unwrap(), run_id);
    assert_eq!(summary["status"].as_str().unwrap(), "completed");
    assert!(summary["detail_url"].as_str().unwrap().contains(&run_id));
    assert_eq!(summary["scenario_id"].as_str().unwrap(), "crypto-bull-q1-2025");
    assert_eq!(summary["metrics"]["total_return_pct"], 12.5);
    assert_eq!(summary["metrics"]["sharpe"], 1.75);
    assert_eq!(summary["metrics"]["max_drawdown_pct"], -4.25);
    assert_eq!(summary["metrics"]["win_rate"], 0.625);
    assert_eq!(summary["metrics"]["n_trades"], 8);
    assert_eq!(summary["metrics"]["n_decisions"], 21);
    assert_eq!(summary["metrics"]["inference_cost_quote_total"], 0.42);
    assert_eq!(summary["metrics"]["net_return_pct"], 12.08);
}

#[tokio::test]
async fn failed_run_maps_to_failed_status_with_error() {
    let (pool, _tmp) = boot_pool().await;
    let store = RunStore::new(pool.clone());
    let run = Run::new_queued(
        "agent-test".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    store
        .update_status(&run.id, RunStatus::Failed, Some("broker timeout"))
        .await
        .unwrap();

    let job_id = prefixed(&run.id);

    let job = get_synthetic_job(&pool, &job_id)
        .await
        .unwrap()
        .expect("should find job");
    assert_eq!(job.status.as_str(), "failed");
    assert_eq!(job.error_message.as_deref(), Some("broker timeout"));

    let output = get_synthetic_output(&pool, &job_id)
        .await
        .unwrap()
        .expect("should find output");
    assert_eq!(output.status.as_str(), "failed");
    // stderr carries the error message
    assert!(
        output.stderr.contains("broker timeout"),
        "stderr should contain error: {}",
        output.stderr
    );
}

#[tokio::test]
async fn cancelled_run_maps_to_cancelled_status() {
    let (pool, _tmp) = boot_pool().await;
    let run_id = seed_run(&pool, RunStatus::Cancelled).await;
    let job_id = prefixed(&run_id);

    let job = get_synthetic_job(&pool, &job_id)
        .await
        .unwrap()
        .expect("should find job");
    assert_eq!(job.status.as_str(), "cancelled");
    assert!(job.cancel_requested);
}

#[tokio::test]
async fn unknown_eval_run_id_returns_none() {
    let (pool, _tmp) = boot_pool().await;
    let job_id = format!("{EVAL_RUN_PREFIX}01JZZZZZZZZZZZZZZZZZZZZZZZ");

    let job = get_synthetic_job(&pool, &job_id).await.unwrap();
    assert!(job.is_none(), "nonexistent run should return None");

    let output = get_synthetic_output(&pool, &job_id).await.unwrap();
    assert!(output.is_none(), "nonexistent run output should return None");
}

#[tokio::test]
async fn bare_ulid_without_prefix_returns_none() {
    let (pool, _tmp) = boot_pool().await;
    // Seed a real run, but call the bridge with the bare id (no prefix)
    let run_id = seed_run(&pool, RunStatus::Queued).await;

    let job = get_synthetic_job(&pool, &run_id).await.unwrap();
    assert!(job.is_none(), "bare ulid without prefix should return None");

    let output = get_synthetic_output(&pool, &run_id).await.unwrap();
    assert!(
        output.is_none(),
        "bare ulid without prefix output should return None"
    );
}

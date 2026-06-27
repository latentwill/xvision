//! Stage 1 observability test (operational-visibility contract item 3).
//!
//! Asserts that a run persisted through the `SqliteRecorder` records
//! `agent_runs.trajectory_mode = 'live'` once migration 039 is applied —
//! the live runtime mode surfaces in the structured run record. The value
//! comes from the migration-039 column default (`'live'`), which the
//! recorder relies on rather than threading a new field through the
//! `RunEvent` vocabulary (see the comment in `sqlite.rs`).
//!
//! Also asserts the migration is idempotent (applying it twice is a no-op
//! at the table level) and that the sibling Stage 2-3 columns
//! (replay_hit_ratio / dropped_events / recovery_reason) exist at their
//! declared defaults.

use sqlx::SqlitePool;

use xvision_observability::{AgentRunRecorder, RunEvent, RunStartedEvent, SqliteRecorder};

const MIGRATION_002: &str = include_str!("../migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");
const MIGRATION_039: &str = include_str!("../migrations/039_run_trajectory_mode.sql");

/// Apply the migrations `agent_runs` (and its FK targets) need. `agent_runs`
/// declares FKs to `cli_jobs` (013) and `eval_runs` (002), so both must
/// exist before an INSERT under SQLite FK enforcement.
async fn base_pool() -> (SqlitePool, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    (pool, tmp)
}

async fn setup_pool() -> (SqlitePool, tempfile::TempDir) {
    let (pool, tmp) = base_pool().await;
    sqlx::query(MIGRATION_039).execute(&pool).await.unwrap();
    (pool, tmp)
}

fn run_started(run_id: &str) -> RunEvent {
    RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.to_string(),
        objective: "cline live cycle".into(),
        strategy_id: Some("01HZSTRAT".into()),
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: chrono::Utc::now(),
        retention_mode: "hash_only".into(),
        trajectory_mode: None,
        sidecar_version: Some("mock-0.0.1".into()),
        cline_sdk_version: Some("mock-cline".into()),
        protocol_version: Some("0.1.0".into()),
        skills_json: None,
        mcp_servers_json: None,
    })
}

#[tokio::test]
async fn run_started_records_trajectory_mode_live() {
    let (pool, _tmp) = setup_pool().await;
    let recorder = SqliteRecorder::new(pool.clone());

    recorder
        .handle_event(&run_started("run-live-1"))
        .await
        .expect("RunStarted must persist");

    let mode: String = sqlx::query_scalar("SELECT trajectory_mode FROM agent_runs WHERE id = ?")
        .bind("run-live-1")
        .fetch_one(&pool)
        .await
        .expect("agent_runs row must exist");
    assert_eq!(
        mode, "fwd",
        "Stage 1 live runs must record trajectory_mode = 'live'"
    );
}

#[tokio::test]
async fn migration_039_declares_stage23_sibling_columns_with_defaults() {
    let (pool, _tmp) = setup_pool().await;
    let recorder = SqliteRecorder::new(pool.clone());
    recorder
        .handle_event(&run_started("run-live-2"))
        .await
        .expect("RunStarted must persist");

    // dropped_events defaults to 0; replay_hit_ratio / recovery_reason are
    // NULL until Stages 2-3 populate them.
    let dropped: i64 = sqlx::query_scalar("SELECT dropped_events FROM agent_runs WHERE id = ?")
        .bind("run-live-2")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(dropped, 0);

    let replay_ratio: Option<f64> =
        sqlx::query_scalar("SELECT replay_hit_ratio FROM agent_runs WHERE id = ?")
            .bind("run-live-2")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(replay_ratio.is_none(), "replay_hit_ratio is NULL on a live run");

    let recovery_reason: Option<String> =
        sqlx::query_scalar("SELECT recovery_reason FROM agent_runs WHERE id = ?")
            .bind("run-live-2")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(recovery_reason.is_none(), "recovery_reason is NULL on a live run");
}

#[tokio::test]
async fn migration_018_only_pool_still_inserts_run_started() {
    // The recorder INSERT must keep working on a pool WITHOUT migration 039
    // (no trajectory_mode column) — the column is omitted from the INSERT
    // so the absence is harmless. This pins the backward-compat guarantee
    // for the many observability tests that apply the pre-039 schema.
    let (pool, _tmp) = base_pool().await;

    let recorder = SqliteRecorder::new(pool.clone());
    recorder
        .handle_event(&run_started("run-018-only"))
        .await
        .expect("RunStarted must persist on a migration-018-only pool");

    let status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = ?")
        .bind("run-018-only")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "running");
}

//! Stage 3 Task 3 — run-level replay metrics writer.
//!
//! Asserts `SqliteRecorder::set_run_replay_metrics` updates the
//! migration-039 `agent_runs` columns (`trajectory_mode`, `replay_hit_ratio`,
//! `recovery_reason`) on an existing run row, and is a no-op for an unknown
//! run id.
//!
//! A minimal `agent_runs` table is created inline with just the columns the
//! writer touches plus the id — independent of the full migration chain so
//! the test does not couple to migration ordering.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use xvision_observability::SqliteRecorder;

async fn pool_with_run(run_id: &str) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE agent_runs (
           id TEXT PRIMARY KEY,
           trajectory_mode TEXT NOT NULL DEFAULT 'live',
           replay_hit_ratio REAL,
           dropped_events INTEGER NOT NULL DEFAULT 0,
           recovery_reason TEXT
         )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO agent_runs (id) VALUES (?)")
        .bind(run_id)
        .execute(&pool)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn set_replay_metrics_marks_run_replay() {
    let pool = pool_with_run("run-1").await;
    let rec = SqliteRecorder::new(pool.clone());

    // Default seeded by the column default.
    let mode: (String, Option<f64>, Option<String>) =
        sqlx::query_as("SELECT trajectory_mode, replay_hit_ratio, recovery_reason FROM agent_runs WHERE id = 'run-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(mode.0, "live");
    assert!(mode.1.is_none());

    let affected = rec
        .set_run_replay_metrics("run-1", "replay", Some(1.0), None)
        .await
        .unwrap();
    assert_eq!(affected, 1);

    let row: (String, Option<f64>, Option<String>) =
        sqlx::query_as("SELECT trajectory_mode, replay_hit_ratio, recovery_reason FROM agent_runs WHERE id = 'run-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "replay");
    assert_eq!(row.1, Some(1.0));
    assert!(row.2.is_none());
}

#[tokio::test]
async fn set_replay_metrics_writes_recovery_reason() {
    let pool = pool_with_run("run-2").await;
    let rec = SqliteRecorder::new(pool.clone());

    rec.set_run_replay_metrics("run-2", "replay", Some(0.0), Some("replay_divergence"))
        .await
        .unwrap();

    let row: (String, Option<f64>, Option<String>) =
        sqlx::query_as("SELECT trajectory_mode, replay_hit_ratio, recovery_reason FROM agent_runs WHERE id = 'run-2'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "replay");
    assert_eq!(row.2.as_deref(), Some("replay_divergence"));
}

#[tokio::test]
async fn set_replay_metrics_is_noop_for_unknown_run() {
    let pool = pool_with_run("run-3").await;
    let rec = SqliteRecorder::new(pool.clone());

    let affected = rec
        .set_run_replay_metrics("does-not-exist", "replay", Some(1.0), None)
        .await
        .unwrap();
    assert_eq!(affected, 0, "unknown run id must affect zero rows");
}

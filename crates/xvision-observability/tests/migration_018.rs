//! Migration 018 lands cleanly on top of the eval/cli_jobs foundations
//! (which it FK-references). Also verifies that the down migration drops
//! everything.

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");
const MIGRATION_018_DOWN: &str =
    include_str!("../../xvision-engine/migrations/018_agent_run_observability.down.sql");

async fn pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap()
}

async fn table_count(pool: &SqlitePool, table: &str) -> i64 {
    let row: (i64,) = sqlx::query_as(&format!(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '{table}'"
    ))
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

#[tokio::test]
async fn migration_018_applies_cleanly() {
    let pool = pool().await;
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();

    // Every table the plan promises is present.
    for table in [
        "agent_runs",
        "spans",
        "checkpoints",
        "model_calls",
        "tool_calls",
        "approvals",
        "sandbox_results",
        "supervisor_notes",
        "artifacts",
        "events",
    ] {
        assert_eq!(
            table_count(&pool, table).await,
            1,
            "table `{table}` was not created by migration 018"
        );
    }
}

#[tokio::test]
async fn migration_018_is_idempotent() {
    let pool = pool().await;
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    // Re-running must be a no-op thanks to `IF NOT EXISTS`.
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn migration_018_down_removes_everything() {
    let pool = pool().await;
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018_DOWN).execute(&pool).await.unwrap();

    for table in [
        "agent_runs",
        "spans",
        "checkpoints",
        "model_calls",
        "tool_calls",
        "approvals",
        "sandbox_results",
        "supervisor_notes",
        "artifacts",
        "events",
    ] {
        assert_eq!(
            table_count(&pool, table).await,
            0,
            "table `{table}` should have been dropped by the down migration"
        );
    }

    for table in ["eval_runs", "cli_jobs"] {
        assert_eq!(
            table_count(&pool, table).await,
            1,
            "foundation table `{table}` should survive migration 018 down"
        );
    }
}

#[tokio::test]
async fn agent_runs_round_trips_a_minimal_row() {
    let pool = pool().await;
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();

    let run_id = "run_test_001";
    sqlx::query(
        "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind("smoke")
    .bind("queued")
    .bind("2026-05-17T00:00:00Z")
    .bind("hash_only")
    .execute(&pool)
    .await
    .unwrap();

    let (id, status, retention): (String, String, String) =
        sqlx::query_as("SELECT id, status, retention_mode FROM agent_runs WHERE id = ?")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(id, run_id);
    assert_eq!(status, "queued");
    assert_eq!(retention, "hash_only");
}

#[tokio::test]
async fn agent_runs_enforces_eval_and_cli_job_foreign_keys() {
    let pool = pool().await;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO eval_runs \
         (id, strategy_bundle_hash, scenario_id, mode, status, started_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("eval_test_001")
    .bind("bundle_hash")
    .bind("scenario_test_001")
    .bind("paper")
    .bind("queued")
    .bind("2026-05-17T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cli_jobs (job_id, argv_json, status, created_at, timeout_secs) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("job_test_001")
    .bind("[]")
    .bind("queued")
    .bind("2026-05-17T00:00:00Z")
    .bind(60_i64)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO agent_runs \
         (id, objective, eval_run_id, source_cli_job_id, status, started_at, retention_mode) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("run_with_parents")
    .bind("fk smoke")
    .bind("eval_test_001")
    .bind("job_test_001")
    .bind("queued")
    .bind("2026-05-17T00:00:00Z")
    .bind("hash_only")
    .execute(&pool)
    .await
    .unwrap();

    let invalid_eval = sqlx::query(
        "INSERT INTO agent_runs \
         (id, objective, eval_run_id, status, started_at, retention_mode) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("run_invalid_eval")
    .bind("fk smoke")
    .bind("missing_eval")
    .bind("queued")
    .bind("2026-05-17T00:00:00Z")
    .bind("hash_only")
    .execute(&pool)
    .await;
    assert!(invalid_eval.is_err(), "missing eval_run_id should be rejected");

    let invalid_job = sqlx::query(
        "INSERT INTO agent_runs \
         (id, objective, source_cli_job_id, status, started_at, retention_mode) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("run_invalid_job")
    .bind("fk smoke")
    .bind("missing_job")
    .bind("queued")
    .bind("2026-05-17T00:00:00Z")
    .bind("hash_only")
    .execute(&pool)
    .await;
    assert!(
        invalid_job.is_err(),
        "missing source_cli_job_id should be rejected"
    );
}

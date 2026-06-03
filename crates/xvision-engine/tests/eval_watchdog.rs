//! Integration tests for the eval-run watchdog (F-3).
//!
//! Covers the four acceptance cases from the contract:
//!  1. A `running` row older than the threshold is finalized to
//!     `failed` with `error='timeout'` after one watchdog tick.
//!  2. A `running` row started within the threshold is left alone.
//!  3. The boot sweep finalizes a pre-existing stuck row at startup.
//!  4. Two ticks on the same stuck row do not double-write (idempotent).
//!
//! Plus a coverage case for the per-scenario override on
//! `params_override_json.max_run_duration_secs`.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_engine::eval::watchdog::{self, WatchdogConfig, DEFAULT_MAX_RUN_DURATION, TIMEOUT_REASON};
use xvision_engine::eval::{Run, RunMode, RunStatus, RunStore};

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/013_cli_jobs.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/018_agent_run_observability.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/037_review_annotations_and_autofire.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/038_eval_runs_live_config.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

/// Insert a `running` row with a synthetic `started_at` so we can put
/// the row deep in the past without waiting wall-clock time. The
/// `RunStore::create` API always stamps `started_at = now`, so the
/// tests go through SQL directly.
async fn insert_running_run(
    pool: &SqlitePool,
    id: &str,
    started_at: DateTime<Utc>,
    params_override_json: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO eval_runs \
         (id, agent_id, scenario_id, params_override_json, mode, status, started_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind("strategy-test")
    .bind("scenario-test")
    .bind(params_override_json)
    .bind("backtest")
    .bind("running")
    .bind(started_at.to_rfc3339())
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn watchdog_tick_finalizes_stuck_running_row_as_failed_timeout() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool.clone());
    let config = WatchdogConfig::new(Duration::from_secs(60), Duration::from_millis(10));

    let stuck_id = "01KS0A5DP8KZVQJ03TCKGKYJVN";
    // Started 10 minutes ago, way beyond the 60s budget. This mirrors
    // the audit case from the contract.
    let stuck_started_at = Utc::now() - chrono::Duration::seconds(600);
    insert_running_run(&pool, stuck_id, stuck_started_at, None).await;

    let n = watchdog::sweep_once(&pool, &store, &config, Utc::now())
        .await
        .unwrap();
    assert_eq!(n, 1, "exactly one stuck row should be finalized");

    let row = store.get(stuck_id).await.unwrap();
    assert_eq!(row.status, RunStatus::Failed);
    assert_eq!(row.error.as_deref(), Some(TIMEOUT_REASON));
    assert!(
        row.completed_at.is_some(),
        "completed_at must be stamped by finalize",
    );
}

#[tokio::test]
async fn watchdog_tick_leaves_fresh_running_row_alone() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool.clone());
    let config = WatchdogConfig::new(Duration::from_secs(1800), Duration::from_millis(10));

    let fresh_id = "01FRESH00000000000000000000";
    // Started 10 seconds ago — nowhere near the 30min budget.
    let fresh_started_at = Utc::now() - chrono::Duration::seconds(10);
    insert_running_run(&pool, fresh_id, fresh_started_at, None).await;

    let n = watchdog::sweep_once(&pool, &store, &config, Utc::now())
        .await
        .unwrap();
    assert_eq!(n, 0, "no rows should be finalized — all within budget");

    let row = store.get(fresh_id).await.unwrap();
    assert_eq!(row.status, RunStatus::Running);
    assert!(row.completed_at.is_none(), "fresh row must stay in flight");
    assert!(row.error.is_none(), "fresh row must have no error");
}

#[tokio::test]
async fn boot_sweep_finalizes_preexisting_stuck_row() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool.clone());
    let config = WatchdogConfig::new(Duration::from_secs(60), Duration::from_millis(10));

    // A row that survived a daemon restart in the `running` state and
    // is already past the budget. boot_sweep should finalize it
    // before the API starts serving traffic.
    let orphan_id = "01ORPHAN00000000000000000000";
    let started_at = Utc::now() - chrono::Duration::seconds(900);
    insert_running_run(&pool, orphan_id, started_at, None).await;

    let n = watchdog::boot_sweep(&pool, &store, &config).await.unwrap();
    assert_eq!(n, 1);

    let row = store.get(orphan_id).await.unwrap();
    assert_eq!(row.status, RunStatus::Failed);
    assert_eq!(row.error.as_deref(), Some(TIMEOUT_REASON));
}

#[tokio::test]
async fn two_ticks_on_same_stuck_row_do_not_double_write() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool.clone());
    let config = WatchdogConfig::new(Duration::from_secs(60), Duration::from_millis(10));

    let stuck_id = "01IDEMP000000000000000000000";
    let started_at = Utc::now() - chrono::Duration::seconds(600);
    insert_running_run(&pool, stuck_id, started_at, None).await;

    // First tick: should finalize.
    let n1 = watchdog::sweep_once(&pool, &store, &config, Utc::now())
        .await
        .unwrap();
    assert_eq!(n1, 1);

    let row_after_first = store.get(stuck_id).await.unwrap();
    let completed_at_first = row_after_first.completed_at.unwrap();
    assert_eq!(row_after_first.status, RunStatus::Failed);
    assert_eq!(row_after_first.error.as_deref(), Some(TIMEOUT_REASON));

    // Sleep a hair so a buggy second-write would tick `completed_at`.
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Second tick: must be a no-op. The row is already terminal so
    // `fail_active` short-circuits via its `WHERE status IN
    // ('queued','running')` guard.
    let n2 = watchdog::sweep_once(&pool, &store, &config, Utc::now())
        .await
        .unwrap();
    assert_eq!(n2, 0, "second tick must not refinalize a terminal row");

    let row_after_second = store.get(stuck_id).await.unwrap();
    assert_eq!(row_after_second.status, RunStatus::Failed);
    assert_eq!(row_after_second.error.as_deref(), Some(TIMEOUT_REASON));
    assert_eq!(
        row_after_second.completed_at.unwrap(),
        completed_at_first,
        "completed_at must not move on the idempotent second tick",
    );
}

#[tokio::test]
async fn per_run_override_extends_budget_beyond_global_default() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool.clone());
    let config = WatchdogConfig::new(
        Duration::from_secs(60), // global default: 60s
        Duration::from_millis(10),
    );

    // 5min old run with a 30min per-run override → not stuck.
    let extended_id = "01EXTEND000000000000000000000";
    let extended_started_at = Utc::now() - chrono::Duration::seconds(300);
    let extended_override = r#"{"max_run_duration_secs": 1800}"#;
    insert_running_run(&pool, extended_id, extended_started_at, Some(extended_override)).await;

    // A second row with no override, started 5min ago → stuck under
    // the 60s global default.
    let stuck_id = "01STUCK0000000000000000000000";
    let stuck_started_at = Utc::now() - chrono::Duration::seconds(300);
    insert_running_run(&pool, stuck_id, stuck_started_at, None).await;

    let n = watchdog::sweep_once(&pool, &store, &config, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        n, 1,
        "only the row without the per-run override should be finalized",
    );

    let extended = store.get(extended_id).await.unwrap();
    assert_eq!(
        extended.status,
        RunStatus::Running,
        "the row with the larger per-run override must stay running",
    );
    let stuck = store.get(stuck_id).await.unwrap();
    assert_eq!(stuck.status, RunStatus::Failed);
    assert_eq!(stuck.error.as_deref(), Some(TIMEOUT_REASON));
}

#[tokio::test]
async fn defaults_match_30min_budget_and_30s_tick() {
    // Lock-in test for the production defaults so a casual edit of
    // the constants in watchdog.rs trips a test instead of slipping
    // through review.
    let config = WatchdogConfig::default();
    assert_eq!(config.max_run_duration, Duration::from_secs(1800));
    assert_eq!(config.tick_interval, Duration::from_secs(30));
    assert_eq!(DEFAULT_MAX_RUN_DURATION, Duration::from_secs(1800));
}

#[tokio::test]
async fn sweep_skips_completed_and_failed_rows() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool.clone());
    let config = WatchdogConfig::new(Duration::from_secs(60), Duration::from_millis(10));

    // A completed row with an ancient started_at — must not be touched.
    let completed_id = "01COMPLETED000000000000000000";
    sqlx::query(
        "INSERT INTO eval_runs \
         (id, agent_id, scenario_id, mode, status, started_at, completed_at) \
         VALUES (?, ?, ?, ?, 'completed', ?, ?)",
    )
    .bind(completed_id)
    .bind("strategy-test")
    .bind("scenario-test")
    .bind("backtest")
    .bind("2020-01-01T00:00:00+00:00")
    .bind("2020-01-01T01:00:00+00:00")
    .execute(&pool)
    .await
    .unwrap();

    // And a queued row — also not in scope for the watchdog (the
    // contract scopes to `status='running'` only).
    let queued = Run::new_queued("strategy-test".into(), "scenario-test".into(), RunMode::Backtest);
    store.create(&queued).await.unwrap();
    store.ensure_agent_run_baseline(&queued.id, "hash_only").await.unwrap();

    let n = watchdog::sweep_once(&pool, &store, &config, Utc::now())
        .await
        .unwrap();
    assert_eq!(n, 0);

    assert_eq!(
        store.get(completed_id).await.unwrap().status,
        RunStatus::Completed
    );
    assert_eq!(store.get(&queued.id).await.unwrap().status, RunStatus::Queued);
}

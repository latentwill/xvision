//! B10: dead-holder detection + force_clear for the optimizer cycle lock.
//!
//! Killing the optimizer process (SIGKILL / OOM / panic) skips `release()`, so
//! the row lingers. Before this fix the only recovery was the 2h stale window —
//! every `optimize run` in that window failed with "already running". These tests
//! verify that a lock held by a dead PID is reclaimed immediately, that a live
//! holder is never stolen, and that `force_clear` always clears the row.

use chrono::Utc;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use xvision_engine::autooptimizer::run_lock::{ensure_run_lock_schema, force_clear, try_acquire, Acquire};

async fn pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap()
}

/// Pick a PID that is essentially guaranteed not to be a live process on this
/// host. PIDs near the top of the 32-bit space are never allocated by real
/// kernels, so `sysinfo` will report them as dead.
const GUARANTEED_DEAD_PID: i64 = 4_000_000_000;

async fn insert_lock_row(pool: &SqlitePool, cycle_id: &str, holder: &str, acquired_at: &str, pid: i64) {
    ensure_run_lock_schema(pool).await.unwrap();
    sqlx::query(
        "INSERT INTO optimizer_cycle_lock (id, cycle_id, holder, acquired_at, pid) \
         VALUES (1, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
            cycle_id = excluded.cycle_id, holder = excluded.holder, \
            acquired_at = excluded.acquired_at, pid = excluded.pid",
    )
    .bind(cycle_id)
    .bind(holder)
    .bind(acquired_at)
    .bind(pid)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn dead_holder_lock_is_reclaimed_immediately() {
    let pool = pool().await;
    // A FRESH row (acquired just now, well inside the 2h stale window) whose
    // holder PID is dead. Pre-fix this returns Busy; post-fix Acquired.
    let now = Utc::now();
    insert_lock_row(
        &pool,
        "dead-cycle",
        "cli:ghost",
        &now.to_rfc3339(),
        GUARANTEED_DEAD_PID,
    )
    .await;

    let outcome = try_acquire(&pool, "fresh-cycle", "cli:alice", now).await.unwrap();
    assert!(
        matches!(outcome.acquire, Acquire::Acquired),
        "a fresh lock held by a dead PID must be reclaimed immediately"
    );
}

#[tokio::test]
async fn live_holder_is_not_stolen() {
    let pool = pool().await;
    let now = Utc::now();
    // Acquire with the current (live) process PID via the real code path.
    assert!(matches!(
        try_acquire(&pool, "live-cycle", "cli:alice", now)
            .await
            .unwrap()
            .acquire,
        Acquire::Acquired
    ));

    // A second attempt must be refused — our own PID is alive.
    match try_acquire(&pool, "other-cycle", "dashboard", now)
        .await
        .unwrap()
        .acquire
    {
        Acquire::Busy { cycle_id, .. } => {
            assert_eq!(cycle_id, "live-cycle", "live holder must be reported");
        }
        Acquire::Acquired => panic!("a lock held by a live PID must not be stolen"),
    }
}

#[tokio::test]
async fn force_clear_removes_lock() {
    let pool = pool().await;
    let now = Utc::now();
    assert!(matches!(
        try_acquire(&pool, "stuck-cycle", "cli:alice", now)
            .await
            .unwrap()
            .acquire,
        Acquire::Acquired
    ));

    let cleared = force_clear(&pool).await.unwrap();
    assert_eq!(
        cleared.as_deref(),
        Some("stuck-cycle"),
        "force_clear must report the holder it cleared"
    );

    // Lock is now free.
    assert!(matches!(
        try_acquire(&pool, "next-cycle", "dashboard", now)
            .await
            .unwrap()
            .acquire,
        Acquire::Acquired
    ));
}

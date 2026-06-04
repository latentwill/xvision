//! F34: a cross-process advisory lock so only one optimizer cycle runs against a
//! workspace at a time. The CLI cycle and the dashboard cycle share the same
//! `$XVN_HOME/xvn.db`; running both at once starved each other (the QA's CLI
//! cycle was timeout-killed at 9.7 min while a dashboard cycle ran). Both the CLI
//! `run-cycle` and the dashboard launch acquire this lock first and get a clear
//! "a cycle is already running" response instead of silently degrading.
//!
//! The lock is a single-row table; acquire is one atomic upsert that succeeds
//! only when the row is absent or stale (a previous holder that died without
//! releasing). Stored in the shared DB so it spans processes — an in-memory lock
//! would only guard within one process.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

/// A cycle whose holder hasn't refreshed within this window is treated as dead,
/// so its lock can be taken over. Comfortably longer than any real cycle (the
/// observed runaway was ~27 min) so a live long cycle is never stolen from.
const STALE_AFTER_SECS: i64 = 2 * 60 * 60;

/// The outcome of a lock acquisition attempt.
#[derive(Debug, Clone)]
pub enum Acquire {
    /// The caller now holds the lock and must `release` it when done.
    Acquired,
    /// Another cycle holds the lock; carries who and since when.
    Busy {
        cycle_id: String,
        holder: String,
        acquired_at: String,
    },
}

/// Create the lock table if absent. Idempotent; safe to call before each acquire
/// (no migration registration needed — the table is engine-internal bookkeeping).
pub async fn ensure_run_lock_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS optimizer_cycle_lock (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            cycle_id TEXT NOT NULL,
            holder TEXT NOT NULL,
            acquired_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .context("create optimizer_cycle_lock")?;
    Ok(())
}

/// Attempt to acquire the workspace cycle lock for `cycle_id`, identified by
/// `holder` (e.g. "cli:alice" or "dashboard"). Atomic: succeeds when the lock is
/// free or held by a stale (dead) holder; otherwise returns [`Acquire::Busy`].
pub async fn try_acquire(
    pool: &SqlitePool,
    cycle_id: &str,
    holder: &str,
    now: DateTime<Utc>,
) -> Result<Acquire> {
    ensure_run_lock_schema(pool).await?;
    let now_str = now.to_rfc3339();
    let stale_cutoff = (now - chrono::Duration::seconds(STALE_AFTER_SECS)).to_rfc3339();

    // One atomic upsert: insert when absent, or take over when the existing row
    // is older than the stale cutoff. When the row is fresh, the WHERE makes the
    // conflict-update a no-op (0 rows affected) → Busy.
    let res = sqlx::query(
        "INSERT INTO optimizer_cycle_lock (id, cycle_id, holder, acquired_at) \
         VALUES (1, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
            cycle_id = excluded.cycle_id, \
            holder = excluded.holder, \
            acquired_at = excluded.acquired_at \
         WHERE optimizer_cycle_lock.acquired_at < ?",
    )
    .bind(cycle_id)
    .bind(holder)
    .bind(&now_str)
    .bind(&stale_cutoff)
    .execute(pool)
    .await
    .context("acquire optimizer_cycle_lock")?;

    if res.rows_affected() > 0 {
        return Ok(Acquire::Acquired);
    }

    // Busy — report the current holder.
    let row = sqlx::query("SELECT cycle_id, holder, acquired_at FROM optimizer_cycle_lock WHERE id = 1")
        .fetch_optional(pool)
        .await
        .context("read optimizer_cycle_lock holder")?;
    match row {
        Some(r) => Ok(Acquire::Busy {
            cycle_id: r.try_get("cycle_id").unwrap_or_default(),
            holder: r.try_get("holder").unwrap_or_default(),
            acquired_at: r.try_get("acquired_at").unwrap_or_default(),
        }),
        // Raced with a release between the upsert and this read — treat as free
        // on the next attempt rather than inventing a holder.
        None => Ok(Acquire::Busy {
            cycle_id: String::new(),
            holder: "(unknown)".into(),
            acquired_at: now_str,
        }),
    }
}

/// Release the lock iff `cycle_id` still holds it. Best-effort and idempotent.
pub async fn release(pool: &SqlitePool, cycle_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM optimizer_cycle_lock WHERE id = 1 AND cycle_id = ?")
        .bind(cycle_id)
        .execute(pool)
        .await
        .context("release optimizer_cycle_lock")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn acquire_is_exclusive_and_releasable() {
        let pool = pool().await;
        let now = Utc::now();

        // First acquire wins.
        assert!(matches!(
            try_acquire(&pool, "cycle-A", "cli", now).await.unwrap(),
            Acquire::Acquired
        ));

        // Second concurrent acquire is refused with the holder reported.
        match try_acquire(&pool, "cycle-B", "dashboard", now).await.unwrap() {
            Acquire::Busy { cycle_id, holder, .. } => {
                assert_eq!(cycle_id, "cycle-A");
                assert_eq!(holder, "cli");
            }
            Acquire::Acquired => panic!("second acquire must be refused while A holds the lock"),
        }

        // After release, a new cycle can acquire.
        release(&pool, "cycle-A").await.unwrap();
        assert!(matches!(
            try_acquire(&pool, "cycle-B", "dashboard", now).await.unwrap(),
            Acquire::Acquired
        ));
    }

    #[tokio::test]
    async fn stale_lock_is_taken_over() {
        let pool = pool().await;
        let long_ago = Utc::now() - chrono::Duration::hours(3);
        // A dead holder acquired 3h ago (> STALE_AFTER).
        assert!(matches!(
            try_acquire(&pool, "dead-cycle", "cli", long_ago).await.unwrap(),
            Acquire::Acquired
        ));
        // A fresh cycle takes over the stale lock.
        assert!(matches!(
            try_acquire(&pool, "fresh-cycle", "dashboard", Utc::now()).await.unwrap(),
            Acquire::Acquired
        ));
    }
}

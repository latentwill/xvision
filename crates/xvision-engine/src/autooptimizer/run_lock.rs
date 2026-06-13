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

/// Backstop stale window keyed on `acquired_at` (never refreshed): a lock this
/// old is taken over even with no heartbeat signal at all. Comfortably longer
/// than any real cycle (the observed runaway was ~27 min). Covers foreign-host
/// holders whose PID and heartbeat we can't reason about.
const STALE_AFTER_SECS: i64 = 2 * 60 * 60;

/// GH #967: a holder that hasn't written a `heartbeat` within this window is
/// treated as dead and its lock auto-cleared on the next acquire — without
/// waiting out the 2h `acquired_at` backstop. This is the normal kill→restart
/// recovery path (a SIGKILL'd/OOM'd run can't refresh its heartbeat). The
/// engine emits periodic `CycleProgressEvent::Heartbeat`s during a live cycle,
/// so a >30s heartbeat gap reliably means the holder is gone. Override with
/// `XVN_OPTIMIZER_LOCK_HEARTBEAT_STALE_SECS`.
pub const HEARTBEAT_STALE_AFTER_SECS: i64 = 30;

/// Resolve the heartbeat-stale window, honoring the env override.
fn heartbeat_stale_after_secs() -> i64 {
    std::env::var("XVN_OPTIMIZER_LOCK_HEARTBEAT_STALE_SECS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(HEARTBEAT_STALE_AFTER_SECS)
}

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

/// Details of a stale lock that was auto-cleared while acquiring (GH #967), so
/// the caller can surface a `stale_lock_cleared` warning to the operator.
#[derive(Debug, Clone)]
pub struct ReclaimedLock {
    /// The cycle id of the dead prior holder whose lock was cleared.
    pub prior_cycle: String,
    /// How long (seconds) the prior holder had been silent (heartbeat age, or
    /// acquired-at age when no heartbeat was recorded).
    pub age_s: i64,
    /// Why it was judged stale: "heartbeat" (no recent heartbeat) or
    /// "dead_pid" (holder process not alive on this host).
    pub reason: String,
}

/// The full result of [`try_acquire`]: the acquire verdict plus, when a stale
/// prior lock had to be reclaimed first, the details of what was cleared.
#[derive(Debug, Clone)]
pub struct AcquireOutcome {
    pub acquire: Acquire,
    pub reclaimed: Option<ReclaimedLock>,
}

impl AcquireOutcome {
    fn acquired(reclaimed: Option<ReclaimedLock>) -> Self {
        Self {
            acquire: Acquire::Acquired,
            reclaimed,
        }
    }
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

    // B10: add the `pid` column on existing tables that predate it. SQLite has no
    // idempotent `ADD COLUMN IF NOT EXISTS`, so guard with a column-exists check
    // (mirrors the table_has_column pattern in api/mod.rs).
    if !lock_table_has_column(pool, "pid").await? {
        sqlx::query("ALTER TABLE optimizer_cycle_lock ADD COLUMN pid INTEGER")
            .execute(pool)
            .await
            .context("add pid column to optimizer_cycle_lock")?;
    }
    // GH #967: `last_heartbeat` powers the short-window auto-clear. Added the
    // same guarded way for tables that predate it.
    if !lock_table_has_column(pool, "last_heartbeat").await? {
        sqlx::query("ALTER TABLE optimizer_cycle_lock ADD COLUMN last_heartbeat TEXT")
            .execute(pool)
            .await
            .context("add last_heartbeat column to optimizer_cycle_lock")?;
    }
    Ok(())
}

/// True if `optimizer_cycle_lock` has the named column.
async fn lock_table_has_column(pool: &SqlitePool, column: &str) -> Result<bool> {
    let rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(optimizer_cycle_lock)")
            .fetch_all(pool)
            .await
            .context("pragma table_info optimizer_cycle_lock")?;
    Ok(rows.iter().any(|(_, name, _, _, _, _)| name == column))
}

/// Whether a process with `pid` is currently alive on this host. Best-effort:
/// used only to reclaim locks abandoned by a dead local holder. Foreign-host
/// PIDs are not resolvable here and fall back to the 2h stale window.
fn pid_is_alive(pid: i64) -> bool {
    use sysinfo::System;
    let Ok(pid_u32) = u32::try_from(pid) else {
        // Out-of-range PID can never be a live local process.
        return false;
    };
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.process(sysinfo::Pid::from_u32(pid_u32)).is_some()
}

/// Attempt to acquire the workspace cycle lock for `cycle_id`, identified by
/// `holder` (e.g. "cli:alice" or "dashboard"). Atomic: succeeds when the lock is
/// free or held by a stale (dead) holder; otherwise returns [`Acquire::Busy`].
pub async fn try_acquire(
    pool: &SqlitePool,
    cycle_id: &str,
    holder: &str,
    now: DateTime<Utc>,
) -> Result<AcquireOutcome> {
    ensure_run_lock_schema(pool).await?;
    let now_str = now.to_rfc3339();
    let stale_cutoff = (now - chrono::Duration::seconds(STALE_AFTER_SECS)).to_rfc3339();
    // B10: capture our PID inside try_acquire so neither caller can forget to
    // record it; this is what dead-holder detection keys off of.
    let our_pid = std::process::id() as i64;

    // One atomic upsert: insert when absent, or take over when the existing row
    // is older than the stale cutoff. When the row is fresh, the WHERE makes the
    // conflict-update a no-op (0 rows affected) → Busy.
    if upsert_lock(pool, cycle_id, holder, &now_str, our_pid, &stale_cutoff).await? {
        return Ok(AcquireOutcome::acquired(None));
    }

    // Busy — read the current holder (including its PID and heartbeat).
    let row = sqlx::query(
        "SELECT cycle_id, holder, acquired_at, pid, last_heartbeat \
         FROM optimizer_cycle_lock WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .context("read optimizer_cycle_lock holder")?;
    let Some(r) = row else {
        // Raced with a release between the upsert and this read — treat as free
        // on the next attempt rather than inventing a holder.
        return Ok(AcquireOutcome {
            acquire: Acquire::Busy {
                cycle_id: String::new(),
                holder: "(unknown)".into(),
                acquired_at: now_str,
            },
            reclaimed: None,
        });
    };

    let holder_cycle: String = r.try_get("cycle_id").unwrap_or_default();
    let holder_name: String = r.try_get("holder").unwrap_or_default();
    let holder_at: String = r.try_get("acquired_at").unwrap_or_default();
    let holder_pid: Option<i64> = r.try_get("pid").ok().flatten();
    let holder_hb: Option<String> = r.try_get("last_heartbeat").ok().flatten();

    // GH #967: heartbeat-stale reclaim. The 2h backstop is too slow for the
    // normal kill→restart loop, and dead-PID detection fails when the PID was
    // reused (common in containers, low PID space). A holder that hasn't
    // refreshed its heartbeat within the short window is treated as dead. The
    // signal is `last_heartbeat`, falling back to `acquired_at` when the holder
    // never wrote one. Foreign-host clocks aside, both are this workspace's DB.
    let hb_ref = holder_hb.as_deref().unwrap_or(holder_at.as_str());
    if let Ok(hb_at) = DateTime::parse_from_rfc3339(hb_ref) {
        let age_s = (now - hb_at.with_timezone(&Utc)).num_seconds();
        if age_s > heartbeat_stale_after_secs() {
            sqlx::query("DELETE FROM optimizer_cycle_lock WHERE id = 1 AND cycle_id = ?")
                .bind(&holder_cycle)
                .execute(pool)
                .await
                .context("clear heartbeat-stale optimizer_cycle_lock")?;
            if upsert_lock(pool, cycle_id, holder, &now_str, our_pid, &stale_cutoff).await? {
                return Ok(AcquireOutcome::acquired(Some(ReclaimedLock {
                    prior_cycle: holder_cycle,
                    age_s,
                    reason: "heartbeat".into(),
                })));
            }
        }
    }

    // B10: dead-holder reclaim. The stale-window upsert above does NOT fire for a
    // FRESH row left by a process that was SIGKILL'd/OOM'd seconds ago. If the
    // holder recorded a PID and that PID is dead on THIS host, drop the row and
    // retry the upsert once. ON CONFLICT(id) still guarantees two live processes
    // can't both win the retry. Foreign-host PIDs (unresolvable here) fall back
    // to the 2h stale backstop.
    if let Some(pid) = holder_pid {
        if !pid_is_alive(pid) {
            sqlx::query("DELETE FROM optimizer_cycle_lock WHERE id = 1 AND pid = ?")
                .bind(pid)
                .execute(pool)
                .await
                .context("clear dead-holder optimizer_cycle_lock")?;
            if upsert_lock(pool, cycle_id, holder, &now_str, our_pid, &stale_cutoff).await? {
                let age_s = DateTime::parse_from_rfc3339(&holder_at)
                    .map(|t| (now - t.with_timezone(&Utc)).num_seconds())
                    .unwrap_or(0);
                return Ok(AcquireOutcome::acquired(Some(ReclaimedLock {
                    prior_cycle: holder_cycle,
                    age_s,
                    reason: "dead_pid".into(),
                })));
            }
        }
    }

    Ok(AcquireOutcome {
        acquire: Acquire::Busy {
            cycle_id: holder_cycle,
            holder: holder_name,
            acquired_at: holder_at,
        },
        reclaimed: None,
    })
}

/// GH #967: refresh the heartbeat for the lock held by `cycle_id`. Called
/// periodically by a live cycle (on each progress event) so that a competing
/// acquire can distinguish a live holder from a killed one. Best-effort and
/// scoped to the holder's own `cycle_id`, so it never refreshes a lock that has
/// already been reclaimed by someone else.
pub async fn heartbeat(pool: &SqlitePool, cycle_id: &str, now: DateTime<Utc>) -> Result<()> {
    sqlx::query("UPDATE optimizer_cycle_lock SET last_heartbeat = ? WHERE id = 1 AND cycle_id = ?")
        .bind(now.to_rfc3339())
        .bind(cycle_id)
        .execute(pool)
        .await
        .context("update optimizer_cycle_lock heartbeat")?;
    Ok(())
}

/// The single atomic acquire upsert. Returns `true` when this process now holds
/// the lock (inserted fresh or took over a stale row).
async fn upsert_lock(
    pool: &SqlitePool,
    cycle_id: &str,
    holder: &str,
    now_str: &str,
    pid: i64,
    stale_cutoff: &str,
) -> Result<bool> {
    // On acquire, seed `last_heartbeat = acquired_at` so a brand-new lock has a
    // fresh heartbeat (GH #967) and is never mistaken for stale before its
    // holder writes the first explicit heartbeat.
    let res = sqlx::query(
        "INSERT INTO optimizer_cycle_lock (id, cycle_id, holder, acquired_at, pid, last_heartbeat) \
         VALUES (1, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
            cycle_id = excluded.cycle_id, \
            holder = excluded.holder, \
            acquired_at = excluded.acquired_at, \
            pid = excluded.pid, \
            last_heartbeat = excluded.last_heartbeat \
         WHERE optimizer_cycle_lock.acquired_at < ?",
    )
    .bind(cycle_id)
    .bind(holder)
    .bind(now_str)
    .bind(pid)
    .bind(now_str)
    .bind(stale_cutoff)
    .execute(pool)
    .await
    .context("acquire optimizer_cycle_lock")?;
    Ok(res.rows_affected() > 0)
}

/// B10: unconditionally clear the workspace cycle lock, whatever holds it.
/// Returns the `cycle_id` that was cleared (if any) so the caller can report it.
/// Used by `xvn optimizer unlock` as the manual escape hatch when an orphaned
/// lock is wedged on a foreign host (where dead-PID detection can't help) and
/// the operator does not want to wait out the 2h stale window.
pub async fn force_clear(pool: &SqlitePool) -> Result<Option<String>> {
    ensure_run_lock_schema(pool).await?;
    let held: Option<String> = sqlx::query_scalar("SELECT cycle_id FROM optimizer_cycle_lock WHERE id = 1")
        .fetch_optional(pool)
        .await
        .context("read optimizer_cycle_lock before force-clear")?;
    sqlx::query("DELETE FROM optimizer_cycle_lock WHERE id = 1")
        .execute(pool)
        .await
        .context("force-clear optimizer_cycle_lock")?;
    Ok(held)
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
            try_acquire(&pool, "cycle-A", "cli", now).await.unwrap().acquire,
            Acquire::Acquired
        ));

        // Second concurrent acquire is refused with the holder reported.
        match try_acquire(&pool, "cycle-B", "dashboard", now)
            .await
            .unwrap()
            .acquire
        {
            Acquire::Busy { cycle_id, holder, .. } => {
                assert_eq!(cycle_id, "cycle-A");
                assert_eq!(holder, "cli");
            }
            Acquire::Acquired => panic!("second acquire must be refused while A holds the lock"),
        }

        // After release, a new cycle can acquire.
        release(&pool, "cycle-A").await.unwrap();
        assert!(matches!(
            try_acquire(&pool, "cycle-B", "dashboard", now)
                .await
                .unwrap()
                .acquire,
            Acquire::Acquired
        ));
    }

    #[tokio::test]
    async fn stale_lock_is_taken_over() {
        let pool = pool().await;
        let long_ago = Utc::now() - chrono::Duration::hours(3);
        // A dead holder acquired 3h ago (> STALE_AFTER).
        assert!(matches!(
            try_acquire(&pool, "dead-cycle", "cli", long_ago)
                .await
                .unwrap()
                .acquire,
            Acquire::Acquired
        ));
        // A fresh cycle takes over the stale lock.
        assert!(matches!(
            try_acquire(&pool, "fresh-cycle", "dashboard", Utc::now())
                .await
                .unwrap()
                .acquire,
            Acquire::Acquired
        ));
    }

    // GH #967: a lock whose holder stopped writing heartbeats (killed
    // mid-cycle, PID since reused by an unrelated live process so dead-PID
    // detection can't help) is auto-cleared once the heartbeat goes stale.
    #[tokio::test]
    async fn heartbeat_stale_lock_is_auto_cleared() {
        let pool = pool().await;
        let now = Utc::now();

        // Holder acquires and writes a heartbeat well inside the 2h acquired-at
        // window, so the acquired-at backstop does NOT fire.
        let acquired = try_acquire(&pool, "wedged-cycle", "cli:ghost", now)
            .await
            .unwrap();
        assert!(matches!(acquired.acquire, Acquire::Acquired));

        // Forge a live but mismatched PID so dead-PID reclaim can't fire, and
        // a heartbeat older than the heartbeat-stale window.
        let live_pid = std::process::id() as i64;
        let stale_hb = (now - chrono::Duration::seconds(HEARTBEAT_STALE_AFTER_SECS + 5)).to_rfc3339();
        sqlx::query("UPDATE optimizer_cycle_lock SET pid = ?, last_heartbeat = ? WHERE id = 1")
            .bind(live_pid)
            .bind(&stale_hb)
            .execute(&pool)
            .await
            .unwrap();

        // A new run a few seconds later (still inside the 2h acquired-at window)
        // must reclaim the lock because the heartbeat is stale, and report it.
        let later = now + chrono::Duration::seconds(2);
        let outcome = try_acquire(&pool, "fresh-cycle", "cli:operator", later)
            .await
            .unwrap();
        assert!(
            matches!(outcome.acquire, Acquire::Acquired),
            "heartbeat-stale lock must be reclaimed"
        );
        let reclaimed = outcome
            .reclaimed
            .expect("a stale-heartbeat reclaim must be reported so the CLI can emit an event");
        assert_eq!(reclaimed.prior_cycle, "wedged-cycle");
        assert!(reclaimed.age_s >= HEARTBEAT_STALE_AFTER_SECS);
    }

    #[tokio::test]
    async fn heartbeat_keeps_a_live_lock_held() {
        let pool = pool().await;
        let now = Utc::now();
        let acquired = try_acquire(&pool, "live-cycle", "cli:a", now).await.unwrap();
        assert!(matches!(acquired.acquire, Acquire::Acquired));

        // Holder refreshes its heartbeat just now.
        heartbeat(&pool, "live-cycle", now + chrono::Duration::seconds(20))
            .await
            .unwrap();

        // A competing acquire shortly after sees a fresh heartbeat → Busy.
        let outcome = try_acquire(&pool, "intruder", "cli:b", now + chrono::Duration::seconds(25))
            .await
            .unwrap();
        match outcome.acquire {
            Acquire::Busy { cycle_id, .. } => assert_eq!(cycle_id, "live-cycle"),
            Acquire::Acquired => panic!("a live, heartbeating lock must not be stolen"),
        }
    }
}

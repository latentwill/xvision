//! Schedule-based autooptimizer ticker — P5-W2.
//!
//! `tick_schedules` is called on a 60-second interval by the dashboard server.
//! For each enabled schedule it checks whether the `time_local` (format "HH:MM")
//! falls within a ±30-second window of the current local time. If due and no
//! active session exists, it calls `create_session`. If due but a session is
//! active, it appends a `schedule_skipped` event. Either way it stamps
//! `last_run_at` after firing.

use anyhow::Result;
use chrono::Local;
use sqlx::SqlitePool;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A row from `autooptimizer_schedules`.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct OptimizerSchedule {
    pub id: i64,
    pub enabled: bool,
    pub time_local: String,
    pub strategy_id: String,
    pub config_json: String,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal helper — load all enabled schedules
// ---------------------------------------------------------------------------

async fn load_enabled_schedules(pool: &SqlitePool) -> Result<Vec<OptimizerSchedule>> {
    Ok(sqlx::query_as::<_, OptimizerSchedule>(
        "SELECT id, enabled, time_local, strategy_id, config_json, last_run_at, next_run_at \
             FROM autooptimizer_schedules WHERE enabled = 1",
    )
    .fetch_all(pool)
    .await?)
}

// ---------------------------------------------------------------------------
// Due-time check — separated so tests can call it directly
// ---------------------------------------------------------------------------

/// Returns `true` if `now_hm` (hours × 60 + minutes in local time) is within
/// ±30 seconds of the schedule's `time_local` ("HH:MM").
///
/// `now_secs` is the current seconds-within-the-minute (0..60). This is
/// factored out so tests can pass a deterministic value.
pub fn is_due(time_local: &str, now_hour: u32, now_minute: u32, now_secs: u32) -> bool {
    let parts: Vec<&str> = time_local.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    let (Ok(h), Ok(m)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) else {
        return false;
    };
    // Total-seconds for the scheduled moment and the current moment.
    let sched_secs = h * 3600 + m * 60;
    let cur_secs = now_hour * 3600 + now_minute * 60 + now_secs;
    // Absolute difference, wrapping midnight (only relevant when checking near
    // midnight, but 30s window makes this rare).
    let diff = (cur_secs as i64 - sched_secs as i64).unsigned_abs();
    diff <= 30
}

// ---------------------------------------------------------------------------
// Core tick — injectable clock for testability
// ---------------------------------------------------------------------------

/// Process one tick of the schedule loop.
///
/// `now_fn` returns `(hour, minute, seconds_within_minute)` in local time.
/// Production callers pass `|| { let l = Local::now(); (l.hour(), l.minute(), l.second()) }`.
/// Tests inject a deterministic triple.
pub async fn tick_schedules_with_clock<F>(pool: &SqlitePool, now_fn: F) -> Result<()>
where
    F: Fn() -> (u32, u32, u32),
{
    let (now_h, now_m, now_s) = now_fn();
    let schedules = load_enabled_schedules(pool).await?;

    for sched in &schedules {
        if !is_due(&sched.time_local, now_h, now_m, now_s) {
            continue;
        }

        // Check whether a session is already active.
        let active = super::session::get_active_session(pool).await?;
        let now_ts = chrono::Utc::now().to_rfc3339();

        if active.is_some() {
            // Log a skip event. We use a deterministic "sched-<id>" fake
            // session_id so the event is attributable without an active session.
            let session_key = format!("sched-{}", sched.id);
            let payload = serde_json::json!({ "reason": "session_active" }).to_string();
            super::events_store::append_event(pool, &session_key, None, "schedule_skipped", &payload).await?;
        } else {
            // Fire: create a new session.
            super::session::create_session(pool, &sched.strategy_id, &sched.config_json, "once", None)
                .await?;
        }

        // Stamp last_run_at regardless of whether we fired or skipped.
        sqlx::query("UPDATE autooptimizer_schedules SET last_run_at = ? WHERE id = ?")
            .bind(&now_ts)
            .bind(sched.id)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Production tick — uses the real local clock.
pub async fn tick_schedules(pool: &SqlitePool) -> Result<()> {
    use chrono::Timelike;
    tick_schedules_with_clock(pool, || {
        let l = Local::now();
        (l.hour(), l.minute(), l.second())
    })
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Build an in-memory SQLite pool with the tables needed by the scheduler.
    async fn open_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();

        // Migration 059: autooptimizer_schedules
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS autooptimizer_schedules (
              id           INTEGER PRIMARY KEY AUTOINCREMENT,
              enabled      INTEGER NOT NULL DEFAULT 1,
              time_local   TEXT NOT NULL,
              strategy_id  TEXT NOT NULL,
              config_json  TEXT NOT NULL,
              last_run_at  TEXT,
              next_run_at  TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Migration 057: session_state + events
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS autooptimizer_session_state (
              session_id        TEXT PRIMARY KEY,
              strategy_id       TEXT NOT NULL,
              config_json       TEXT NOT NULL,
              state             TEXT NOT NULL,
              mode              TEXT NOT NULL,
              cycles_planned    INTEGER,
              cycles_completed  INTEGER NOT NULL DEFAULT 0,
              kept_count        INTEGER NOT NULL DEFAULT 0,
              suspect_count     INTEGER NOT NULL DEFAULT 0,
              dropped_count     INTEGER NOT NULL DEFAULT 0,
              errored_count     INTEGER NOT NULL DEFAULT 0,
              error             TEXT,
              created_at        TEXT NOT NULL,
              started_at        TEXT,
              finished_at       TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS autooptimizer_events (
              seq          INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id   TEXT NOT NULL,
              cycle_id     TEXT,
              kind         TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              ts           TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    fn insert_schedule<'a>(
        pool: &'a SqlitePool,
        time_local: &'a str,
        strategy_id: &'a str,
        enabled: bool,
    ) -> impl std::future::Future<Output = i64> + 'a {
        async move {
            let row: (i64,) = sqlx::query_as(
                "INSERT INTO autooptimizer_schedules \
                 (enabled, time_local, strategy_id, config_json) \
                 VALUES (?, ?, ?, '{}') RETURNING id",
            )
            .bind(enabled as i64)
            .bind(time_local)
            .bind(strategy_id)
            .fetch_one(pool)
            .await
            .unwrap();
            row.0
        }
    }

    fn insert_active_session<'a>(
        pool: &'a SqlitePool,
        session_id: &'a str,
    ) -> impl std::future::Future<Output = ()> + 'a {
        async move {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO autooptimizer_session_state \
                 (session_id, strategy_id, config_json, state, mode, created_at) \
                 VALUES (?, 's1', '{}', 'running', 'once', ?)",
            )
            .bind(session_id)
            .bind(&now)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    // ── is_due unit tests ────────────────────────────────────────────────────

    #[test]
    fn test_is_due_exact_match() {
        // schedule at 14:30, current time = 14:30:00 → due
        assert!(is_due("14:30", 14, 30, 0));
    }

    #[test]
    fn test_is_due_within_window() {
        // ±30s — e.g. 14:30:28 is still within 30s of 14:30:00
        assert!(is_due("14:30", 14, 30, 28));
        assert!(is_due("14:30", 14, 29, 31)); // 29 seconds before
    }

    #[test]
    fn test_is_due_outside_window() {
        // 14:30:31 is 31 seconds past → NOT due
        assert!(!is_due("14:30", 14, 30, 31));
    }

    #[test]
    fn test_is_due_wrong_minute() {
        // completely different minute
        assert!(!is_due("14:30", 14, 45, 0));
    }

    // ── tick integration tests ───────────────────────────────────────────────

    /// test_schedule_ticker_fires_when_due:
    /// mock current time == schedule time_local, no active session → create_session called
    #[tokio::test]
    async fn test_schedule_ticker_fires_when_due() {
        let pool = open_pool().await;

        // Insert a schedule at 09:00.
        insert_schedule(&pool, "09:00", "strat-abc", true).await;

        // Tick with clock pinned to 09:00:00 — exactly on time.
        tick_schedules_with_clock(&pool, || (9, 0, 0)).await.unwrap();

        // A session should have been created.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_session_state")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "expected one session to be created");

        // Verify strategy_id and mode.
        let (strategy_id, mode): (String, String) =
            sqlx::query_as("SELECT strategy_id, mode FROM autooptimizer_session_state")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(strategy_id, "strat-abc");
        assert_eq!(mode, "once");

        // last_run_at should be stamped.
        let last_run: Option<String> =
            sqlx::query_scalar("SELECT last_run_at FROM autooptimizer_schedules WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(last_run.is_some(), "last_run_at should be set after firing");
    }

    /// test_schedule_ticker_skips_when_active:
    /// active session exists → schedule_skipped event logged, no new session
    #[tokio::test]
    async fn test_schedule_ticker_skips_when_active() {
        let pool = open_pool().await;

        // Active session pre-exists.
        insert_active_session(&pool, "existing-session").await;

        // Schedule at 10:00.
        insert_schedule(&pool, "10:00", "strat-xyz", true).await;

        // Tick with clock at 10:00:05 — within window.
        tick_schedules_with_clock(&pool, || (10, 0, 5)).await.unwrap();

        // Session count should still be 1 (no new session created).
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_session_state")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "no new session should have been created");

        // A schedule_skipped event should exist.
        let event_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_events WHERE kind = 'schedule_skipped'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(event_count, 1, "expected one schedule_skipped event");

        // Verify the event payload contains reason="session_active".
        let payload: String = sqlx::query_scalar(
            "SELECT payload_json FROM autooptimizer_events WHERE kind = 'schedule_skipped'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            payload.contains("session_active"),
            "payload should contain reason=session_active, got: {payload}"
        );

        // last_run_at should still be stamped (we did attempt to fire).
        let last_run: Option<String> =
            sqlx::query_scalar("SELECT last_run_at FROM autooptimizer_schedules WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(last_run.is_some(), "last_run_at should be set even on skip");
    }

    /// Disabled schedules are ignored even when due.
    #[tokio::test]
    async fn test_disabled_schedule_not_fired() {
        let pool = open_pool().await;

        insert_schedule(&pool, "11:00", "strat-dis", false).await;

        tick_schedules_with_clock(&pool, || (11, 0, 0)).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_session_state")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "disabled schedule should not fire");
    }

    /// Not-due schedules are ignored.
    #[tokio::test]
    async fn test_not_due_schedule_not_fired() {
        let pool = open_pool().await;

        insert_schedule(&pool, "12:00", "strat-nd", true).await;

        // Clock is at 14:00:00 — not due.
        tick_schedules_with_clock(&pool, || (14, 0, 0)).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_session_state")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "not-due schedule should not fire");
    }
}

//! OptimizerSession entity and state-machine helpers.
//!
//! State transitions:
//! ```text
//! queued -> running
//! running -> paused, cancelling, finished, failed
//! paused  -> running, cancelling, failed
//! cancelling -> cancelled, failed
//! (terminal: cancelled, finished, failed)
//! any -> failed  (crash recovery)
//! ```

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;

// ---------------------------------------------------------------------------
// Entity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct OptimizerSession {
    pub session_id: String,
    pub strategy_id: String,
    pub config_json: String,
    pub state: String,
    pub mode: String,
    pub cycles_planned: Option<i64>,
    pub cycles_completed: i64,
    pub kept_count: i64,
    pub suspect_count: i64,
    pub dropped_count: i64,
    pub errored_count: i64,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_legal_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("queued", "running")
            | ("running", "paused")
            | ("running", "cancelling")
            | ("running", "finished")
            | ("running", "failed")
            | ("paused", "running")
            | ("paused", "cancelling")
            | ("paused", "failed")
            | ("cancelling", "cancelled")
            | ("cancelling", "failed")
            // R5: a session that recorded a `failed` may still be sealed
            // terminal — exit 0 with the error preserved — instead of an
            // illegal-transition crash (the cosmetic EXIT=5). Covers a SIGTERM
            // or a healthy teardown arriving after a `failed` was recorded.
            | ("failed", "finished")
            | ("failed", "cancelling")
            | (_, "failed") // crash recovery: any -> failed
    )
}

fn is_terminal(state: &str) -> bool {
    matches!(state, "finished" | "cancelled" | "failed")
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Ensure the `autooptimizer_session_state` and `autooptimizer_events` tables
/// exist (GH #968). The CLI optimize path opens the shared `xvn.db` via
/// `ensure_lineage_schema` and does NOT run `sqlx::migrate!`, so on a
/// CLI-only / fresh workspace these tables (created by migration 057 for the
/// dashboard) would be absent and `create_session` / event persistence would
/// fail. Idempotent `CREATE TABLE IF NOT EXISTS`; a no-op on already-migrated
/// DBs. The `mode` CHECK matches migration 057 — unlimited "fire-and-forget"
/// runs are stored as `n_experiments` with `cycles_planned = NULL`, so no new
/// mode value is needed here.
pub async fn ensure_session_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS autooptimizer_session_state (
          session_id        TEXT PRIMARY KEY,
          strategy_id       TEXT NOT NULL,
          config_json       TEXT NOT NULL,
          state             TEXT NOT NULL CHECK(state IN ('queued','running','paused','cancelling','cancelled','finished','failed')),
          mode              TEXT NOT NULL CHECK(mode IN ('once','n_experiments','until_budget')),
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
    .execute(pool)
    .await?;
    // Upgrade guard: add `errored_count` to existing DBs that predate this column.
    // SQLite has no ADD COLUMN IF NOT EXISTS, so we probe PRAGMA table_info first.
    let col_exists: bool = {
        let rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as("PRAGMA table_info(autooptimizer_session_state)")
                .fetch_all(pool)
                .await?;
        rows.iter().any(|(_, name, _, _, _, _)| name == "errored_count")
    };
    if !col_exists {
        sqlx::query(
            "ALTER TABLE autooptimizer_session_state \
             ADD COLUMN errored_count INTEGER NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await?;
    }
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_aoss_state ON autooptimizer_session_state(state)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_aoss_created ON autooptimizer_session_state(created_at)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS autooptimizer_events (
          seq         INTEGER PRIMARY KEY AUTOINCREMENT,
          session_id  TEXT NOT NULL,
          cycle_id    TEXT,
          kind        TEXT NOT NULL,
          payload_json TEXT NOT NULL,
          ts          TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_aoe_session ON autooptimizer_events(session_id)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Create a new session row in state `running`, generating a fresh id.
pub async fn create_session(
    pool: &SqlitePool,
    strategy_id: &str,
    config_json: &str,
    mode: &str,
    cycles_planned: Option<i64>,
) -> Result<String> {
    let session_id = ulid::Ulid::new().to_string();
    create_session_with_id(pool, &session_id, strategy_id, config_json, mode, cycles_planned).await?;
    Ok(session_id)
}

/// Create a new session row in state `running` with a caller-chosen id. The CLI
/// uses this so the session-state id is the SAME as the workspace cycle-lock id
/// (GH #968): one id ties the lock, the live session row, and the persisted
/// events together.
pub async fn create_session_with_id(
    pool: &SqlitePool,
    session_id: &str,
    strategy_id: &str,
    config_json: &str,
    mode: &str,
    cycles_planned: Option<i64>,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO autooptimizer_session_state \
         (session_id, strategy_id, config_json, state, mode, cycles_planned, created_at) \
         VALUES (?,?,?,?,?,?,?)",
    )
    .bind(session_id)
    .bind(strategy_id)
    .bind(config_json)
    .bind("running")
    .bind(mode)
    .bind(cycles_planned)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Attempt a state transition. Returns `Err` on illegal transitions.
pub async fn transition_state(
    pool: &SqlitePool,
    session_id: &str,
    new_state: &str,
    error: Option<&str>,
) -> Result<()> {
    let current: String =
        sqlx::query_scalar("SELECT state FROM autooptimizer_session_state WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(pool)
            .await?;

    anyhow::ensure!(
        is_legal_transition(&current, new_state),
        "illegal state transition {} -> {}",
        current,
        new_state
    );

    let now = chrono::Utc::now().to_rfc3339();
    let finished_at: Option<String> = if is_terminal(new_state) {
        Some(now.clone())
    } else {
        None
    };

    // Set state + error + finished_at (only first terminal sets finished_at).
    sqlx::query(
        "UPDATE autooptimizer_session_state \
         SET state = ?, error = ?, finished_at = COALESCE(finished_at, ?) \
         WHERE session_id = ?",
    )
    .bind(new_state)
    .bind(error)
    .bind(finished_at)
    .bind(session_id)
    .execute(pool)
    .await?;

    // Set started_at only on the first transition to running.
    if new_state == "running" {
        sqlx::query(
            "UPDATE autooptimizer_session_state \
             SET started_at = COALESCE(started_at, ?) \
             WHERE session_id = ?",
        )
        .bind(now)
        .bind(session_id)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Called at startup: mark any in-flight sessions (running / paused /
/// cancelling / queued) as `failed` with `error = 'interrupted'`.
pub async fn mark_interrupted_sessions(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "UPDATE autooptimizer_session_state \
         SET state = 'failed', error = 'interrupted' \
         WHERE state IN ('running','paused','cancelling','queued')",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Return the first session in an active state (running or paused), if any.
pub async fn get_active_session(pool: &SqlitePool) -> Result<Option<OptimizerSession>> {
    Ok(sqlx::query_as::<_, OptimizerSession>(
        "SELECT * FROM autooptimizer_session_state \
         WHERE state IN ('running','paused') LIMIT 1",
    )
    .fetch_optional(pool)
    .await?)
}

/// Increment `cycles_completed` and the appropriate outcome counter.
pub async fn increment_cycle_completed(pool: &SqlitePool, session_id: &str, outcome: &str) -> Result<()> {
    let col = match outcome {
        "kept" => "kept_count",
        "suspect" => "suspect_count",
        "errored" => "errored_count",
        _ => "dropped_count",
    };
    sqlx::query(&format!(
        "UPDATE autooptimizer_session_state \
         SET cycles_completed = cycles_completed + 1, \
             {col} = {col} + 1 \
         WHERE session_id = ?",
    ))
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Session runner
// ---------------------------------------------------------------------------

/// Result returned by a single cycle invocation inside the session runner.
/// The caller (the injected `run_cycle_fn`) is responsible for running one
/// optimizer cycle and returning this summary.
#[derive(Debug, Clone)]
pub struct CycleRunOutcome {
    /// Gate verdict bucket: "kept", "suspect", or "dropped".
    pub outcome: String,
    /// Cumulative cost in USD across all cycles run so far in this session.
    /// Used to check `until_budget` exit conditions.
    pub cum_cost_usd: f64,
    /// Number of consecutive cycles that produced zero active (kept) nodes,
    /// starting from the beginning of the session.  Used to detect the
    /// sustained-no-pass floor.
    pub sustained_no_pass_cycles: u32,
}

/// Floor detection: returns `true` when the loosening schedule has been
/// exhausted — i.e. `sustained_no_pass_cycles` has exceeded the length of
/// `day_n_thresholds` (every step has been taken). When there is no schedule,
/// the floor is never reached.
pub fn loosening_floor_reached(thresholds: &[f64], sustained_no_pass_cycles: u32) -> bool {
    !thresholds.is_empty() && sustained_no_pass_cycles as usize > thresholds.len()
}

/// Run the session loop.
///
/// Drives `run_cycle_fn` in a loop, honouring cancel / pause flags and
/// the mode-specific exit conditions (once / n_experiments / until_budget /
/// sustained-no-pass floor).
///
/// `run_cycle_fn` is called once per iteration; it must be `Send` because
/// this function is intended to run inside a `tokio::spawn` task.
///
/// # Mode-specific exit conditions
/// - `once` — stop after 1 cycle.
/// - `n_experiments` — stop when `cycles_completed >= cycles_planned`. Passing
///   `cycles_planned = None` makes the count test `>= i64::MAX`, i.e. unlimited
///   "fire-and-forget" mode that runs until cancel / budget / floor (GH #965).
/// - `until_budget` — stop when `cum_cost_usd >= budget_cap`.
/// - (All modes) `budget_cap` is a hard ceiling: whenever it is finite, the
///   loop stops as soon as `cum_cost_usd >= budget_cap`, independent of mode.
/// - (All modes) sustained-no-pass floor — stop when the loosening
///   schedule has been fully exhausted.
pub async fn run_session<F, Fut>(
    pool: &SqlitePool,
    session_id: &str,
    mode: &str,
    cycles_planned: Option<i64>,
    budget_cap: Option<f64>,
    loosening_thresholds: Vec<f64>,
    max_consecutive_errors: u32,
    cost_so_far: Arc<dyn Fn() -> f64 + Send + Sync>,
    cancel_flag: Arc<AtomicBool>,
    pause_flag: Arc<AtomicBool>,
    run_cycle_fn: F,
) -> Result<()>
where
    F: Fn() -> Fut + Send,
    Fut: Future<Output = Result<CycleRunOutcome>> + Send,
{
    let budget = budget_cap.unwrap_or(f64::INFINITY);

    // R3: session-level breaker. Each cycle that errors counts; `max == 0`
    // disables it (never trips). A one-off cycle error is sealed `errored` and
    // the loop continues; only `max_consecutive_errors` consecutive cycle
    // failures stop the run (transition `failed` + a halt error).
    let mut session_breaker = crate::autooptimizer::cycle::ConsecutiveErrors::new(max_consecutive_errors);

    loop {
        // 1. Check cancel flag.
        if cancel_flag.load(Ordering::Relaxed) {
            transition_state(pool, session_id, "cancelling", None).await?;
            transition_state(pool, session_id, "cancelled", None).await?;
            return Ok(());
        }

        // 2. Check pause flag — wait in a 1s poll until cleared or cancelled.
        while pause_flag.load(Ordering::Relaxed) {
            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        // Re-check cancel after waking from pause.
        if cancel_flag.load(Ordering::Relaxed) {
            transition_state(pool, session_id, "cancelling", None).await?;
            transition_state(pool, session_id, "cancelled", None).await?;
            return Ok(());
        }

        // 3. Run one cycle — isolate ANY cycle-level failure (trader, mutator,
        //    judge, dispatch, DB) so one bad cycle never kills the session.
        let cycle_result = run_cycle_fn().await;

        // 4. Account for the cycle and update the session breaker. On error:
        //    seal this cycle as `errored`; if that trips the breaker (sustained
        //    failure) transition `failed` and bail with a halt error (the CLI
        //    maps it to a distinct exit code). A one-off error falls through to
        //    the normal mode/budget/floor exit checks (so e.g. `once` still
        //    stops after its single — errored — cycle) and exits 0.
        let (outcome_bucket, cum_cost_usd, sustained_no_pass) = match &cycle_result {
            Ok(o) => {
                session_breaker.record_success();
                (o.outcome.clone(), o.cum_cost_usd, o.sustained_no_pass_cycles)
            }
            Err(e) => {
                tracing::warn!(
                    target: "xvision::autooptimizer",
                    session_id,
                    error = %e,
                    "optimizer cycle errored; sealing as errored and continuing"
                );
                increment_cycle_completed(pool, session_id, "errored").await?;
                if session_breaker.record_failure() {
                    let msg = format!(
                        "optimizer halted: {max_consecutive_errors} consecutive cycle failures; \
                         last error: {e:#}"
                    );
                    transition_state(pool, session_id, "failed", Some(&msg)).await?;
                    anyhow::bail!(msg);
                }
                // Errored cycle already counted above. It produced no honest
                // no-pass signal (0), but it MAY have spent real money before
                // failing — read the live cumulative spend so the `--budget`
                // hard ceiling still trips even on a run where every cycle
                // errors (otherwise zeroing cost here defeats the cap and an
                // error loop with the breaker disabled spends unbounded).
                ("errored".to_string(), cost_so_far(), 0u32)
            }
        };

        // Count a *successful* cycle (the errored branch already incremented).
        if cycle_result.is_ok() {
            increment_cycle_completed(pool, session_id, &outcome_bucket).await?;
        }

        // 5. Fetch updated cycles_completed.
        let cycles_completed: i64 = sqlx::query_scalar(
            "SELECT cycles_completed FROM autooptimizer_session_state WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_one(pool)
        .await?;

        // 6. Budget is a hard ceiling in EVERY mode (GH #965). When no cap is
        //    set, `budget` is `f64::INFINITY` so this is never true.
        let budget_exceeded = cum_cost_usd >= budget;

        // 7. Check mode-specific exit conditions.
        let mode_done = match mode {
            "once" => true,
            "n_experiments" => {
                let planned = cycles_planned.unwrap_or(i64::MAX);
                cycles_completed >= planned
            }
            "until_budget" => budget_exceeded,
            // `continuous` (and any unknown mode) has no count limit — it stops
            // only via cancel, the universal budget ceiling, or the floor.
            _ => false,
        };

        // 8. Check sustained-no-pass floor.
        let floor_hit = loosening_floor_reached(&loosening_thresholds, sustained_no_pass);

        if mode_done || budget_exceeded || floor_hit {
            break;
        }
    }

    // Transition to finished.
    transition_state(pool, session_id, "finished", None).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    /// Build a fresh in-memory SQLite pool with migration 057 applied.
    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        // Run the migration SQL directly (same content as 057_autooptimizer_sessions.sql).
        let sql = include_str!("../../migrations/057_autooptimizer_sessions.sql");
        for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(stmt).execute(&pool).await.unwrap();
        }
        // Mirror the production CLI flow: the base 057 DDL is created, then
        // `ensure_session_schema`'s guarded ALTERs upgrade it with any newer
        // additive columns (e.g. `errored_count`). This keeps the test pool in
        // sync with what `open_and_migrate_db` produces, without editing the
        // committed 057 migration (which would break the sqlx::migrate! checksum).
        ensure_session_schema(&pool).await.unwrap();
        pool
    }

    // -----------------------------------------------------------------------
    // test_legal_transitions
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_legal_transitions() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-1", "{}", "once", Some(5))
            .await
            .unwrap();

        // running -> paused
        transition_state(&pool, &sid, "paused", None).await.unwrap();

        // paused -> running  (resume)
        transition_state(&pool, &sid, "running", None).await.unwrap();

        // running -> finished
        transition_state(&pool, &sid, "finished", None).await.unwrap();

        // finished_at should now be set
        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(row.state, "finished");
        assert!(row.finished_at.is_some(), "finished_at must be set");
        assert!(row.started_at.is_some(), "started_at must be set");
    }

    // -----------------------------------------------------------------------
    // test_illegal_transition
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_illegal_transition() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-1", "{}", "once", None)
            .await
            .unwrap();

        // Drive to finished first.
        transition_state(&pool, &sid, "finished", None).await.unwrap();

        // finished -> running must fail.
        let result = transition_state(&pool, &sid, "running", None).await;
        assert!(result.is_err(), "finished -> running should be illegal");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("illegal state transition"),
            "error message should describe the illegal transition, got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // test_crash_recovery
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_crash_recovery() {
        let pool = test_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        // Insert rows directly with various non-terminal states.
        for (id, state) in [
            ("01SESS_RUNNING", "running"),
            ("01SESS_PAUSED", "paused"),
            ("01SESS_CANCEL", "cancelling"),
            ("01SESS_QUEUED", "queued"),
        ] {
            sqlx::query(
                "INSERT INTO autooptimizer_session_state \
                 (session_id, strategy_id, config_json, state, mode, created_at) \
                 VALUES (?,?,?,?,?,?)",
            )
            .bind(id)
            .bind("strat-x")
            .bind("{}")
            .bind(state)
            .bind("once")
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Insert rows that are already terminal — they must NOT be touched.
        for (id, state) in [
            ("01SESS_FINISHED", "finished"),
            ("01SESS_CANCELLED", "cancelled"),
            ("01SESS_FAILED", "failed"),
        ] {
            sqlx::query(
                "INSERT INTO autooptimizer_session_state \
                 (session_id, strategy_id, config_json, state, mode, created_at) \
                 VALUES (?,?,?,?,?,?)",
            )
            .bind(id)
            .bind("strat-x")
            .bind("{}")
            .bind(state)
            .bind("once")
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        }

        mark_interrupted_sessions(&pool).await.unwrap();

        // Verify non-terminal rows were converted.
        for id in [
            "01SESS_RUNNING",
            "01SESS_PAUSED",
            "01SESS_CANCEL",
            "01SESS_QUEUED",
        ] {
            let row: OptimizerSession =
                sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(row.state, "failed", "{id} should be 'failed'");
            assert_eq!(
                row.error.as_deref(),
                Some("interrupted"),
                "{id} error should be 'interrupted'"
            );
        }

        // Verify terminal rows are unchanged.
        for (id, expected_state) in [
            ("01SESS_FINISHED", "finished"),
            ("01SESS_CANCELLED", "cancelled"),
            ("01SESS_FAILED", "failed"),
        ] {
            let row: OptimizerSession =
                sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(row.state, expected_state, "{id} state must not change");
        }
    }

    // -----------------------------------------------------------------------
    // test_get_active_session
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_get_active_session_none_when_empty() {
        let pool = test_pool().await;
        let active = get_active_session(&pool).await.unwrap();
        assert!(active.is_none(), "should return None when no sessions");
    }

    #[tokio::test]
    async fn test_get_active_session_returns_running() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-2", "{}", "once", None)
            .await
            .unwrap();

        let active = get_active_session(&pool).await.unwrap();
        assert!(active.is_some(), "should return Some for running session");
        assert_eq!(active.unwrap().session_id, sid);
    }

    #[tokio::test]
    async fn test_get_active_session_returns_paused() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-3", "{}", "once", None)
            .await
            .unwrap();

        // running -> paused
        transition_state(&pool, &sid, "paused", None).await.unwrap();

        let active = get_active_session(&pool).await.unwrap();
        assert!(active.is_some(), "should return Some for paused session");
        assert_eq!(active.unwrap().session_id, sid);
    }

    #[tokio::test]
    async fn test_get_active_session_none_after_finished() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-4", "{}", "once", None)
            .await
            .unwrap();
        transition_state(&pool, &sid, "finished", None).await.unwrap();

        let active = get_active_session(&pool).await.unwrap();
        assert!(active.is_none(), "finished session should not be active");
    }

    // -----------------------------------------------------------------------
    // test_increment_cycle_completed
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_increment_cycle_completed() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-5", "{}", "n_experiments", Some(10))
            .await
            .unwrap();

        increment_cycle_completed(&pool, &sid, "kept").await.unwrap();
        increment_cycle_completed(&pool, &sid, "suspect").await.unwrap();
        increment_cycle_completed(&pool, &sid, "dropped").await.unwrap();
        increment_cycle_completed(&pool, &sid, "dropped").await.unwrap();

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(row.cycles_completed, 4);
        assert_eq!(row.kept_count, 1);
        assert_eq!(row.suspect_count, 1);
        assert_eq!(row.dropped_count, 2);
    }

    // -----------------------------------------------------------------------
    // test_cancelling_path
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_cancelling_path() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-6", "{}", "once", None)
            .await
            .unwrap();

        // running -> cancelling -> cancelled
        transition_state(&pool, &sid, "cancelling", None).await.unwrap();
        transition_state(&pool, &sid, "cancelled", None).await.unwrap();

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(row.state, "cancelled");
        assert!(row.finished_at.is_some(), "cancelled must have finished_at");
    }

    // -----------------------------------------------------------------------
    // Session runner tests (P4-W2)
    // -----------------------------------------------------------------------

    /// Helper: make a no-cost CycleRunOutcome with the given outcome bucket.
    fn make_outcome(outcome: &str) -> CycleRunOutcome {
        CycleRunOutcome {
            outcome: outcome.to_string(),
            cum_cost_usd: 0.0,
            sustained_no_pass_cycles: 0,
        }
    }

    /// Helper: make a CycleRunOutcome with a specific cumulative cost.
    fn make_outcome_with_cost(outcome: &str, cum_cost_usd: f64) -> CycleRunOutcome {
        CycleRunOutcome {
            outcome: outcome.to_string(),
            cum_cost_usd,
            sustained_no_pass_cycles: 0,
        }
    }

    // -----------------------------------------------------------------------
    // test_once_mode_runs_one_cycle
    // -----------------------------------------------------------------------
    /// `once` mode: `run_session` with mode="once" must run exactly 1 cycle
    /// and leave the session in "finished" state.
    #[tokio::test]
    async fn test_once_mode_runs_one_cycle() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-once", "{}", "once", None)
            .await
            .unwrap();

        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));

        run_session(
            &pool,
            &sid,
            "once",
            None,
            None,
            vec![],
            0,                // max_consecutive_errors: session breaker disabled in this test
            Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || {
                counter.fetch_add(1, Ordering::Relaxed);
                let outcome = make_outcome("kept");
                async move { Ok(outcome) }
            },
        )
        .await
        .unwrap();

        assert_eq!(
            call_count.load(Ordering::Relaxed),
            1,
            "once mode must run exactly 1 cycle"
        );

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "finished");
        assert_eq!(row.cycles_completed, 1);
        assert_eq!(row.kept_count, 1);
    }

    // -----------------------------------------------------------------------
    // R3: cycle isolation + session-level breaker
    // -----------------------------------------------------------------------
    /// R3: a single cycle error must NOT crash the session. With the breaker
    /// disabled, `once` mode seals the cycle `errored`, finishes cleanly, and
    /// `run_session` returns Ok (the CLI then exits 0).
    #[tokio::test]
    async fn test_one_off_cycle_error_seals_errored_and_exits_clean() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-err1", "{}", "once", None)
            .await
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));

        let result = run_session(
            &pool,
            &sid,
            "once",
            None,
            None,
            vec![],
            0,                // breaker disabled
            Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || async move {
                Err::<CycleRunOutcome, anyhow::Error>(anyhow::anyhow!("simulated cycle failure"))
            },
        )
        .await;

        assert!(
            result.is_ok(),
            "a one-off cycle error must not crash the session: {result:?}"
        );
        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            row.state, "finished",
            "session sealed cleanly despite the errored cycle"
        );
        assert_eq!(row.cycles_completed, 1);
        assert_eq!(row.errored_count, 1, "the cycle is counted as errored");
    }

    /// R3: sustained cycle failure trips the session-level breaker, transitions
    /// the session to `failed`, and returns Err (the CLI maps it to the distinct
    /// `OptHalted` exit code). With the breaker at 2, the 2nd consecutive error
    /// halts the run.
    #[tokio::test]
    async fn test_sustained_cycle_errors_trip_breaker_and_fail() {
        let pool = test_pool().await;
        // High cycle plan so the loop keeps running until the breaker trips
        // first (mode is constrained to once|n_experiments|until_budget).
        let sid = create_session(&pool, "strat-err2", "{}", "n_experiments", Some(10))
            .await
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&calls);

        let result = run_session(
            &pool,
            &sid,
            "n_experiments",
            Some(10),
            None,
            vec![],
            2,                // trips on the 2nd consecutive error
            Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || {
                counter.fetch_add(1, Ordering::Relaxed);
                async move { Err::<CycleRunOutcome, anyhow::Error>(anyhow::anyhow!("boom")) }
            },
        )
        .await;

        assert!(result.is_err(), "sustained failure must halt the run");
        assert!(
            format!("{:#}", result.unwrap_err()).contains("consecutive cycle failures"),
            "halt error must name the breaker reason"
        );
        assert_eq!(
            calls.load(Ordering::Relaxed),
            2,
            "the breaker trips exactly on the 2nd consecutive error"
        );
        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "failed");
        assert_eq!(row.errored_count, 2);
        assert!(row.error.is_some(), "the failure reason is persisted");
    }

    /// R3 regression (adversarial review): an errored cycle must still honor the
    /// `--budget` hard ceiling. With the breaker disabled and every cycle
    /// erroring, the loop must terminate once the live cumulative spend crosses
    /// the cap — it must NOT spin forever (the cost is read from `cost_so_far`,
    /// not fabricated as 0).
    #[tokio::test]
    async fn test_budget_ceiling_trips_on_errored_cycles() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-budget", "{}", "n_experiments", Some(1_000_000))
            .await
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&calls);

        let result = run_session(
            &pool,
            &sid,
            "n_experiments",
            Some(1_000_000),
            Some(5.0), // budget cap $5
            vec![],
            0,                 // breaker DISABLED — only the budget can stop this
            Arc::new(|| 10.0), // live cumulative spend already over the cap
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || {
                counter.fetch_add(1, Ordering::Relaxed);
                async move { Err::<CycleRunOutcome, anyhow::Error>(anyhow::anyhow!("boom")) }
            },
        )
        .await;

        assert!(result.is_ok(), "budget-capped run must seal cleanly, not crash");
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "the budget ceiling must stop the run after the first over-budget errored cycle"
        );
        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "finished");
        assert_eq!(row.errored_count, 1);
    }

    // -----------------------------------------------------------------------
    // R5: terminal seal transitions out of `failed`
    // -----------------------------------------------------------------------
    /// R5: a session that recorded `failed` may still be sealed `finished`
    /// (seal-with-errors → exit 0) instead of an illegal-transition crash.
    #[tokio::test]
    async fn test_failed_to_finished_is_legal_terminal_seal() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-r5a", "{}", "once", None)
            .await
            .unwrap();
        transition_state(&pool, &sid, "failed", Some("boom"))
            .await
            .unwrap();
        transition_state(&pool, &sid, "finished", None)
            .await
            .expect("failed -> finished must be a legal terminal seal (R5)");
        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "finished");
        assert!(row.finished_at.is_some(), "finished_at must be set");
    }

    /// R5: a SIGTERM/cancel arriving after a recorded `failed` can still seal.
    #[tokio::test]
    async fn test_failed_to_cancelling_is_legal() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-r5b", "{}", "once", None)
            .await
            .unwrap();
        transition_state(&pool, &sid, "failed", Some("boom"))
            .await
            .unwrap();
        transition_state(&pool, &sid, "cancelling", None)
            .await
            .expect("failed -> cancelling must be legal (R5)");
        transition_state(&pool, &sid, "cancelled", None).await.unwrap();
        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "cancelled");
    }

    // -----------------------------------------------------------------------
    // test_n_experiments_mode
    // -----------------------------------------------------------------------
    /// `n_experiments` mode with `cycles_planned=3` must run exactly 3 cycles.
    #[tokio::test]
    async fn test_n_experiments_mode() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-n", "{}", "n_experiments", Some(3))
            .await
            .unwrap();

        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));

        run_session(
            &pool,
            &sid,
            "n_experiments",
            Some(3),
            None,
            vec![],
            0,                // max_consecutive_errors: session breaker disabled in this test
            Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || {
                counter.fetch_add(1, Ordering::Relaxed);
                let outcome = make_outcome("dropped");
                async move { Ok(outcome) }
            },
        )
        .await
        .unwrap();

        assert_eq!(
            call_count.load(Ordering::Relaxed),
            3,
            "n_experiments=3 must run exactly 3 cycles"
        );

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "finished");
        assert_eq!(row.cycles_completed, 3);
    }

    // -----------------------------------------------------------------------
    // test_until_budget_stops_when_exceeded
    // -----------------------------------------------------------------------
    /// `until_budget` mode: loop stops when `cum_cost_usd >= budget_cap`.
    /// The cycle function reports increasing cumulative cost.
    #[tokio::test]
    async fn test_until_budget_stops_when_exceeded() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-budget", "{}", "until_budget", None)
            .await
            .unwrap();

        // Each cycle adds $0.05. Budget cap is $0.12 → should stop after 3rd cycle
        // (cum = 0.05, 0.10, 0.15 where 0.15 >= 0.12).
        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);
        // Use u64 bits to share f64 across the closure (AtomicF64 isn't stable).
        let cum_bits = Arc::new(AtomicU64::new(0));
        let cum_shared = Arc::clone(&cum_bits);

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));

        run_session(
            &pool,
            &sid,
            "until_budget",
            None,
            Some(0.12),
            vec![],
            0,                // max_consecutive_errors: session breaker disabled in this test
            Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || {
                let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
                let cum = n as f64 * 0.05;
                cum_shared.store(cum.to_bits(), Ordering::Relaxed);
                let outcome = make_outcome_with_cost("dropped", cum);
                async move { Ok(outcome) }
            },
        )
        .await
        .unwrap();

        let calls = call_count.load(Ordering::Relaxed);
        assert!(
            calls >= 3,
            "must run at least 3 cycles before budget exceeded (got {calls})"
        );
        // Should not have run a 4th cycle since budget was exceeded after cycle 3.
        assert_eq!(
            calls, 3,
            "must stop exactly at cycle 3 when cum_cost >= budget (got {calls})"
        );

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "finished");
    }

    // -----------------------------------------------------------------------
    // test_cancel_stops_loop
    // -----------------------------------------------------------------------
    /// Setting the cancel flag before the run starts causes the session to
    /// transition to "cancelled" without running any cycles.
    #[tokio::test]
    async fn test_cancel_stops_loop() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-cancel", "{}", "n_experiments", Some(10))
            .await
            .unwrap();

        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let cancel = Arc::new(AtomicBool::new(true)); // pre-cancelled
        let pause = Arc::new(AtomicBool::new(false));

        run_session(
            &pool,
            &sid,
            "n_experiments",
            Some(10),
            None,
            vec![],
            0,                // max_consecutive_errors: session breaker disabled in this test
            Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || {
                counter.fetch_add(1, Ordering::Relaxed);
                let outcome = make_outcome("kept");
                async move { Ok(outcome) }
            },
        )
        .await
        .unwrap();

        assert_eq!(
            call_count.load(Ordering::Relaxed),
            0,
            "pre-cancelled session must not run any cycles"
        );

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "cancelled");
    }

    // -----------------------------------------------------------------------
    // test_pause_resumes_loop
    // -----------------------------------------------------------------------
    /// Pause flag causes the loop to wait; clearing it lets the loop continue
    /// and finish normally (once mode).
    #[tokio::test]
    async fn test_pause_resumes_loop() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-pause", "{}", "once", None)
            .await
            .unwrap();

        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(true)); // start paused

        // Spawn a task that clears the pause flag after a tiny delay.
        let pause_for_clear = Arc::clone(&pause);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            pause_for_clear.store(false, Ordering::Relaxed);
        });

        run_session(
            &pool,
            &sid,
            "once",
            None,
            None,
            vec![],
            0,                // max_consecutive_errors: session breaker disabled in this test
            Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || {
                counter.fetch_add(1, Ordering::Relaxed);
                let outcome = make_outcome("kept");
                async move { Ok(outcome) }
            },
        )
        .await
        .unwrap();

        assert_eq!(
            call_count.load(Ordering::Relaxed),
            1,
            "pause/resume must still run the 1 cycle"
        );

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "finished");
    }

    // -----------------------------------------------------------------------
    // test_no_pass_floor_stops_loop
    // -----------------------------------------------------------------------
    /// When the loosening schedule is exhausted (sustained_no_pass_cycles
    /// exceeds the schedule length), the loop stops and finishes normally.
    #[tokio::test]
    async fn test_no_pass_floor_stops_loop() {
        let pool = test_pool().await;
        // Use n_experiments=100 so mode alone would never stop it.
        let sid = create_session(&pool, "strat-floor", "{}", "n_experiments", Some(100))
            .await
            .unwrap();

        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));

        // Schedule has 2 steps. Floor is exceeded when sustained_no_pass_cycles > 2.
        // We return sustained_no_pass_cycles=3 immediately → floor hit on first cycle.
        run_session(
            &pool,
            &sid,
            "n_experiments",
            Some(100),
            None,
            vec![0.05, 0.02], // 2-step loosening schedule
            0,                // max_consecutive_errors: session breaker disabled in this test
            Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || {
                counter.fetch_add(1, Ordering::Relaxed);
                // Return floor-exceeded immediately (3 > 2 steps).
                let outcome = CycleRunOutcome {
                    outcome: "dropped".to_string(),
                    cum_cost_usd: 0.0,
                    sustained_no_pass_cycles: 3,
                };
                async move { Ok(outcome) }
            },
        )
        .await
        .unwrap();

        assert_eq!(
            call_count.load(Ordering::Relaxed),
            1,
            "floor stop must trigger after first cycle reports floor-exceeded"
        );

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "finished");
    }

    // -----------------------------------------------------------------------
    // test_loosening_floor_reached pure helper
    // -----------------------------------------------------------------------
    #[test]
    fn test_loosening_floor_reached_empty_schedule() {
        // No schedule → floor never reached.
        assert!(!loosening_floor_reached(&[], 0));
        assert!(!loosening_floor_reached(&[], 100));
    }

    #[test]
    fn test_loosening_floor_reached_2_step_schedule() {
        let thresholds = vec![0.05, 0.02];
        // Not yet exhausted: 0, 1, 2 steps applied.
        assert!(!loosening_floor_reached(&thresholds, 0));
        assert!(!loosening_floor_reached(&thresholds, 1));
        assert!(!loosening_floor_reached(&thresholds, 2));
        // Exhausted at 3 (> 2).
        assert!(loosening_floor_reached(&thresholds, 3));
        assert!(loosening_floor_reached(&thresholds, 10));
    }

    // -----------------------------------------------------------------------
    // test_errored_bucket — Task 3.1 (WU-10)
    // -----------------------------------------------------------------------
    /// `increment_cycle_completed` with outcome="errored" must bump
    /// `errored_count` and leave `dropped_count` untouched.
    #[tokio::test]
    async fn test_errored_bucket() {
        let pool = test_pool().await;
        let sid = "sid-errored";
        create_session_with_id(&pool, sid, "strat-err", "{}", "once", None)
            .await
            .unwrap();

        increment_cycle_completed(&pool, sid, "errored").await.unwrap();

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(sid)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(row.cycles_completed, 1, "cycles_completed must be 1");
        assert_eq!(row.errored_count, 1, "errored_count must be 1");
        assert_eq!(row.dropped_count, 0, "dropped_count must remain 0");
    }

    // -----------------------------------------------------------------------
    // test_errored_count_upgrade — ensure_session_schema upgrades existing DBs
    // -----------------------------------------------------------------------
    /// Simulate a pre-existing DB that lacks `errored_count`. Calling
    /// `ensure_session_schema` must add the column via the guarded ALTER.
    #[tokio::test]
    async fn test_errored_count_upgrade() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create the table from a PRE-change schema (no errored_count).
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
              error             TEXT,
              created_at        TEXT NOT NULL,
              started_at        TEXT,
              finished_at       TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Confirm errored_count is absent before the upgrade.
        let pre_check: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as("PRAGMA table_info(autooptimizer_session_state)")
                .fetch_all(&pool)
                .await
                .unwrap();
        let has_before = pre_check
            .iter()
            .any(|(_, name, _, _, _, _)| name == "errored_count");
        assert!(!has_before, "errored_count must not exist before upgrade");

        // Run the schema function — it should add the column via the guarded ALTER.
        ensure_session_schema(&pool).await.unwrap();

        // Confirm the column now exists.
        let post_check: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as("PRAGMA table_info(autooptimizer_session_state)")
                .fetch_all(&pool)
                .await
                .unwrap();
        let has_after = post_check
            .iter()
            .any(|(_, name, _, _, _, _)| name == "errored_count");
        assert!(
            has_after,
            "errored_count must exist after ensure_session_schema upgrade"
        );

        // Confirm the column is queryable and defaults to 0.
        sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, created_at) \
             VALUES ('sid-up','strat-up','{}','running','once','2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let errored_count: i64 = sqlx::query_scalar(
            "SELECT errored_count FROM autooptimizer_session_state WHERE session_id = 'sid-up'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(errored_count, 0, "errored_count default must be 0");
    }

    // -----------------------------------------------------------------------
    // test_unlimited_runs_until_cancel (GH #965)
    // -----------------------------------------------------------------------
    /// Fire-and-forget mode is `n_experiments` with `cycles_planned = None`:
    /// the count test (`cycles_completed >= i64::MAX`) is never true, so the
    /// loop runs until cancel. No new stored `mode` value or migration is
    /// needed. The injected cycle fn flips cancel after 3 cycles; the loop must
    /// run exactly 3 and end "cancelled".
    #[tokio::test]
    async fn test_unlimited_runs_until_cancel() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-cont", "{}", "n_experiments", None)
            .await
            .unwrap();

        let call_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&call_count);

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let cancel_in_fn = Arc::clone(&cancel);

        run_session(
            &pool,
            &sid,
            "n_experiments",
            None, // unlimited
            None,
            vec![],
            0,                // max_consecutive_errors: session breaker disabled in this test
            Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
            Arc::clone(&cancel),
            Arc::clone(&pause),
            move || {
                let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
                if n >= 3 {
                    // Request shutdown; the loop checks cancel at the top of
                    // the next iteration and seals cleanly.
                    cancel_in_fn.store(true, Ordering::Relaxed);
                }
                let outcome = make_outcome("kept");
                async move { Ok(outcome) }
            },
        )
        .await
        .unwrap();

        assert_eq!(
            call_count.load(Ordering::Relaxed),
            3,
            "unlimited mode must run cycles until cancel is observed"
        );

        let row: OptimizerSession =
            sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.state, "cancelled");
    }

    // -----------------------------------------------------------------------
    // test_budget_is_a_universal_ceiling (GH #965)
    // -----------------------------------------------------------------------
    /// A `budget_cap` is a hard ceiling in EVERY mode: with a high
    /// `n_experiments` count AND with unlimited (`cycles_planned = None`), the
    /// loop stops as soon as `cum_cost_usd >= budget`, even though the count
    /// would otherwise keep going.
    #[tokio::test]
    async fn test_budget_is_a_universal_ceiling() {
        for planned in [Some(1000_i64), None] {
            let pool = test_pool().await;
            let sid = create_session(&pool, "strat-cap", "{}", "n_experiments", planned)
                .await
                .unwrap();

            let call_count = Arc::new(AtomicU32::new(0));
            let counter = Arc::clone(&call_count);

            let cancel = Arc::new(AtomicBool::new(false));
            let pause = Arc::new(AtomicBool::new(false));

            // Each cycle adds $0.05; cap is $0.12 → stop after cycle 3 (cum 0.15).
            run_session(
                &pool,
                &sid,
                "n_experiments",
                planned,
                Some(0.12),
                vec![],
                0,                // max_consecutive_errors: session breaker disabled in this test
                Arc::new(|| 0.0), // cost_so_far: no spend tracked in this test
                Arc::clone(&cancel),
                Arc::clone(&pause),
                move || {
                    let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
                    let cum = n as f64 * 0.05;
                    let outcome = make_outcome_with_cost("dropped", cum);
                    async move { Ok(outcome) }
                },
            )
            .await
            .unwrap();

            assert_eq!(
                call_count.load(Ordering::Relaxed),
                3,
                "planned={planned:?}: budget cap must stop the loop at cycle 3 regardless of count"
            );

            let row: OptimizerSession =
                sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
                    .bind(&sid)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                row.state, "finished",
                "planned={planned:?}: budget stop is a normal finish"
            );
        }
    }
}

//! OptimizerSession entity and state-machine helpers.
//!
//! State transitions:
//! ```
//! queued -> running
//! running -> paused, cancelling, finished, failed
//! paused  -> running, cancelling, failed
//! cancelling -> cancelled, failed
//! (terminal: cancelled, finished, failed)
//! any -> failed  (crash recovery)
//! ```

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
            | (_, "failed") // crash recovery: any -> failed
    )
}

fn is_terminal(state: &str) -> bool {
    matches!(state, "finished" | "cancelled" | "failed")
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Create a new session row in state `running`.
pub async fn create_session(
    pool: &SqlitePool,
    strategy_id: &str,
    config_json: &str,
    mode: &str,
    cycles_planned: Option<i64>,
) -> Result<String> {
    let session_id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO autooptimizer_session_state \
         (session_id, strategy_id, config_json, state, mode, cycles_planned, created_at) \
         VALUES (?,?,?,?,?,?,?)",
    )
    .bind(&session_id)
    .bind(strategy_id)
    .bind(config_json)
    .bind("running")
    .bind(mode)
    .bind(cycles_planned)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(session_id)
}

/// Attempt a state transition. Returns `Err` on illegal transitions.
pub async fn transition_state(
    pool: &SqlitePool,
    session_id: &str,
    new_state: &str,
    error: Option<&str>,
) -> Result<()> {
    let current: String = sqlx::query_scalar(
        "SELECT state FROM autooptimizer_session_state WHERE session_id = ?",
    )
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
pub async fn increment_cycle_completed(
    pool: &SqlitePool,
    session_id: &str,
    outcome: &str,
) -> Result<()> {
    let col = match outcome {
        "kept" => "kept_count",
        "suspect" => "suspect_count",
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a fresh in-memory SQLite pool with migration 057 applied.
    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        // Run the migration SQL directly (same content as 057_autooptimizer_sessions.sql).
        let sql = include_str!("../../migrations/057_autooptimizer_sessions.sql");
        for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(stmt).execute(&pool).await.unwrap();
        }
        pool
    }

    // -----------------------------------------------------------------------
    // test_legal_transitions
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_legal_transitions() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-1", "{}", "once", Some(5)).await.unwrap();

        // running -> paused
        transition_state(&pool, &sid, "paused", None).await.unwrap();

        // paused -> running  (resume)
        transition_state(&pool, &sid, "running", None).await.unwrap();

        // running -> finished
        transition_state(&pool, &sid, "finished", None).await.unwrap();

        // finished_at should now be set
        let row: OptimizerSession = sqlx::query_as(
            "SELECT * FROM autooptimizer_session_state WHERE session_id = ?",
        )
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
        let sid = create_session(&pool, "strat-1", "{}", "once", None).await.unwrap();

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
        for id in ["01SESS_RUNNING", "01SESS_PAUSED", "01SESS_CANCEL", "01SESS_QUEUED"] {
            let row: OptimizerSession = sqlx::query_as(
                "SELECT * FROM autooptimizer_session_state WHERE session_id = ?",
            )
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
            let row: OptimizerSession = sqlx::query_as(
                "SELECT * FROM autooptimizer_session_state WHERE session_id = ?",
            )
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
        let sid = create_session(&pool, "strat-2", "{}", "once", None).await.unwrap();

        let active = get_active_session(&pool).await.unwrap();
        assert!(active.is_some(), "should return Some for running session");
        assert_eq!(active.unwrap().session_id, sid);
    }

    #[tokio::test]
    async fn test_get_active_session_returns_paused() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-3", "{}", "once", None).await.unwrap();

        // running -> paused
        transition_state(&pool, &sid, "paused", None).await.unwrap();

        let active = get_active_session(&pool).await.unwrap();
        assert!(active.is_some(), "should return Some for paused session");
        assert_eq!(active.unwrap().session_id, sid);
    }

    #[tokio::test]
    async fn test_get_active_session_none_after_finished() {
        let pool = test_pool().await;
        let sid = create_session(&pool, "strat-4", "{}", "once", None).await.unwrap();
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

        let row: OptimizerSession = sqlx::query_as(
            "SELECT * FROM autooptimizer_session_state WHERE session_id = ?",
        )
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
        let sid = create_session(&pool, "strat-6", "{}", "once", None).await.unwrap();

        // running -> cancelling -> cancelled
        transition_state(&pool, &sid, "cancelling", None).await.unwrap();
        transition_state(&pool, &sid, "cancelled", None).await.unwrap();

        let row: OptimizerSession = sqlx::query_as(
            "SELECT * FROM autooptimizer_session_state WHERE session_id = ?",
        )
        .bind(&sid)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(row.state, "cancelled");
        assert!(row.finished_at.is_some(), "cancelled must have finished_at");
    }
}

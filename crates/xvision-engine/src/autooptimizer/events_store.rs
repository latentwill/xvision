//! Persistence helpers for the `autooptimizer_events` table (migration 057).
//!
//! `append_event` writes a single structured event row under a session_id
//! so run history is queryable after the SSE stream closes.
//!
//! `prune_old_events` removes event rows for sessions beyond the most recent
//! 50, keeping the table bounded.

use sqlx::SqlitePool;

/// Append a structured event to `autooptimizer_events`.
///
/// - `session_id`: the optimizer session this event belongs to.
/// - `cycle_id`: optional cycle within the session.
/// - `kind`: the snake_case wire name (e.g. `"phase_started"`).
/// - `payload_json`: the full serialized event JSON.
pub async fn append_event(
    pool: &SqlitePool,
    session_id: &str,
    cycle_id: Option<&str>,
    kind: &str,
    payload_json: &str,
) -> anyhow::Result<()> {
    let ts = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO autooptimizer_events (session_id, cycle_id, kind, payload_json, ts) VALUES (?,?,?,?,?)",
    )
    .bind(session_id)
    .bind(cycle_id)
    .bind(kind)
    .bind(payload_json)
    .bind(&ts)
    .execute(pool)
    .await?;
    Ok(())
}

/// Prune event rows for sessions beyond the 50 most recently created.
///
/// The 50-session cap keeps the table bounded without losing data for any
/// active or recent session.
pub async fn prune_old_events(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "DELETE FROM autooptimizer_events WHERE session_id NOT IN \
         (SELECT session_id FROM autooptimizer_session_state ORDER BY created_at DESC LIMIT 50)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn open_test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        // Create the tables from migration 057.
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
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn insert_session_sync<'a>(
        pool: &'a SqlitePool,
        session_id: &'a str,
        created_at: &'a str,
    ) -> impl std::future::Future<Output = ()> + 'a {
        async move {
            sqlx::query(
                "INSERT INTO autooptimizer_session_state \
                 (session_id, strategy_id, config_json, state, mode, created_at) \
                 VALUES (?, 's1', '{}', 'finished', 'once', ?)",
            )
            .bind(session_id)
            .bind(created_at)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    /// append_event inserts a row; a second call increments seq.
    #[tokio::test]
    async fn test_append_event() {
        let pool = open_test_pool().await;

        append_event(
            &pool,
            "sess-1",
            Some("cycle-1"),
            "phase_started",
            r#"{"type":"phase_started"}"#,
        )
        .await
        .unwrap();
        append_event(
            &pool,
            "sess-1",
            None,
            "cycle_finished",
            r#"{"type":"cycle_finished"}"#,
        )
        .await
        .unwrap();

        let rows: Vec<(i64, String, Option<String>, String)> =
            sqlx::query_as("SELECT seq, session_id, cycle_id, kind FROM autooptimizer_events ORDER BY seq")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows.len(), 2, "should have 2 rows");
        let (seq1, _, _, kind1) = &rows[0];
        let (seq2, _, _, kind2) = &rows[1];
        assert!(seq2 > seq1, "seq should increment");
        assert_eq!(kind1, "phase_started");
        assert_eq!(kind2, "cycle_finished");
        assert_eq!(rows[0].2.as_deref(), Some("cycle-1"));
        assert_eq!(rows[1].2, None);
    }

    /// prune_old_events removes events for sessions outside the 50-most-recent.
    #[tokio::test]
    async fn test_prune_old_events() {
        let pool = open_test_pool().await;

        // Insert 55 sessions with distinct created_at timestamps. Sessions are
        // ordered newest-first; we'll keep the 50 most recent (51..55 +
        // chronological ordering).
        for i in 0..55usize {
            let sid = format!("session-{i:03}");
            // created_at: "2026-01-01T00:00:00Z" for session 0 (oldest),
            // "2026-01-01T00:00:54Z" for session 54 (newest).
            let ts = format!("2026-01-01T00:00:{i:02}Z");
            insert_session_sync(&pool, &sid, &ts).await;
            // Insert one event per session.
            append_event(&pool, &sid, None, "test_event", r#"{}"#)
                .await
                .unwrap();
        }

        // Verify we start with 55 events.
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, 55);

        prune_old_events(&pool).await.unwrap();

        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        // 5 oldest sessions should have their events pruned; 50 remain.
        assert_eq!(
            after, 50,
            "prune should retain events for only 50 most-recent sessions"
        );

        // Verify the oldest session (session-000) was pruned.
        let old_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_events WHERE session_id = 'session-000'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(old_count, 0, "events for oldest session should be pruned");

        // Verify the newest session (session-054) is retained.
        let new_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM autooptimizer_events WHERE session_id = 'session-054'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(new_count, 1, "events for newest session should be retained");
    }
}

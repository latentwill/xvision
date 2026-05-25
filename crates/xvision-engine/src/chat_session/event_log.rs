//! `SessionEventLog` — async sqlx wrapper around the `session_events` table
//! (migration 042).
//!
//! Phase 1.2 of the chat-rail / DSPy / strategy-agents wave. This is the
//! persisted unified-event log: every chat-rail row (and, post dual-path
//! migration, every trace-dock row) is a projection of a [`UnifiedEvent`]
//! durably appended here. The legacy `WizardEvent` SSE stream remains a
//! deprecated compatibility shim; the unified session stream replays from this
//! table on reconnect (resume by `session_id` + `after_seq`) and then tails
//! live events from the per-session broadcast bus.
//!
//! Sibling to [`super::store::ChatSessionStore`]. The engine depends on
//! `xvision-observability`, so the canonical `UnifiedEvent` type is reused via
//! the re-export rather than redefined here.

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use xvision_observability::UnifiedEvent;

/// Stateless CRUD over `session_events`. Methods take `&SqlitePool` so the
/// same store is shared across handlers via the dashboard `AppState`.
pub struct SessionEventLog;

impl SessionEventLog {
    /// The next `seq` to assign for `session_id` — `COALESCE(MAX(seq), -1) + 1`.
    /// A fresh session returns `0`. Callers seed a projector with this so the
    /// per-session sequence continues monotonically across turns.
    pub async fn next_seq(pool: &SqlitePool, session_id: &str) -> Result<i64> {
        let next: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(seq), -1) + 1 FROM session_events WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_one(pool)
        .await
        .context("compute next session_events seq")?;
        Ok(next)
    }

    /// Append one already-sequenced [`UnifiedEvent`] to the log. The event's
    /// `seq` is taken as authoritative (the projector assigned it from a
    /// `next_seq`-seeded cursor); `source` and `kind` are denormalized out of
    /// the envelope for cheap filtering, and the full event is stored as JSON
    /// for verbatim SSE replay.
    ///
    /// The owning `session_id` is read off the envelope; an event with no
    /// `session_id` cannot be logged to a session and is rejected loudly
    /// rather than silently dropped (typed-error / never-silent discipline).
    pub async fn append(pool: &SqlitePool, event: &UnifiedEvent) -> Result<()> {
        let session_id = event
            .session_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("UnifiedEvent has no session_id; cannot append to session_events"))?;

        // EventSource serializes (serde rename_all = "snake_case") to a bare
        // JSON string; strip the surrounding quotes to store the snake_case
        // discriminant the migration documents.
        let source = serde_json::to_value(event.source)
            .context("serialize EventSource")?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow::anyhow!("EventSource did not serialize to a string"))?;
        let kind = event.event_name().to_string();
        let payload_json = serde_json::to_string(event).context("serialize UnifiedEvent")?;
        let ts = event.ts.to_rfc3339();
        let seq = i64::try_from(event.seq).context("UnifiedEvent.seq exceeds i64")?;

        sqlx::query(
            "INSERT INTO session_events \
             (event_id, session_id, seq, ts, source, kind, payload_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&event.event_id)
        .bind(session_id)
        .bind(seq)
        .bind(&ts)
        .bind(&source)
        .bind(&kind)
        .bind(&payload_json)
        .execute(pool)
        .await
        .with_context(|| {
            format!("insert session_events row (session={session_id}, seq={seq}, kind={kind})")
        })?;

        Ok(())
    }

    /// Load every persisted [`UnifiedEvent`] for `session_id` with
    /// `seq > after_seq`, ascending. This is the resume primitive: the client
    /// reconnects with its last-rendered cursor and the stream replays only
    /// the events it has not yet seen. Pass `-1` to replay the whole session.
    pub async fn load_after(
        pool: &SqlitePool,
        session_id: &str,
        after_seq: i64,
    ) -> Result<Vec<UnifiedEvent>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT payload_json FROM session_events \
             WHERE session_id = ?1 AND seq > ?2 ORDER BY seq ASC",
        )
        .bind(session_id)
        .bind(after_seq)
        .fetch_all(pool)
        .await
        .context("load session_events after cursor")?;

        rows.into_iter()
            .map(|(json,)| serde_json::from_str(&json).context("parse session_events payload_json"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_session::{ChatSessionStore, ContextScope};
    use chrono::{DateTime, Utc};
    use xvision_observability::{Actor, EventScope, EventSource, UnifiedPayload};

    fn ts() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-24T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    /// Build a session-scoped UnifiedEvent at a given seq with a token-delta
    /// payload carrying `text` (so round-trips are distinguishable).
    fn token_event(session_id: &str, event_id: &str, seq: u64, text: &str) -> UnifiedEvent {
        UnifiedEvent {
            event_id: event_id.into(),
            session_id: Some(session_id.into()),
            run_id: None,
            span_id: None,
            parent_event_id: None,
            seq,
            ts: ts(),
            scope: EventScope::workspace(),
            actor: Actor::Agent,
            source: EventSource::ChatRail,
            blob_hash: None,
            payload: UnifiedPayload::AssistantTokenDelta { text: text.into() },
        }
    }

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        // chat_sessions (FK parent) + session_events (this table).
        sqlx::query(include_str!("../../migrations/003_chat_sessions.sql"))
            .execute(&pool)
            .await
            .unwrap();
        // Apply the 042 schema. The migration file is the production source
        // (run via `sqlx::migrate!`); the DDL is inlined here so the unit test
        // does not have to parse the file's leading comment block + inline
        // column comments through sqlx's single-statement `query`. The schema
        // below is kept byte-for-byte equal to the DDL in
        // `migrations/042_session_events.sql`.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS session_events (\
                 event_id    TEXT PRIMARY KEY, \
                 session_id  TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE, \
                 seq         INTEGER NOT NULL, \
                 ts          TEXT NOT NULL, \
                 source      TEXT NOT NULL, \
                 kind        TEXT NOT NULL, \
                 payload_json TEXT NOT NULL\
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_session_events_seq ON session_events(session_id, seq)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn append_and_load_round_trip() {
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();

        let ev = token_event(&sid, "ev0", 0, "hello");
        SessionEventLog::append(&pool, &ev).await.unwrap();

        let loaded = SessionEventLog::load_after(&pool, &sid, -1).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].event_id, "ev0");
        assert_eq!(loaded[0].seq, 0);
        match &loaded[0].payload {
            UnifiedPayload::AssistantTokenDelta { text } => assert_eq!(text, "hello"),
            other => panic!("wrong payload after round-trip: {other:?}"),
        }
    }

    #[tokio::test]
    async fn next_seq_is_monotonic() {
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();

        // Fresh session starts at 0.
        assert_eq!(SessionEventLog::next_seq(&pool, &sid).await.unwrap(), 0);

        SessionEventLog::append(&pool, &token_event(&sid, "e0", 0, "a"))
            .await
            .unwrap();
        assert_eq!(SessionEventLog::next_seq(&pool, &sid).await.unwrap(), 1);

        SessionEventLog::append(&pool, &token_event(&sid, "e1", 1, "b"))
            .await
            .unwrap();
        assert_eq!(SessionEventLog::next_seq(&pool, &sid).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn load_after_filters_by_cursor() {
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();

        for (i, txt) in ["a", "b", "c", "d", "e"].iter().enumerate() {
            let seq = i as u64;
            SessionEventLog::append(&pool, &token_event(&sid, &format!("e{seq}"), seq, txt))
                .await
                .unwrap();
        }

        // Resume from cursor 2 → only seq 3 and 4 (strictly greater).
        let after2 = SessionEventLog::load_after(&pool, &sid, 2).await.unwrap();
        assert_eq!(after2.len(), 2);
        assert_eq!(after2[0].seq, 3);
        assert_eq!(after2[1].seq, 4);

        // -1 replays everything, ascending.
        let all = SessionEventLog::load_after(&pool, &sid, -1).await.unwrap();
        let seqs: Vec<u64> = all.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2, 3, 4]);

        // Cursor past the end → empty.
        let after_end = SessionEventLog::load_after(&pool, &sid, 99).await.unwrap();
        assert!(after_end.is_empty());
    }

    #[tokio::test]
    async fn load_after_isolates_by_session() {
        let pool = fresh_pool().await;
        let a = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();
        let b = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();

        SessionEventLog::append(&pool, &token_event(&a, "a0", 0, "from-a"))
            .await
            .unwrap();
        SessionEventLog::append(&pool, &token_event(&b, "b0", 0, "from-b"))
            .await
            .unwrap();

        let a_events = SessionEventLog::load_after(&pool, &a, -1).await.unwrap();
        assert_eq!(a_events.len(), 1);
        assert_eq!(a_events[0].event_id, "a0");
    }

    #[tokio::test]
    async fn append_without_session_id_errors() {
        let pool = fresh_pool().await;
        let mut ev = token_event("ignored", "e0", 0, "x");
        ev.session_id = None;
        let err = SessionEventLog::append(&pool, &ev).await.unwrap_err();
        assert!(err.to_string().contains("no session_id"), "got: {err}");
    }

    #[tokio::test]
    async fn delete_session_cascades_events() {
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();
        SessionEventLog::append(&pool, &token_event(&sid, "e0", 0, "x"))
            .await
            .unwrap();

        ChatSessionStore::delete_session(&pool, &sid).await.unwrap();

        let leftover: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE session_id = ?1")
                .bind(&sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(leftover, 0, "ON DELETE CASCADE should clear session_events");
    }
}

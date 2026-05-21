//! `ChatSessionStore` — async sqlx wrapper around `chat_sessions` and
//! `chat_messages`. The Wizard rail's session state lives here; the rail
//! itself (Phase D) and the WizardLoop refactor that drives it (Phase B)
//! call into this store.
//!
//! `seq` is computed atomically per session inside `append` so concurrent
//! writers can't observe a gap or duplicate. The whole append is wrapped in
//! a transaction; on commit the row is durably persisted with a monotonic
//! sequence relative to existing rows for the session.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ulid::Ulid;

use super::context::ContextScope;

/// Classify a SQLx error into a short, operator-readable label naming the
/// SQLite error class. This makes the `append` error visible to operators
/// instead of the swallowed "insert chat_messages row" wrapper that was the
/// original bug (2026-05-21 session: three sequential stream errors with no
/// indication of the underlying cause).
///
/// Known classes that can appear here:
/// - `UNIQUE constraint failed` — (session_id, seq) collision; impossible in
///   normal serial execution but possible if the pool returns a stale
///   connection whose in-flight transaction was already rolled back.
/// - `FOREIGN KEY constraint failed` — session_id references a chat_sessions
///   row that does not exist (e.g. session was deleted between resolve() and
///   append(); sqlx 0.8 enables FK enforcement by default).
/// - `database is locked` — SQLITE_BUSY: another connection (e.g. a
///   concurrent strategy-write transaction) holds the write lock. Root cause
///   of the 2026-05-21 cluster: the default SQLite pool has up to 10
///   connections and no WAL mode; a failed strategy-write that held a
///   connection mid-transaction blocked all subsequent chat-message writes.
fn sqlite_error_label(e: &sqlx::Error) -> &'static str {
    let msg = e.to_string();
    if msg.contains("UNIQUE constraint failed") {
        "UNIQUE constraint failed"
    } else if msg.contains("FOREIGN KEY constraint failed") {
        "FOREIGN KEY constraint failed (session_id → chat_sessions)"
    } else if msg.contains("database is locked") || msg.contains("SQLITE_BUSY") {
        "database is locked (SQLITE_BUSY)"
    } else if msg.contains("no such table") {
        "no such table (schema not migrated)"
    } else {
        "SQLite error (see tracing log for details)"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub role: String,
    pub content_blocks: Vec<serde_json::Value>,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatSessionSummary {
    pub id: String,
    pub scope: ContextScope,
    pub started_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

/// Stateless CRUD over `chat_sessions` + `chat_messages`. Holds nothing —
/// methods take `&SqlitePool` so the same store can be shared across
/// handlers via the AppState.
pub struct ChatSessionStore;

impl ChatSessionStore {
    /// Create a new session row. Returns the generated ULID. The session's
    /// `started_at` and `last_activity_at` are both set to now; the scope
    /// is serialized to JSON.
    pub async fn create_session(pool: &SqlitePool, scope: &ContextScope) -> Result<String> {
        let id = Ulid::new().to_string();
        let now = Utc::now().to_rfc3339();
        let scope_json = serde_json::to_string(scope).context("serialize ContextScope")?;
        sqlx::query(
            "INSERT INTO chat_sessions (id, started_at, last_activity_at, context_scope_json) \
             VALUES (?1, ?2, ?2, ?3)",
        )
        .bind(&id)
        .bind(&now)
        .bind(&scope_json)
        .execute(pool)
        .await
        .context("insert chat_sessions row")?;
        Ok(id)
    }

    /// Append a message to a session. Computes the next `seq` atomically
    /// (single transaction: SELECT MAX → INSERT → UPDATE last_activity).
    /// Returns the inserted `ChatMessage` with its assigned id + seq.
    pub async fn append(
        pool: &SqlitePool,
        session_id: &str,
        role: &str,
        blocks: &[serde_json::Value],
    ) -> Result<ChatMessage> {
        let id = Ulid::new().to_string();
        let now = Utc::now();
        let now_rfc = now.to_rfc3339();
        let blocks_json = serde_json::to_string(blocks).context("serialize content_blocks array")?;

        let mut tx = pool.begin().await.context("begin tx for append")?;

        let next_seq: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(seq), -1) + 1 FROM chat_messages WHERE session_id = ?1")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await
                .context("compute next seq")?;

        sqlx::query(
            "INSERT INTO chat_messages (id, session_id, seq, role, content_blocks_json, ts) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(next_seq)
        .bind(role)
        .bind(&blocks_json)
        .bind(&now_rfc)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            let label = sqlite_error_label(&e);
            tracing::error!(
                session_id = session_id,
                seq = next_seq,
                db_error = %e,
                "insert chat_messages row failed: {label}",
            );
            anyhow::anyhow!("insert chat_messages row: {label} — {e}")
        })?;

        sqlx::query("UPDATE chat_sessions SET last_activity_at = ?2 WHERE id = ?1")
            .bind(session_id)
            .bind(&now_rfc)
            .execute(&mut *tx)
            .await
            .context("touch session last_activity_at")?;

        tx.commit().await.context("commit append tx")?;

        Ok(ChatMessage {
            id,
            session_id: session_id.into(),
            seq: next_seq,
            role: role.into(),
            content_blocks: blocks.to_vec(),
            ts: now,
        })
    }

    /// Load all messages for a session in chronological (`seq` ASC) order.
    pub async fn load_history(pool: &SqlitePool, session_id: &str) -> Result<Vec<ChatMessage>> {
        let rows: Vec<(String, String, i64, String, String, String)> = sqlx::query_as(
            "SELECT id, session_id, seq, role, content_blocks_json, ts \
             FROM chat_messages WHERE session_id = ?1 ORDER BY seq ASC",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
        .context("load chat history")?;

        rows.into_iter()
            .map(|(id, sid, seq, role, blocks_json, ts)| {
                Ok(ChatMessage {
                    id,
                    session_id: sid,
                    seq,
                    role,
                    content_blocks: serde_json::from_str(&blocks_json)
                        .context("parse content_blocks_json")?,
                    ts: DateTime::parse_from_rfc3339(&ts)
                        .context("parse ts")?
                        .with_timezone(&Utc),
                })
            })
            .collect()
    }

    /// Update only `last_activity_at` without writing a message — useful
    /// for keep-alive heartbeats from the rail's WebSocket / SSE handler.
    pub async fn touch(pool: &SqlitePool, session_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE chat_sessions SET last_activity_at = ?2 WHERE id = ?1")
            .bind(session_id)
            .bind(&now)
            .execute(pool)
            .await
            .context("touch session")?;
        Ok(())
    }

    /// Resolve the chat-rail session for a given scope: return the
    /// most-recently active session for that scope along with its
    /// history, creating a fresh empty session if no match exists.
    ///
    /// The lookup compares the persisted `context_scope_json` against a
    /// fresh serialization of `scope`. Serde is deterministic for the
    /// same value so writer and reader hash to the same string.
    ///
    /// Replaces the previous "frontend caches a session id and validates
    /// via update_scope" flow — sessions are now owned server-side so
    /// the rail can't hold a stale id across DB resets or fresh deploys.
    pub async fn resolve(pool: &SqlitePool, scope: &ContextScope) -> Result<(String, Vec<ChatMessage>)> {
        let scope_json = serde_json::to_string(scope).context("serialize ContextScope")?;
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM chat_sessions \
             WHERE context_scope_json = ?1 \
             ORDER BY last_activity_at DESC LIMIT 1",
        )
        .bind(&scope_json)
        .fetch_optional(pool)
        .await
        .context("query chat_sessions by scope")?;

        match existing {
            Some(id) => {
                let history = Self::load_history(pool, &id).await?;
                Ok((id, history))
            }
            None => {
                let id = Self::create_session(pool, scope).await?;
                Ok((id, Vec::new()))
            }
        }
    }

    /// Read the session's current `ContextScope`. Errors if the session
    /// doesn't exist; falls back to `Workspace` if the JSON blob can't be
    /// parsed (forward-compat: a future variant the local binary doesn't
    /// know yet shouldn't break the read path).
    pub async fn load_scope(pool: &SqlitePool, session_id: &str) -> Result<ContextScope> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT context_scope_json FROM chat_sessions WHERE id = ?1")
                .bind(session_id)
                .fetch_optional(pool)
                .await
                .context("read context_scope_json")?;
        let json = json.ok_or_else(|| anyhow::anyhow!("session {session_id} not found"))?;
        Ok(serde_json::from_str(&json).unwrap_or_default())
    }

    /// Delete a session. Cascades to its messages via the FK.
    pub async fn delete_session(pool: &SqlitePool, session_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM chat_sessions WHERE id = ?1")
            .bind(session_id)
            .execute(pool)
            .await
            .context("delete session")?;
        Ok(())
    }

    /// List sessions newest-first. Used by the chat rail's history pane.
    pub async fn list_sessions(pool: &SqlitePool) -> Result<Vec<ChatSessionSummary>> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT id, context_scope_json, started_at, last_activity_at \
             FROM chat_sessions ORDER BY last_activity_at DESC",
        )
        .fetch_all(pool)
        .await
        .context("list sessions")?;

        rows.into_iter()
            .map(|(id, scope_json, started_at, last_activity_at)| {
                let scope = serde_json::from_str(&scope_json).unwrap_or_default();
                Ok(ChatSessionSummary {
                    id,
                    scope,
                    started_at: DateTime::parse_from_rfc3339(&started_at)
                        .context("parse started_at")?
                        .with_timezone(&Utc),
                    last_activity_at: DateTime::parse_from_rfc3339(&last_activity_at)
                        .context("parse last_activity_at")?
                        .with_timezone(&Utc),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        // Foreign keys are off by default in SQLite; turn them on so the
        // ON DELETE CASCADE on chat_messages.session_id actually fires.
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(include_str!("../../migrations/003_chat_sessions.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn block(text: &str) -> serde_json::Value {
        serde_json::json!({ "type": "text", "text": text })
    }

    #[tokio::test]
    async fn append_assigns_monotonic_seq() {
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();
        let m1 = ChatSessionStore::append(&pool, &sid, "user", &[block("hi")])
            .await
            .unwrap();
        let m2 = ChatSessionStore::append(&pool, &sid, "assistant", &[block("hello")])
            .await
            .unwrap();
        let m3 = ChatSessionStore::append(&pool, &sid, "user", &[block("again")])
            .await
            .unwrap();
        assert_eq!(m1.seq, 0);
        assert_eq!(m2.seq, 1);
        assert_eq!(m3.seq, 2);
    }

    #[tokio::test]
    async fn load_history_returns_seq_order() {
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();
        ChatSessionStore::append(&pool, &sid, "user", &[block("first")])
            .await
            .unwrap();
        ChatSessionStore::append(&pool, &sid, "assistant", &[block("second")])
            .await
            .unwrap();
        let history = ChatSessionStore::load_history(&pool, &sid).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].seq, 0);
        assert_eq!(history[1].seq, 1);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[1].role, "assistant");
    }

    #[tokio::test]
    async fn delete_session_cascades_messages() {
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();
        ChatSessionStore::append(&pool, &sid, "user", &[block("hi")])
            .await
            .unwrap();

        ChatSessionStore::delete_session(&pool, &sid).await.unwrap();

        let leftover: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE session_id = ?1")
            .bind(&sid)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(leftover, 0, "ON DELETE CASCADE should clear messages");
    }

    #[tokio::test]
    async fn create_persists_scope() {
        let pool = fresh_pool().await;
        let scope = ContextScope::Run {
            run_id: "01HRUN".into(),
        };
        let sid = ChatSessionStore::create_session(&pool, &scope).await.unwrap();
        let back = ChatSessionStore::load_scope(&pool, &sid).await.unwrap();
        assert_eq!(back, scope);
    }

    #[tokio::test]
    async fn resolve_creates_then_returns_existing_session() {
        let pool = fresh_pool().await;
        let scope = ContextScope::Strategy {
            draft_id: "btc-momentum".into(),
        };

        // First call has no match → creates a fresh session, empty history.
        let (id1, history1) = ChatSessionStore::resolve(&pool, &scope).await.unwrap();
        assert!(history1.is_empty());

        // Append a message; second resolve must return the same id and
        // include that message.
        ChatSessionStore::append(&pool, &id1, "user", &[block("hi")])
            .await
            .unwrap();
        let (id2, history2) = ChatSessionStore::resolve(&pool, &scope).await.unwrap();
        assert_eq!(id2, id1);
        assert_eq!(history2.len(), 1);
        assert_eq!(history2[0].role, "user");
    }

    #[tokio::test]
    async fn resolve_returns_most_recent_for_scope() {
        let pool = fresh_pool().await;
        let scope = ContextScope::Workspace;

        let id_old = ChatSessionStore::create_session(&pool, &scope).await.unwrap();
        // Sleep enough that last_activity_at differs at RFC3339 resolution.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let id_new = ChatSessionStore::create_session(&pool, &scope).await.unwrap();

        let (resolved, _) = ChatSessionStore::resolve(&pool, &scope).await.unwrap();
        assert_eq!(resolved, id_new, "most recent wins");
        assert_ne!(resolved, id_old);
    }

    #[tokio::test]
    async fn resolve_differentiates_by_scope() {
        let pool = fresh_pool().await;
        let workspace = ContextScope::Workspace;
        let strategy = ContextScope::Strategy { draft_id: "x".into() };

        let (workspace_id, _) = ChatSessionStore::resolve(&pool, &workspace).await.unwrap();
        let (strategy_id, _) = ChatSessionStore::resolve(&pool, &strategy).await.unwrap();
        assert_ne!(workspace_id, strategy_id, "scopes get distinct sessions");

        // Re-resolving each scope returns its own existing id, not a new one.
        let (workspace_again, _) = ChatSessionStore::resolve(&pool, &workspace).await.unwrap();
        assert_eq!(workspace_again, workspace_id);
    }

    #[tokio::test]
    async fn append_updates_last_activity() {
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();
        let initial: String = sqlx::query_scalar("SELECT last_activity_at FROM chat_sessions WHERE id = ?1")
            .bind(&sid)
            .fetch_one(&pool)
            .await
            .unwrap();
        // Sleep just enough that the RFC3339 timestamp must differ.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        ChatSessionStore::append(&pool, &sid, "user", &[block("x")])
            .await
            .unwrap();
        let after: String = sqlx::query_scalar("SELECT last_activity_at FROM chat_sessions WHERE id = ?1")
            .bind(&sid)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_ne!(initial, after);
    }

    #[tokio::test]
    async fn load_scope_falls_back_to_workspace_on_unknown_variant() {
        // Forward-compat: a future variant the local binary doesn't know
        // shouldn't break load_scope.
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();
        sqlx::query("UPDATE chat_sessions SET context_scope_json = ?2 WHERE id = ?1")
            .bind(&sid)
            .bind(r#"{"scope":"future_thing","payload":42}"#)
            .execute(&pool)
            .await
            .unwrap();
        let scope = ChatSessionStore::load_scope(&pool, &sid).await.unwrap();
        assert_eq!(scope, ContextScope::Workspace);
    }
}

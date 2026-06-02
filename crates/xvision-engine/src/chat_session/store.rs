//! `ChatSessionStore` — async sqlx wrapper around `chat_sessions` and
//! `chat_messages`. The Wizard rail's session state lives here; the rail
//! itself (Phase D) and the WizardLoop refactor that drives it (Phase B)
//! call into this store.
//!
//! `seq` is computed atomically per session inside `append` so concurrent
//! writers can't observe a gap or duplicate. The whole append is wrapped in
//! a transaction; on commit the row is durably persisted with a monotonic
//! sequence relative to existing rows for the session.

use std::time::Instant;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ulid::Ulid;

use super::context::ContextScope;

type RailStateRow = Option<(i64, String, Option<String>, Option<String>, Option<String>, Option<String>)>;

#[derive(Debug, Clone, Copy)]
struct PoolSnapshot {
    size: u32,
    idle: usize,
}

impl PoolSnapshot {
    fn in_use(self) -> u32 {
        self.size.saturating_sub(self.idle as u32)
    }
}

fn pool_snapshot(pool: &SqlitePool) -> PoolSnapshot {
    PoolSnapshot {
        size: pool.size(),
        idle: pool.num_idle(),
    }
}

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

/// Turn a zero-rows-affected UPDATE into a "session not found" error so
/// callers never silently no-op against a missing session id.
fn ensure_session_existed(rows_affected: u64, session_id: &str) -> Result<()> {
    if rows_affected == 0 {
        anyhow::bail!("session {session_id} not found");
    }
    Ok(())
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
    pub mode: String,
    pub started_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

/// Durable per-session rail state (migration 041). Drives unified-stream
/// resume and the Phase 2 safety surfaces. All fields have defaults so a
/// freshly-created session is immediately usable in read-only `research`
/// mode with an empty cursor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatSessionRailState {
    /// Last unified-event `seq` the client acknowledged consuming.
    pub event_cursor: i64,
    /// `research` (read-only, default) | `act` (write tools available).
    pub mode: String,
    /// Pinned focus-chain file path for the session's scope, if attached.
    pub focus_path: Option<String>,
    /// Snapshot of the three-state tool policy in force, serialized JSON.
    /// `None` = inherit the user/global default.
    pub tool_policy_json: Option<String>,
    /// Id of the most recent checkpoint written for this session.
    pub checkpoint_head: Option<String>,
    /// Optional participant list, serialized JSON.
    pub participants_json: Option<String>,
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

        // Acquire a dedicated connection so the write transaction can be opened
        // with `BEGIN IMMEDIATE`. sqlx's `pool.begin()` issues a *deferred*
        // `BEGIN`; this method is a read-modify-write (SELECT MAX(seq) → INSERT
        // → UPDATE), and under a deferred transaction the SELECT takes a read
        // snapshot and the INSERT must then upgrade to a writer. Under WAL a
        // concurrent writer (the FinalizeWriter actor, the observability
        // event-bus writer, api_audit / search-index upserts) can commit in
        // that window, so the upgrade fails with SQLITE_BUSY_SNAPSHOT —
        // returned *immediately* (`wait_ms=0`), which `busy_timeout` cannot
        // rescue because waiting can never resolve a stale snapshot. That was
        // the root cause of the `wait_ms=0` "database is locked" cluster.
        // `BEGIN IMMEDIATE` takes the write lock up front: `busy_timeout` then
        // governs the wait, no stale snapshot can form, and same-session
        // appends serialize cleanly. Mirrors `SearchIndex::upsert_once`, which
        // fixed the identical deferred→immediate upgrade race (intake #344).
        let begin_pool = pool_snapshot(pool);
        let begin_started = Instant::now();
        let mut conn = pool
            .acquire()
            .await
            .context("acquire connection for chat append")?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| {
                let wait_ms = begin_started.elapsed().as_millis() as u64;
                let label = sqlite_error_label(&e);
                tracing::error!(
                    session_id = session_id,
                    wait_ms = wait_ms,
                    pool_size = begin_pool.size,
                    pool_idle = begin_pool.idle,
                    pool_in_use = begin_pool.in_use(),
                    db_error = %e,
                    "begin chat append tx failed: {label}",
                );
                anyhow::anyhow!(
                    "begin immediate tx for append: {label} (wait_ms={wait_ms}, pool_in_use={}) - {e}",
                    begin_pool.in_use()
                )
            })?;

        let next_seq: i64 = match sqlx::query_scalar(
            "SELECT COALESCE(MAX(seq), -1) + 1 FROM chat_messages WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_one(&mut *conn)
        .await
        {
            Ok(seq) => seq,
            Err(e) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(anyhow::Error::from(e).context("compute next seq"));
            }
        };

        let insert_pool = pool_snapshot(pool);
        let insert_started = Instant::now();
        if let Err(e) = sqlx::query(
            "INSERT INTO chat_messages (id, session_id, seq, role, content_blocks_json, ts) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(next_seq)
        .bind(role)
        .bind(&blocks_json)
        .bind(&now_rfc)
        .execute(&mut *conn)
        .await
        {
            let wait_ms = insert_started.elapsed().as_millis() as u64;
            let label = sqlite_error_label(&e);
            tracing::error!(
                session_id = session_id,
                seq = next_seq,
                wait_ms = wait_ms,
                pool_size = insert_pool.size,
                pool_idle = insert_pool.idle,
                pool_in_use = insert_pool.in_use(),
                db_error = %e,
                "insert chat_messages row failed: {label}",
            );
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(anyhow::anyhow!(
                "insert chat_messages row: {label} (wait_ms={wait_ms}, pool_in_use={}) - {e}",
                insert_pool.in_use()
            ));
        }

        if let Err(e) = sqlx::query("UPDATE chat_sessions SET last_activity_at = ?2 WHERE id = ?1")
            .bind(session_id)
            .bind(&now_rfc)
            .execute(&mut *conn)
            .await
        {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(anyhow::Error::from(e).context("touch session last_activity_at"));
        }

        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .context("commit append tx")?;

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

    /// Load the durable rail state (migration 041). Returns an error if the
    /// session does not exist.
    pub async fn load_rail_state(pool: &SqlitePool, session_id: &str) -> Result<ChatSessionRailState> {
        let row: RailStateRow = sqlx::query_as(
            "SELECT event_cursor, mode, focus_path, tool_policy_json, checkpoint_head, participants_json \
                 FROM chat_sessions WHERE id = ?1",
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .context("read chat_session rail state")?;
        let (event_cursor, mode, focus_path, tool_policy_json, checkpoint_head, participants_json) =
            row.ok_or_else(|| anyhow::anyhow!("session {session_id} not found"))?;
        Ok(ChatSessionRailState {
            event_cursor,
            mode,
            focus_path,
            tool_policy_json,
            checkpoint_head,
            participants_json,
        })
    }

    /// Advance the unified-stream resume cursor. The client acks the last
    /// `seq` it rendered; resume replays from `event_cursor + 1`.
    pub async fn set_event_cursor(pool: &SqlitePool, session_id: &str, cursor: i64) -> Result<()> {
        let affected = sqlx::query("UPDATE chat_sessions SET event_cursor = ?1 WHERE id = ?2")
            .bind(cursor)
            .bind(session_id)
            .execute(pool)
            .await
            .context("update chat_sessions.event_cursor")?
            .rows_affected();
        ensure_session_existed(affected, session_id)
    }

    /// Set the Research / Act mode. Server-side enforcement (Phase 2.2) reads
    /// this; the column is the source of truth, never the client-sent flag.
    pub async fn set_mode(pool: &SqlitePool, session_id: &str, mode: &str) -> Result<()> {
        let affected = sqlx::query("UPDATE chat_sessions SET mode = ?1 WHERE id = ?2")
            .bind(mode)
            .bind(session_id)
            .execute(pool)
            .await
            .context("update chat_sessions.mode")?
            .rows_affected();
        ensure_session_existed(affected, session_id)
    }

    /// Attach / clear the pinned focus-chain file path (Phase 2.4).
    pub async fn set_focus_path(pool: &SqlitePool, session_id: &str, path: Option<&str>) -> Result<()> {
        let affected = sqlx::query("UPDATE chat_sessions SET focus_path = ?1 WHERE id = ?2")
            .bind(path)
            .bind(session_id)
            .execute(pool)
            .await
            .context("update chat_sessions.focus_path")?
            .rows_affected();
        ensure_session_existed(affected, session_id)
    }

    /// Snapshot the three-state tool policy in force (Phase 2.3).
    pub async fn set_tool_policy(pool: &SqlitePool, session_id: &str, json: Option<&str>) -> Result<()> {
        let affected = sqlx::query("UPDATE chat_sessions SET tool_policy_json = ?1 WHERE id = ?2")
            .bind(json)
            .bind(session_id)
            .execute(pool)
            .await
            .context("update chat_sessions.tool_policy_json")?
            .rows_affected();
        ensure_session_existed(affected, session_id)
    }

    /// Record the latest checkpoint id written for this session (Phase 2.5).
    pub async fn set_checkpoint_head(pool: &SqlitePool, session_id: &str, ckpt: Option<&str>) -> Result<()> {
        let affected = sqlx::query("UPDATE chat_sessions SET checkpoint_head = ?1 WHERE id = ?2")
            .bind(ckpt)
            .bind(session_id)
            .execute(pool)
            .await
            .context("update chat_sessions.checkpoint_head")?
            .rows_affected();
        ensure_session_existed(affected, session_id)
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
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, context_scope_json, mode, started_at, last_activity_at \
             FROM chat_sessions ORDER BY last_activity_at DESC",
        )
        .fetch_all(pool)
        .await
        .context("list sessions")?;

        rows.into_iter()
            .map(|(id, scope_json, mode, started_at, last_activity_at)| {
                let scope = serde_json::from_str(&scope_json).unwrap_or_default();
                Ok(ChatSessionSummary {
                    id,
                    scope,
                    mode,
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
        // Phase 1.3 rail-state columns. The migration is several ALTER
        // statements; sqlx's simple `query` runs one statement at a time, so
        // run each `ALTER TABLE …` line on its own. SQLite tolerates the
        // leading `--` comment lines, so we just keep lines that start an
        // ALTER and feed them through.
        let m041 = include_str!("../../migrations/041_chat_session_rail_state.sql");
        for line in m041.lines() {
            let line = line.trim();
            if line.starts_with("ALTER") {
                let stmt = line.trim_end_matches(';');
                sqlx::query(stmt).execute(&pool).await.unwrap();
            }
        }
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
    async fn rail_state_defaults_and_updates() {
        let pool = fresh_pool().await;
        let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
            .await
            .unwrap();

        // Fresh session: read-only research mode, empty cursor, no focus/policy.
        let st = ChatSessionStore::load_rail_state(&pool, &sid).await.unwrap();
        assert_eq!(st.mode, "research");
        assert_eq!(st.event_cursor, 0);
        assert_eq!(st.focus_path, None);
        assert_eq!(st.tool_policy_json, None);
        assert_eq!(st.checkpoint_head, None);

        // Mutate each field, reload, confirm persistence.
        ChatSessionStore::set_event_cursor(&pool, &sid, 42).await.unwrap();
        ChatSessionStore::set_mode(&pool, &sid, "act").await.unwrap();
        ChatSessionStore::set_focus_path(&pool, &sid, Some("scopes/strategy/abc/focus.md"))
            .await
            .unwrap();
        ChatSessionStore::set_tool_policy(&pool, &sid, Some(r#"{"create_strategy":{"enabled":true}}"#))
            .await
            .unwrap();
        ChatSessionStore::set_checkpoint_head(&pool, &sid, Some("ckpt_1"))
            .await
            .unwrap();

        let st = ChatSessionStore::load_rail_state(&pool, &sid).await.unwrap();
        assert_eq!(st.event_cursor, 42);
        assert_eq!(st.mode, "act");
        assert_eq!(st.focus_path.as_deref(), Some("scopes/strategy/abc/focus.md"));
        assert!(st.tool_policy_json.unwrap().contains("create_strategy"));
        assert_eq!(st.checkpoint_head.as_deref(), Some("ckpt_1"));
    }

    #[tokio::test]
    async fn rail_state_update_unknown_session_errors() {
        let pool = fresh_pool().await;
        let err = ChatSessionStore::set_mode(&pool, "nope", "act")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
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

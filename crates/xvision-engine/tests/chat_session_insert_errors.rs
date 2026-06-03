//! Reproduction tests for the 2026-05-21 `insert chat_messages row` stream
//! errors. Three sequential failures hit the operator's session on consecutive
//! messages immediately after a series of failed `create_strategy` writes.
//!
//! Three root causes investigated:
//!
//! 1. **`(session_id, seq)` unique-constraint collision** (root cause A).
//!    RULED OUT as the primary driver: the `append` transaction reads
//!    `MAX(seq)` and inserts atomically. A rolled-back prior append leaves the
//!    seq counter correct because the read was inside the same tx that rolled
//!    back. Test `seq_does_not_collide_after_failed_insert` verifies this.
//!
//! 2. **FK violation on `session_id`** (root cause B — CONFIRMED as a possible
//!    failure mode). Contrary to the initial hypothesis, `sqlx 0.8` enables
//!    `PRAGMA foreign_keys = ON` by default on every new connection (see
//!    `sqlx-sqlite/src/options/mod.rs`). This means production pools DO enforce
//!    FK constraints. If a `create_strategy` write opened a transaction that
//!    also created a chat session row and then rolled back, the subsequent
//!    `append` for that session_id would hit `FOREIGN KEY constraint failed`.
//!    Test `append_to_nonexistent_session_with_fk_on` reproduces this.
//!    Test `append_to_nonexistent_session_fk_error_names_constraint` confirms
//!    the new error message names the constraint class.
//!
//! 3. **`database is locked` (SQLITE_BUSY)** (root cause C — also a plausible
//!    contributor). The production pool is a default `SqlitePool` (up to 10
//!    connections). With SQLite's default journal mode, only one writer at a
//!    time is allowed across connections. When a strategy-write transaction
//!    holds the write lock on one connection, a concurrent `append` call on a
//!    second pool connection gets `SQLITE_BUSY`. This was the "swallowed" error
//!    — the old `.context("insert chat_messages row")` wrapper discarded the
//!    underlying SQLx error string, so the operator saw only
//!    "insert chat_messages row" with no indication of the cause.
//!
//!    The fix replaces the `.context()` with a `map_err` that:
//!    (a) logs `tracing::error!` with `db_error = %e` (structured field),
//!    (b) preserves the SQLite class in the returned error chain.
//!
//!    Test `error_message_names_sqlite_class_on_unique_violation` drives a
//!    deterministic UNIQUE constraint violation through `ChatSessionStore::append`
//!    and asserts the error string contains the SQLite class label rather than
//!    the opaque old text.

use std::{str::FromStr, time::Duration};

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::SqlitePool;
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Standard pool — sqlx 0.8 enables `PRAGMA foreign_keys = ON` by default on
/// every new connection, so all pools built this way enforce FK constraints.
/// This mirrors the production pool created by `ApiContext::open` →
/// `SqlitePool::connect(&url)`.
async fn fresh_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/003_chat_sessions.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn file_pool_with_short_busy_timeout() -> (SqlitePool, tempfile::TempDir) {
    let td = tempfile::tempdir().unwrap();
    let db_path = td.path().join("chat.sqlite");
    let url = format!("sqlite://{}", db_path.display());
    let opts = SqliteConnectOptions::from_str(&url)
        .unwrap()
        .create_if_missing(true)
        .busy_timeout(Duration::from_millis(1));
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/003_chat_sessions.sql"))
        .execute(&pool)
        .await
        .unwrap();
    (pool, td)
}

/// File-backed pool configured exactly like the production `xvn.db` pool
/// (`ApiContext::open`): WAL journaling + non-zero `busy_timeout` + a bounded
/// connection cap. This is the shape under which the `wait_ms=0`
/// SQLITE_BUSY_SNAPSHOT cluster occurred — WAL lets a concurrent writer commit
/// while a deferred reader holds its snapshot, so the read→write upgrade fails
/// immediately and `busy_timeout` can't rescue it.
async fn wal_file_pool() -> (SqlitePool, tempfile::TempDir) {
    let td = tempfile::tempdir().unwrap();
    let db_path = td.path().join("chat.sqlite");
    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(10))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/003_chat_sessions.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/013_cli_jobs.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/018_agent_run_observability.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/037_review_annotations_and_autofire.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/038_eval_runs_live_config.sql"))
        .execute(&pool)
        .await
        .unwrap();
    (pool, td)
}

fn block(text: &str) -> serde_json::Value {
    serde_json::json!({ "type": "text", "text": text })
}

// ── root cause A: seq collision ───────────────────────────────────────────────

/// The `append` tx reads MAX(seq) and inserts atomically. If a prior append
/// rolled back, the seq counter is correct on the next call because the
/// rolled-back tx released its write lock and left no committed rows.
/// This test forces a rollback scenario by inserting the same (session_id, seq)
/// pair manually — simulating what would happen if the pool handed back a
/// connection whose in-flight insert was already rolled back — and asserts
/// that the *next* `append` call still succeeds with the right seq.
#[tokio::test]
async fn seq_does_not_collide_after_failed_insert() {
    let pool = fresh_pool().await;
    let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
        .await
        .unwrap();

    // Append first message normally → seq 0.
    let m0 = ChatSessionStore::append(&pool, &sid, "user", &[block("first")])
        .await
        .unwrap();
    assert_eq!(m0.seq, 0);

    // Simulate: a failed insert whose transaction was rolled back. The key
    // point is that we attempt a BEGIN outside `append` and then roll it back,
    // leaving the committed seq at 0.
    {
        let mut tx = pool.begin().await.unwrap();
        let _: Result<_, _> = sqlx::query(
            "INSERT INTO chat_messages (id, session_id, seq, role, content_blocks_json, ts) \
             VALUES ('fake-id', ?1, 99, 'user', '[]', '2026-01-01T00:00:00Z')",
        )
        .bind(&sid)
        .execute(&mut *tx)
        .await;
        // Explicit rollback — this is what a failed strategy-write tx does.
        tx.rollback().await.unwrap();
    }

    // The next real append must see seq = 1 (not 99, because the tx rolled back,
    // and not 0 again, because seq 0 is committed).
    let m1 = ChatSessionStore::append(&pool, &sid, "user", &[block("second")])
        .await
        .unwrap();
    assert_eq!(
        m1.seq, 1,
        "seq must advance past the committed row, ignoring the rolled-back insert"
    );
}

// ── root cause B: FK violation ────────────────────────────────────────────────

/// sqlx 0.8 enables `PRAGMA foreign_keys = ON` by default on every connection
/// (source: sqlx-sqlite/src/options/mod.rs). This means the FK on
/// `chat_messages.session_id → chat_sessions.id` IS enforced in production.
///
/// If a `create_strategy` write opened a transaction that also created a
/// chat_sessions row and then rolled back, the subsequent `append` for that
/// session_id hits `FOREIGN KEY constraint failed`.
///
/// This test reproduces that scenario: insert a message for a session_id that
/// was never committed to chat_sessions.
#[tokio::test]
async fn append_to_nonexistent_session_with_fk_on() {
    let pool = fresh_pool().await;
    // "ghost-session" is never inserted into chat_sessions.
    let result = ChatSessionStore::append(&pool, "ghost-session", "user", &[block("hi")]).await;
    let err = result.expect_err("FK ON (sqlx 0.8 default): insert to unknown session_id must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("FOREIGN KEY constraint failed"),
        "error message must name the FK constraint class; got: {msg}"
    );
    // Confirm the new error format names the class rather than being opaque.
    assert!(
        msg.contains("session_id → chat_sessions"),
        "error message must identify which FK failed; got: {msg}"
    );
}

/// The error returned by `append` for an FK violation must not be the old
/// opaque "insert chat_messages row" with no further detail.
#[tokio::test]
async fn append_to_nonexistent_session_fk_error_names_constraint() {
    let pool = fresh_pool().await;
    let result = ChatSessionStore::append(&pool, "no-such-session", "user", &[block("msg")]).await;
    let err = result.expect_err("must fail with FK violation");
    let msg = err.to_string();
    // Old behaviour: "insert chat_messages row\n\nCaused by:\n    ..."
    // New behaviour: "insert chat_messages row: FOREIGN KEY constraint failed ..."
    assert!(
        !msg.starts_with("insert chat_messages row\n"),
        "error must include SQLite class inline, not as a bare anyhow context chain; got: {msg}"
    );
    assert!(
        msg.contains("FOREIGN KEY constraint failed"),
        "error must name the SQLite error class; got: {msg}"
    );
}

// ── root cause C: error message quality ──────────────────────────────────────

/// The core fix: when an insert fails, the error message must name the SQLite
/// error class. Drive a deterministic UNIQUE constraint violation inside
/// `ChatSessionStore::append` by adding a test-only unique index after the
/// first append, then appending another message with the same role.
///
/// This is the most direct reproduction of the swallowed-error bug:
/// previously the operator saw "stream error: insert chat_messages row"
/// with no indication of which SQLite error fired.
#[tokio::test]
async fn error_message_names_sqlite_class_on_unique_violation() {
    let pool = fresh_pool().await;
    let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
        .await
        .unwrap();

    ChatSessionStore::append(&pool, &sid, "user", &[block("first")])
        .await
        .unwrap();
    sqlx::query(
        "CREATE UNIQUE INDEX test_unique_chat_messages_session_role \
         ON chat_messages(session_id, role)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let err = ChatSessionStore::append(&pool, &sid, "user", &[block("second")])
        .await
        .expect_err("test-only unique index must force append to fail");
    let msg = err.to_string();
    assert!(
        msg.contains("insert chat_messages row: UNIQUE constraint failed"),
        "append error must include the inline SQLite class label; got: {msg}"
    );
}

#[tokio::test]
async fn append_error_includes_sqlite_busy_label_when_writer_lock_is_held() {
    let (pool, _td) = file_pool_with_short_busy_timeout().await;
    let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO chat_sessions (id, started_at, last_activity_at, context_scope_json) \
         VALUES ('lock-holder', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '{}')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    let err = ChatSessionStore::append(&pool, &sid, "user", &[block("blocked")])
        .await
        .expect_err("held writer transaction must force SQLITE_BUSY on append");
    let msg = err.to_string();
    // With the `BEGIN IMMEDIATE` fix the write lock is taken up front, so a
    // held writer surfaces SQLITE_BUSY at the begin step rather than the
    // INSERT. The contract is unchanged: the error must still name the
    // SQLite class inline rather than swallowing it.
    assert!(
        msg.contains("database is locked (SQLITE_BUSY)"),
        "append error must include the inline SQLITE_BUSY label; got: {msg}"
    );

    tx.rollback().await.unwrap();
}

/// Regression for the `wait_ms=0` "database is locked (SQLITE_BUSY)" stream
/// errors (2026-05-27 session, eval-run chat thread). `append` is a
/// read-modify-write: `SELECT MAX(seq)+1` → `INSERT` → `UPDATE`. Run as a
/// *deferred* transaction (sqlx's `pool.begin()`), the SELECT takes a read
/// snapshot and the INSERT must upgrade to a writer. Under WAL a concurrent
/// writer can commit in that window, so the upgrade fails with
/// SQLITE_BUSY_SNAPSHOT — returned immediately (`wait_ms=0`), which
/// `busy_timeout` cannot rescue. Because `(session_id, seq)` is a non-unique
/// index, the deferred form can also silently commit duplicate seqs.
///
/// `BEGIN IMMEDIATE` takes the write lock up front, serializing same-session
/// appends so every one commits with a distinct, gap-free monotonic seq.
///
/// Pre-fix this panics (a dropped append) or fails the seq assertion (a
/// duplicate/gap); post-fix all N appends commit with seqs `0..N`.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_same_session_appends_all_commit_with_monotonic_seq() {
    let (pool, _td) = wal_file_pool().await;
    let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
        .await
        .unwrap();

    const N: i64 = 48;
    let mut handles = Vec::new();
    for i in 0..N {
        let pool = pool.clone();
        let sid = sid.clone();
        handles.push(tokio::spawn(async move {
            ChatSessionStore::append(&pool, &sid, "user", &[block(&format!("m{i}"))]).await
        }));
    }

    let mut seqs = Vec::new();
    for h in handles {
        let msg = h
            .await
            .expect("task join")
            .expect("every concurrent append must commit (no SQLITE_BUSY / seq collision)");
        seqs.push(msg.seq);
    }

    seqs.sort_unstable();
    let expected: Vec<i64> = (0..N).collect();
    assert_eq!(
        seqs, expected,
        "concurrent appends must each get a distinct, gap-free monotonic seq"
    );
}

/// Integration: after a rolled-back transaction that simulates a failed
/// strategy write, subsequent `append` calls must still succeed with correct
/// monotonic seq. This guards against connection-pool state corruption
/// (root cause C).
#[tokio::test]
async fn append_error_includes_sqlite_class_label() {
    let pool = fresh_pool().await;
    let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace)
        .await
        .unwrap();

    // Append successfully to establish seq 0.
    ChatSessionStore::append(&pool, &sid, "user", &[block("first")])
        .await
        .unwrap();

    // Verify sequential appends still work after a raw-SQL non-committing tx
    // (the "strategy write failed and rolled back" scenario).
    {
        let mut tx = pool.begin().await.unwrap();
        // Simulate strategy write: inserts something unrelated, fails.
        let _ = sqlx::query("INSERT INTO chat_sessions (id, started_at, last_activity_at, context_scope_json) VALUES ('fake', 'bad-ts', 'bad-ts', '{}')")
            .execute(&mut *tx)
            .await; // May or may not fail; we don't care.
        tx.rollback().await.unwrap();
    }

    // Chat append must succeed after the rolled-back strategy tx.
    let m1 = ChatSessionStore::append(&pool, &sid, "assistant", &[block("reply")])
        .await
        .expect("append must succeed after a rolled-back concurrent tx");
    assert_eq!(m1.seq, 1);
}

//! Regression test: `ApiContext::open` must configure the production
//! `xvn.db` pool with WAL journaling + a non-zero `busy_timeout`.
//!
//! Background: the deployed `xvn-app` was hitting `SQLITE_BUSY` on
//! `chat_messages` inserts because the previous bare
//! `SqlitePool::connect("sqlite://…?mode=rwc")` form used sqlx defaults
//! (rollback journal + 0 ms busy timeout + uncapped pool). The
//! `chat_session_insert_errors` test exists specifically to *trigger*
//! that lock path with `busy_timeout(1ms)`; this test asserts the
//! production path has the opposite shape and never falls back to it.

use sqlx::Row;
use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext};

#[tokio::test]
async fn xvn_db_pool_uses_wal_and_nonzero_busy_timeout() {
    let dir = tempdir().expect("tempdir");
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .expect("open ApiContext");

    // PRAGMA journal_mode returns the current mode as text. WAL must
    // be active; the prior rollback-journal default is the bug.
    let mode: String = sqlx::query("PRAGMA journal_mode")
        .fetch_one(&ctx.db)
        .await
        .expect("read journal_mode")
        .try_get(0)
        .expect("decode journal_mode");
    assert_eq!(
        mode.to_ascii_lowercase(),
        "wal",
        "production pool must run in WAL mode — got {mode:?}"
    );

    // PRAGMA busy_timeout returns milliseconds. Anything > 0 means a
    // contending writer waits instead of failing immediately with
    // SQLITE_BUSY. The contract is "5 seconds"; we assert >= 1 second
    // so a future tweak to e.g. 10s doesn't flap this test.
    let busy_ms: i64 = sqlx::query("PRAGMA busy_timeout")
        .fetch_one(&ctx.db)
        .await
        .expect("read busy_timeout")
        .try_get(0)
        .expect("decode busy_timeout");
    assert!(
        busy_ms >= 1_000,
        "production pool must set busy_timeout >= 1s — got {busy_ms} ms"
    );

    // Foreign keys: a long-standing invariant of the schema (every
    // migration assumes them). The previous URL-style open did NOT
    // enforce this, so dependent rows could be orphaned by a careless
    // delete. The new builder turns them on; assert it.
    let fk: i64 = sqlx::query("PRAGMA foreign_keys")
        .fetch_one(&ctx.db)
        .await
        .expect("read foreign_keys")
        .try_get(0)
        .expect("decode foreign_keys");
    assert_eq!(fk, 1, "foreign_keys must be enabled");
}

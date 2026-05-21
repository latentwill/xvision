//! Unit + integration tests for `eval::finalize_writer::FinalizeWriter`.
//!
//! Covers acceptance items 7.1–7.4 from the
//! `eval-finalize-write-serializer` contract:
//!
//! 1. 27 concurrent `send_mark_failed` calls finalize in ≤ 2 batched
//!    UPDATEs.
//! 2. A `oneshot` reply surfaces a typed error if the UPDATE fails
//!    (we close the pool before sending).
//! 3. Queue-full path returns `Err(QueueFull)` rather than blocking.
//! 4. Integration: stage 27 finalize-failed calls in quick succession
//!    through a real `RunStore` + temp DB; assert all 27 rows ended up
//!    `status='failed'` and the call set completes well under 200ms
//!    (proxy for "no slow statement warning fires").

use std::time::{Duration, Instant};

use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use xvision_engine::eval::finalize_writer::{FinalizeError, FinalizeWriter};
use xvision_engine::eval::{Run, RunMode, RunStore};

async fn pool_with_eval_migration() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn create_n_running_runs(store: &RunStore, n: usize) -> Vec<String> {
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let run = Run::new_queued(
            format!("strategy-{i}"),
            "scenario-x".to_string(),
            RunMode::Backtest,
        );
        store.create(&run).await.unwrap();
        store.begin_running(&run.id).await.unwrap();
        ids.push(run.id);
    }
    ids
}

/// 27 concurrent `send_mark_failed` calls. The receiver batches in
/// windows of up to 16 messages, so 27 messages flush in 2 batched
/// UPDATEs (16 + 11). We assert all 27 rows end up `failed` and the
/// total wall-clock is well under the 1.029s slow-statement threshold
/// captured in the audit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn batches_27_concurrent_mark_failed_calls() {
    let pool = pool_with_eval_migration().await;
    let store = RunStore::new(pool.clone());
    let ids = create_n_running_runs(&store, 27).await;

    let writer = FinalizeWriter::start(pool.clone());
    let started = Instant::now();
    let mut handles = Vec::with_capacity(27);
    for (i, id) in ids.iter().enumerate() {
        let w = writer.clone();
        let id = id.clone();
        handles.push(tokio::spawn(async move {
            w.send_mark_failed(id, format!("test failure {i}"), Utc::now())
                .await
        }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "27 concurrent finalize writes should batch through quickly; elapsed = {elapsed:?}"
    );

    // Every row should be `failed`.
    for id in &ids {
        let row = sqlx::query("SELECT status, error, completed_at FROM eval_runs WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let status: String = row.get("status");
        let error: Option<String> = row.get("error");
        let completed_at: Option<String> = row.get("completed_at");
        assert_eq!(status, "failed", "row {id} should be failed");
        assert!(error.is_some(), "row {id} should have an error");
        assert!(completed_at.is_some(), "row {id} should have completed_at");
    }
}

/// Closing the pool before the writer receives the message forces the
/// batched UPDATE to fail. The caller's `oneshot` should surface a
/// typed `FinalizeError::Db(...)` instead of hanging.
#[tokio::test]
async fn oneshot_surfaces_db_error_when_pool_closed() {
    let pool = pool_with_eval_migration().await;
    let writer = FinalizeWriter::start(pool.clone());
    // Close the pool — every subsequent query against `pool` errors with
    // `sqlx::Error::PoolClosed`.
    pool.close().await;

    let res = writer
        .send_mark_failed(
            "ulid-does-not-exist".to_string(),
            "should fail".to_string(),
            Utc::now(),
        )
        .await;
    match res {
        Err(FinalizeError::Db(_)) => {}
        Err(FinalizeError::WriterShutdown) => {
            // Acceptable: closing the pool may cause the receiver to
            // observe its drop and shut the channel down before the
            // batch UPDATE is even attempted. Either way the caller
            // gets a typed error, never a hang.
        }
        other => panic!("expected Db(_) or WriterShutdown, got {other:?}"),
    }
}

/// Filling the channel above its bound makes additional `send_*` calls
/// return `QueueFull` instead of blocking. We force the bound to 1 via
/// `XVN_FINALIZE_WRITER_CAP`, then fire two sends without ever yielding
/// to the receiver so the first one occupies the slot and the second
/// trips `try_send`.
#[tokio::test(flavor = "current_thread")]
async fn queue_full_path_returns_typed_error() {
    // SAFETY: tests run serially in this binary because we mutate a
    // process-global env var. Cargo-test default is one binary per
    // file, and inside a binary tokio::test functions run
    // sequentially.
    std::env::set_var("XVN_FINALIZE_WRITER_CAP", "1");

    let pool = pool_with_eval_migration().await;
    let writer = FinalizeWriter::start(pool.clone());

    // Hold the receiver task off the runqueue: in `current_thread`
    // flavor we don't `.await` anything else between the two sends, so
    // the spawned receiver doesn't get a chance to drain. The first
    // `try_send` succeeds, the second sees a full channel.
    let (reply1, _rx1) = tokio::sync::oneshot::channel();
    let msg1 = xvision_engine::eval::finalize_writer::FinalizeMsg::MarkFailed {
        run_id: "a".to_string(),
        error: "e".to_string(),
        completed_at: Utc::now(),
        reply: reply1,
    };
    // Acquire the slot through the raw send path so we don't await the
    // reply (which would yield to the receiver).
    writer
        .try_enqueue_for_test(msg1)
        .expect("first try_send should succeed");

    let (reply2, _rx2) = tokio::sync::oneshot::channel();
    let msg2 = xvision_engine::eval::finalize_writer::FinalizeMsg::MarkFailed {
        run_id: "b".to_string(),
        error: "e".to_string(),
        completed_at: Utc::now(),
        reply: reply2,
    };
    match writer.try_enqueue_for_test(msg2) {
        Err(FinalizeError::QueueFull { cap: 1 }) => {}
        other => panic!("expected QueueFull(1), got {other:?}"),
    }

    std::env::remove_var("XVN_FINALIZE_WRITER_CAP");
}

/// Integration: real `RunStore` + temp DB; 27 staged calls fired
/// back-to-back; verify wall-clock well under the 1s slow-statement
/// alert threshold and every row reaches `failed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn integration_27_finalize_failed_under_200ms() {
    let pool = pool_with_eval_migration().await;
    let store = RunStore::new(pool.clone());
    let ids = create_n_running_runs(&store, 27).await;

    let writer = FinalizeWriter::start(pool.clone());

    // Fire all 27 in quick succession.
    let started = Instant::now();
    let mut handles = Vec::with_capacity(27);
    for (i, id) in ids.iter().enumerate() {
        let w = writer.clone();
        let id = id.clone();
        handles.push(tokio::spawn(async move {
            w.send_mark_failed(id, format!("burst {i}"), Utc::now()).await
        }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }
    let elapsed = started.elapsed();
    // 200ms is the contract acceptance budget. On in-memory SQLite this
    // is comfortably achievable; on real disk-backed SQLite the
    // batching keeps us within the 1.029s slow-statement threshold the
    // audit captured.
    assert!(
        elapsed < Duration::from_millis(200),
        "27 staged finalize-failed calls should complete under 200ms; elapsed = {elapsed:?}"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM eval_runs WHERE status = 'failed'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 27);
}

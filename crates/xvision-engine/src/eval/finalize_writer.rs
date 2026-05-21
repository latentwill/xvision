//! Batched, single-writer finalize serializer for `eval_runs` status
//! transitions.
//!
//! ## Why this module exists
//!
//! The 2026-05-19 incident audit captured:
//!
//! ```text
//! WARN sqlx::query: slow statement: execution time exceeded alert threshold
//!     UPDATE eval_runs SET status='failed' ... elapsed=1.029s
//! ```
//!
//! under a 27-runs-in-15s storm. Every executor task was racing every other
//! one to `UPDATE eval_runs SET status = 'failed'` on the same SQLite file,
//! and the writer queue at the SQLite end serialized them anyway — but only
//! after each task paid the full per-statement overhead.
//!
//! [`FinalizeWriter`] funnels every finalize write through a single
//! background task. The task collects contiguous messages of the same kind
//! (all `MarkFailed` OR all `MarkCompleted`) in a 50ms / 16-msg window and
//! issues one batched UPDATE per window. Each caller still gets its own
//! typed `Result` back via a `oneshot::Sender` reply.
//!
//! ## Pairs with
//!
//! - **F-1** (`crates/xvision-engine/src/eval/concurrency.rs`, PR #361):
//!   caps simultaneous launches per `(provider, model)` so bursts can't
//!   form upstream.
//! - **F-3** (`crates/xvision-engine/src/eval/watchdog.rs`, PR #345):
//!   sweeps stuck runs. The watchdog and the boot-sweep
//!   (`fail_orphan_runs`) currently call [`RunStore::fail_active`] /
//!   [`RunStore::fail_active_runs`] directly — keeping them on the direct
//!   path is fine because they fire at most once per tick (watchdog) or
//!   once per process start (boot sweep), so they never produce a burst.
//!
//! Together with the F-1 cap and this serializer, the audit's P0
//! root-cause chain closes.
//!
//! ## Channel sizing
//!
//! Bounded `mpsc` with a default capacity of 256 (override via
//! `XVN_FINALIZE_WRITER_CAP`). 256 is well above the audit's 27-burst and
//! the F-1 per-model cap of 4, so under normal load `try_send` will
//! always succeed.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

/// Default channel capacity. Override with `XVN_FINALIZE_WRITER_CAP`.
const DEFAULT_CAPACITY: usize = 256;

/// Max messages collapsed into one batched UPDATE.
const BATCH_MAX: usize = 16;

/// Wall-clock window the receiver waits to accumulate more messages of the
/// same kind before flushing.
const BATCH_WINDOW: Duration = Duration::from_millis(50);

/// One queued finalize write. Each variant carries a `oneshot::Sender`
/// reply so the caller surfaces the actual DB outcome (success or shared
/// batch error) instead of fire-and-forget.
#[derive(Debug)]
pub enum FinalizeMsg {
    MarkFailed {
        run_id: String,
        error: String,
        completed_at: DateTime<Utc>,
        reply: oneshot::Sender<Result<(), FinalizeError>>,
    },
    MarkCompleted {
        run_id: String,
        metrics_json: String,
        completed_at: DateTime<Utc>,
        reply: oneshot::Sender<Result<(), FinalizeError>>,
    },
}

impl FinalizeMsg {
    fn is_failed(&self) -> bool {
        matches!(self, FinalizeMsg::MarkFailed { .. })
    }
}

/// Typed error surface. Each variant signals a different recovery path:
/// `QueueFull` is safe to retry; `WriterShutdown` usually means the
/// caller is itself aborting; `Db` is a real SQL error and should be
/// logged / surfaced.
#[derive(Debug, Error)]
pub enum FinalizeError {
    #[error("finalize writer queue full (cap={cap})")]
    QueueFull { cap: usize },
    #[error("finalize writer has shut down")]
    WriterShutdown,
    #[error("db error in batched finalize update: {0}")]
    Db(String),
}

impl FinalizeError {
    fn from_sqlx(e: &sqlx::Error) -> Self {
        FinalizeError::Db(e.to_string())
    }
}

/// Bounded mpsc front-end. Callers hold an `Arc<FinalizeWriter>` (stored
/// on `ApiContext::finalize_writer`) and dispatch with
/// [`Self::send_mark_failed`] / [`Self::send_mark_completed`].
#[derive(Debug)]
pub struct FinalizeWriter {
    tx: mpsc::Sender<FinalizeMsg>,
    capacity: usize,
}

impl FinalizeWriter {
    /// Spawn the background receiver task and return the front-end
    /// handle. The receiver is driven on the current Tokio runtime.
    ///
    /// Dropping the returned `Arc<FinalizeWriter>` closes the channel
    /// and the receiver task drains the queue then exits.
    pub fn start(pool: SqlitePool) -> Arc<Self> {
        let capacity = capacity_from_env();
        let (tx, rx) = mpsc::channel::<FinalizeMsg>(capacity);
        tokio::spawn(run_receiver(pool, rx));
        Arc::new(Self { tx, capacity })
    }

    /// Channel capacity. Exposed for diagnostics / tests.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Enqueue a `MarkFailed`. Returns once the batched UPDATE that
    /// included this row has committed (or errored).
    pub async fn send_mark_failed(
        &self,
        run_id: String,
        error: String,
        completed_at: DateTime<Utc>,
    ) -> Result<(), FinalizeError> {
        let (reply, rx) = oneshot::channel();
        let msg = FinalizeMsg::MarkFailed {
            run_id,
            error,
            completed_at,
            reply,
        };
        self.enqueue(msg).await?;
        rx.await.map_err(|_| FinalizeError::WriterShutdown)?
    }

    /// Enqueue a `MarkCompleted`. Returns once the batched UPDATE that
    /// included this row has committed (or errored).
    pub async fn send_mark_completed(
        &self,
        run_id: String,
        metrics_json: String,
        completed_at: DateTime<Utc>,
    ) -> Result<(), FinalizeError> {
        let (reply, rx) = oneshot::channel();
        let msg = FinalizeMsg::MarkCompleted {
            run_id,
            metrics_json,
            completed_at,
            reply,
        };
        self.enqueue(msg).await?;
        rx.await.map_err(|_| FinalizeError::WriterShutdown)?
    }

    /// Try-send only — never blocks. Returns `QueueFull` immediately if
    /// the channel is at capacity. Production code uses
    /// [`Self::send_mark_failed`] / [`Self::send_mark_completed`] which
    /// also await the reply. Tests exercise the queue-full path through
    /// [`Self::try_enqueue_for_test`].
    #[doc(hidden)]
    pub fn try_enqueue_for_test(&self, msg: FinalizeMsg) -> Result<(), FinalizeError> {
        self.try_enqueue(msg)
    }

    fn try_enqueue(&self, msg: FinalizeMsg) -> Result<(), FinalizeError> {
        match self.tx.try_send(msg) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(FinalizeError::QueueFull { cap: self.capacity }),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(FinalizeError::WriterShutdown),
        }
    }

    async fn enqueue(&self, msg: FinalizeMsg) -> Result<(), FinalizeError> {
        // Bounded queue: `try_send` so callers see `QueueFull` instantly
        // rather than blocking on the channel under burst. The capacity
        // (256 default) is well above the audited 27-run burst and the
        // F-1 per-model cap of 4, so under healthy load this always
        // succeeds on the first try.
        self.try_enqueue(msg)
    }
}

fn capacity_from_env() -> usize {
    std::env::var("XVN_FINALIZE_WRITER_CAP")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_CAPACITY)
}

/// Receiver loop. Pulls one message, then either:
///   - flushes immediately if it is the only thing in the queue, or
///   - accumulates contiguous messages of the same kind (`MarkFailed` vs
///     `MarkCompleted`) for up to `BATCH_WINDOW` or `BATCH_MAX`, then
///     flushes the homogeneous batch.
///
/// Mixed types in the same window flush separately — the moment a
/// different kind shows up we flush what we have and start a new batch
/// with the new kind.
async fn run_receiver(pool: SqlitePool, mut rx: mpsc::Receiver<FinalizeMsg>) {
    while let Some(first) = rx.recv().await {
        let mut batch: Vec<FinalizeMsg> = Vec::with_capacity(BATCH_MAX);
        let first_is_failed = first.is_failed();
        batch.push(first);

        // Try to extend the batch with same-kind messages already
        // waiting on the channel. We bound by both `BATCH_MAX` and
        // `BATCH_WINDOW`.
        let deadline = tokio::time::Instant::now() + BATCH_WINDOW;
        while batch.len() < BATCH_MAX {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(msg)) => {
                    if msg.is_failed() == first_is_failed {
                        batch.push(msg);
                    } else {
                        // Different kind — flush what we have, then
                        // start a fresh batch with this message as its
                        // seed. We re-enter the outer loop after flush
                        // by handling this message inline.
                        flush_batch(&pool, batch).await;
                        // Process this stray message as the seed of a
                        // new batch. Recurse via a single-shot batch
                        // path — simplest correct implementation.
                        let stray_is_failed = msg.is_failed();
                        let mut new_batch = Vec::with_capacity(BATCH_MAX);
                        new_batch.push(msg);
                        // Drain more same-kind messages within a fresh
                        // window.
                        let new_deadline = tokio::time::Instant::now() + BATCH_WINDOW;
                        while new_batch.len() < BATCH_MAX {
                            let r = new_deadline.saturating_duration_since(tokio::time::Instant::now());
                            if r.is_zero() {
                                break;
                            }
                            match tokio::time::timeout(r, rx.recv()).await {
                                Ok(Some(m)) => {
                                    if m.is_failed() == stray_is_failed {
                                        new_batch.push(m);
                                    } else {
                                        flush_batch(&pool, new_batch).await;
                                        new_batch = Vec::with_capacity(BATCH_MAX);
                                        new_batch.push(m);
                                    }
                                }
                                Ok(None) => {
                                    flush_batch(&pool, new_batch).await;
                                    return;
                                }
                                Err(_) => break,
                            }
                        }
                        flush_batch(&pool, new_batch).await;
                        // Outer loop continues — wait for next message.
                        batch = Vec::new();
                        break;
                    }
                }
                Ok(None) => {
                    // Channel closed. Flush whatever we have and exit.
                    flush_batch(&pool, batch).await;
                    return;
                }
                Err(_) => {
                    // Window elapsed.
                    break;
                }
            }
        }
        if !batch.is_empty() {
            flush_batch(&pool, batch).await;
        }
    }
}

/// Issue one batched UPDATE for the messages in `batch`. All entries are
/// guaranteed to be the same variant (caller-enforced).
async fn flush_batch(pool: &SqlitePool, batch: Vec<FinalizeMsg>) {
    if batch.is_empty() {
        return;
    }
    if batch[0].is_failed() {
        flush_failed(pool, batch).await;
    } else {
        flush_completed(pool, batch).await;
    }
}

async fn flush_failed(pool: &SqlitePool, batch: Vec<FinalizeMsg>) {
    // Build per-row CASE arms so each id gets its own completed_at and
    // error string in a single statement. SQLite is happy with this
    // shape and it preserves the per-call timestamp + reason that the
    // direct `fail_active` path used to write.
    let mut ids: Vec<String> = Vec::with_capacity(batch.len());
    let mut completed_at_arms: Vec<(String, String)> = Vec::with_capacity(batch.len());
    let mut error_arms: Vec<(String, String)> = Vec::with_capacity(batch.len());
    let mut replies: Vec<oneshot::Sender<Result<(), FinalizeError>>> = Vec::with_capacity(batch.len());

    for msg in batch {
        match msg {
            FinalizeMsg::MarkFailed {
                run_id,
                error,
                completed_at,
                reply,
            } => {
                completed_at_arms.push((run_id.clone(), completed_at.to_rfc3339()));
                error_arms.push((run_id.clone(), error));
                ids.push(run_id);
                replies.push(reply);
            }
            FinalizeMsg::MarkCompleted { .. } => unreachable!("flush_failed called with completed msg"),
        }
    }

    let placeholders = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(", ");
    let completed_case = build_case_clause(&completed_at_arms, "completed_at");
    let error_case = build_case_clause(&error_arms, "error");

    let sql = format!(
        "UPDATE eval_runs \
         SET status = 'failed', \
             completed_at = {completed_case}, \
             error = {error_case} \
         WHERE id IN ({placeholders}) \
           AND status IN ('queued', 'running')",
    );

    let mut q = sqlx::query(&sql);
    // Bind order: CASE arms (id, value) pairs for completed_at, then for
    // error, then the IN-list ids. Matches `build_case_clause`.
    for (id, ts) in &completed_at_arms {
        q = q.bind(id).bind(ts);
    }
    for (id, err) in &error_arms {
        q = q.bind(id).bind(err);
    }
    for id in &ids {
        q = q.bind(id);
    }

    let res = q.execute(pool).await;
    notify_replies(replies, &res);
}

async fn flush_completed(pool: &SqlitePool, batch: Vec<FinalizeMsg>) {
    let mut ids: Vec<String> = Vec::with_capacity(batch.len());
    let mut completed_at_arms: Vec<(String, String)> = Vec::with_capacity(batch.len());
    let mut metrics_arms: Vec<(String, String)> = Vec::with_capacity(batch.len());
    let mut replies: Vec<oneshot::Sender<Result<(), FinalizeError>>> = Vec::with_capacity(batch.len());

    for msg in batch {
        match msg {
            FinalizeMsg::MarkCompleted {
                run_id,
                metrics_json,
                completed_at,
                reply,
            } => {
                completed_at_arms.push((run_id.clone(), completed_at.to_rfc3339()));
                metrics_arms.push((run_id.clone(), metrics_json));
                ids.push(run_id);
                replies.push(reply);
            }
            FinalizeMsg::MarkFailed { .. } => unreachable!("flush_completed called with failed msg"),
        }
    }

    let placeholders = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(", ");
    let completed_case = build_case_clause(&completed_at_arms, "completed_at");
    let metrics_case = build_case_clause(&metrics_arms, "metrics_json");

    let sql = format!(
        "UPDATE eval_runs \
         SET status = 'completed', \
             completed_at = {completed_case}, \
             metrics_json = {metrics_case} \
         WHERE id IN ({placeholders}) \
           AND status IN ('queued', 'running')",
    );

    let mut q = sqlx::query(&sql);
    for (id, ts) in &completed_at_arms {
        q = q.bind(id).bind(ts);
    }
    for (id, mj) in &metrics_arms {
        q = q.bind(id).bind(mj);
    }
    for id in &ids {
        q = q.bind(id);
    }

    let res = q.execute(pool).await;
    notify_replies(replies, &res);
}

/// Build a `CASE WHEN id = ? THEN ? ... ELSE <column> END` clause. The
/// `ELSE <column>` keeps the existing value if somehow no arm matches
/// (defensive; the IN-list should make it unreachable).
fn build_case_clause(arms: &[(String, String)], else_col: &str) -> String {
    if arms.is_empty() {
        return else_col.to_string();
    }
    let mut s = String::from("CASE");
    for _ in arms {
        s.push_str(" WHEN id = ? THEN ?");
    }
    s.push_str(" ELSE ");
    s.push_str(else_col);
    s.push_str(" END");
    s
}

fn notify_replies(
    replies: Vec<oneshot::Sender<Result<(), FinalizeError>>>,
    res: &Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error>,
) {
    match res {
        Ok(_) => {
            for r in replies {
                let _ = r.send(Ok(()));
            }
        }
        Err(e) => {
            for r in replies {
                let _ = r.send(Err(FinalizeError::from_sqlx(e)));
            }
        }
    }
}

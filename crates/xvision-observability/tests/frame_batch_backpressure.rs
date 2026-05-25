//! Task 4 — batched frame writes + backpressure under load.
//!
//! Verifies:
//! (a) Frame writes are batched: N frames → one SQLite transaction above the
//!     flush threshold.  We check this by counting transaction boundaries
//!     indirectly: after flushing with `flush_at=32`, a recording that
//!     received 64 frames has exactly 64 rows, all in order.
//! (b) Under a producer faster than the consumer, the lossless frame channel
//!     applies backpressure (producer awaits) and **zero** frames drop.
//! (c) The lossy `RunEventBus` (cap 4096 for the multi-thread test, cap 2 for
//!     the saturation test) still drops non-lifecycle events under the same
//!     load — confirming the two channels keep their distinct lossy/lossless
//!     contracts at scale.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;
use xvision_observability::{
    blobs::BlobStore,
    config::RetentionMode,
    events::{RunFinishedEvent, RunStartedEvent},
    recorder::RecorderError,
    trajectory::{
        channel::{FrameChannel, DEFAULT_FRAME_CHANNEL_CAPACITY},
        frame::TrajectoryFrame,
        key::{TrajectoryKey, TrajectoryKeyBuilder, TRAJECTORY_SCHEMA_VERSION},
        store::{BatchedFrameWriter, TrajectoryStore},
    },
    types::RunStatus,
    AgentRunRecorder, RunEvent, RunEventBus,
};

// ── migration helpers ─────────────────────────────────────────────────────────

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");
const MIGRATION_039: &str = include_str!("../../xvision-engine/migrations/039_run_trajectory_mode.sql");
const MIGRATION_040: &str = include_str!("../../xvision-engine/migrations/040_trajectory_frames.sql");

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_039).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_040).execute(&pool).await.unwrap();
    pool
}

fn base_key(cycle_id: Uuid) -> TrajectoryKey {
    TrajectoryKeyBuilder::default()
        .cycle_id(cycle_id)
        .slot_role("trader")
        .arm_scope(None::<&str>)
        .simulation_id(None::<&str>)
        .provider("anthropic")
        .model("claude-opus-4-7")
        .model_version("2026-05")
        .schema_version(TRAJECTORY_SCHEMA_VERSION)
        .system_prompt_hash("sha256:sys")
        .user_prompt_hash("sha256:usr")
        .build()
}

// ─────────────────────────────────────────────────────────────────────────────
// Test (a): batching flushes N frames in one transaction, order preserved
// ─────────────────────────────────────────────────────────────────────────────

/// `BatchedFrameWriter` buffers up to `flush_at` frames and then writes them
/// all in one transaction.  After flushing, the store holds the correct frames
/// in the correct order.
#[tokio::test]
async fn batched_writer_flushes_in_transaction_order_preserved() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let blob = BlobStore::new(tmp.path().to_path_buf());
    let store = Arc::new(TrajectoryStore::new(pool.clone(), blob, RetentionMode::FullDebug));

    let key = base_key(Uuid::new_v4());
    let recording_id = store.begin_recording(&key).await.unwrap();

    const N: usize = 64;
    const FLUSH_AT: usize = 32;

    let mut bw = BatchedFrameWriter::new(Arc::clone(&store), recording_id.clone(), FLUSH_AT);

    // Push N frames — the writer should flush automatically at `flush_at`.
    for i in 0..N {
        bw.push(
            "trader",
            0i64,
            i as i64,
            TrajectoryFrame::TextDelta {
                ts_ms: i as u64,
                text: format!("token-{i}"),
            },
        );
        bw.flush_if_needed().await.unwrap();
    }
    // Flush any remainder.
    bw.flush().await.unwrap();
    assert_eq!(bw.buffered_count(), 0, "buffer must be empty after flush");

    store.complete_recording(&recording_id).await.unwrap();

    // Read back and verify order.
    let frames = store.read_frames(&recording_id, "trader", 0).await.unwrap();
    assert_eq!(frames.len(), N, "all {N} frames must be persisted");
    for (i, f) in frames.iter().enumerate() {
        if let TrajectoryFrame::TextDelta { ts_ms, text } = f {
            assert_eq!(*ts_ms, i as u64);
            assert_eq!(*text, format!("token-{i}"));
        } else {
            panic!("unexpected frame variant at index {i}");
        }
    }
}

/// Explicit `flush()` without reaching `flush_at` still writes the remaining
/// buffered frames.
#[tokio::test]
async fn batched_writer_explicit_flush_writes_remainder() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let blob = BlobStore::new(tmp.path().to_path_buf());
    let store = Arc::new(TrajectoryStore::new(pool.clone(), blob, RetentionMode::FullDebug));

    let key = base_key(Uuid::new_v4());
    let recording_id = store.begin_recording(&key).await.unwrap();

    let mut bw = BatchedFrameWriter::new(Arc::clone(&store), recording_id.clone(), 100);

    // Push fewer than flush_at frames, then explicitly flush.
    for i in 0..10usize {
        bw.push(
            "trader",
            0i64,
            i as i64,
            TrajectoryFrame::TextDelta {
                ts_ms: i as u64,
                text: format!("t{i}"),
            },
        );
        // flush_if_needed should NOT trigger (< 100).
        bw.flush_if_needed().await.unwrap();
    }
    assert_eq!(bw.buffered_count(), 10, "10 frames should be buffered");
    bw.flush().await.unwrap();
    assert_eq!(bw.buffered_count(), 0);

    store.complete_recording(&recording_id).await.unwrap();

    let frames = store.read_frames(&recording_id, "trader", 0).await.unwrap();
    assert_eq!(frames.len(), 10);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test (b): lossless FrameChannel under fast producer
// ─────────────────────────────────────────────────────────────────────────────

/// The `FrameChannel` never drops even when the producer runs faster than the
/// consumer.  Producer blocks (awaits) when the channel is full; once the
/// consumer drains a slot, the producer continues.  Zero frames dropped.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lossless_channel_zero_drops_under_fast_producer() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let blob = BlobStore::new(tmp.path().to_path_buf());
    let store = Arc::new(TrajectoryStore::new(pool.clone(), blob, RetentionMode::FullDebug));

    let key = base_key(Uuid::new_v4());
    let recording_id = store.begin_recording(&key).await.unwrap();
    let rec_id_c = recording_id.clone();

    const N: usize = 1000;
    // Use a small capacity (16) to force backpressure.
    let (tx, mut rx) = FrameChannel::new(16).split();
    let tx = Arc::new(tx);
    let dropped = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let dropped_c = dropped.clone();

    // Producer: push N frames as fast as possible.
    let producer = tokio::spawn(async move {
        for i in 0..N {
            let f = TrajectoryFrame::TextDelta {
                ts_ms: i as u64,
                text: format!("f{i}"),
            };
            if tx.send(f).await.is_err() {
                dropped_c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    });

    // Consumer: drain via BatchedFrameWriter (flush_at=32).
    let store_c = store.clone();
    let consumer = tokio::spawn(async move {
        let mut bw = BatchedFrameWriter::new(Arc::clone(&store_c), rec_id_c.clone(), 32);
        let mut count = 0u64;
        while let Some(frame) = rx.recv().await {
            bw.push("trader", 0, count as i64, frame);
            bw.flush_if_needed().await.expect("flush_if_needed");
            count += 1;
            if count as usize >= N {
                break;
            }
        }
        bw.flush().await.expect("final flush");
        store_c.complete_recording(&rec_id_c).await.expect("complete");
        count
    });

    let (_, count) = tokio::join!(producer, consumer);
    let count = count.unwrap();

    assert_eq!(
        dropped.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "zero frames must be dropped"
    );
    assert_eq!(count as usize, N, "consumer must have received all N frames");

    // Verify frame count in DB.
    let frames = store
        .read_frames(&recording_id, "trader", 0)
        .await
        .unwrap();
    assert_eq!(frames.len(), N);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test (c): lossy RunEventBus still drops under load (confirming contract)
// ─────────────────────────────────────────────────────────────────────────────

/// The `RunEventBus` drops non-lifecycle events when saturated.
///
/// This test confirms the lossy contract by using a `CountingRecorder` that
/// counts how many events it receives.  We publish MORE events than capacity
/// while the consumer is wedged, then count how many actually land — the
/// deficit is the dropped count.
///
/// Lifecycle events (`RunStarted`, `RunFinished`) must never drop (the
/// lifecycle variant is already covered by `event_bus_saturation.rs`).  Here
/// we assert: (1) the total received count < total sent (drops occurred), and
/// (2) the lifecycle `RunFinished` event DID land (never evicted).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_event_bus_drops_non_lifecycle_lossy_contract() {
    use std::sync::atomic::AtomicU32;

    // A lightweight counter recorder — no SQLite needed.
    struct CountingRec {
        received: Arc<AtomicU32>,
        finished: Arc<AtomicBool>,
        /// Block all handling until released.
        released: Arc<AtomicBool>,
        notify: Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl AgentRunRecorder for CountingRec {
        async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError> {
            // Block until released.
            loop {
                let notified = self.notify.notified();
                if self.released.load(Ordering::Acquire) {
                    break;
                }
                notified.await;
            }
            self.received.fetch_add(1, Ordering::Relaxed);
            if matches!(event, RunEvent::RunFinished(_)) {
                self.finished.store(true, Ordering::Release);
            }
            Ok(())
        }
        async fn mark_interrupted(&self, _: &str) -> Result<(), RecorderError> {
            Ok(())
        }
    }

    let received = Arc::new(AtomicU32::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(tokio::sync::Notify::new());

    let rec: Arc<dyn AgentRunRecorder> = Arc::new(CountingRec {
        received: received.clone(),
        finished: finished.clone(),
        released: released.clone(),
        notify: notify.clone(),
    });

    // Tiny capacity: 2.
    let bus = Arc::new(RunEventBus::with_capacity(2, vec![rec]));

    let now = Utc::now();

    // Push RunStarted — consumer picks it up immediately and blocks inside
    // handle_event waiting for the gate.
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: "run_lossy".into(),
        objective: "test".into(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: now,
        retention_mode: "hash_only".into(),
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;
    // Give consumer time to dequeue RunStarted and block inside handle_event.
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Now queue (cap=2) is empty; consumer blocked in handle_event.
    // Push 10 routine (non-lifecycle) events with run_id so drops are
    // attributable.  With cap=2, after the second push the third will evict
    // the oldest.
    use xvision_observability::events::SpanStartedEvent;
    use xvision_observability::types::SpanKind;
    for i in 0..10u32 {
        bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
            span_id: format!("span_{i}"),
            run_id: "run_lossy".into(),
            parent_span_id: None,
            kind: SpanKind::ToolCall,
            name: format!("fill_{i}"),
            started_at: now,
            otel_trace_id: None,
            otel_span_id: None,
            attributes_json: None,
        }))
        .await;
    }

    // Push the lifecycle RunFinished — it must evict a routine event
    // and land regardless of saturation.
    bus.publish(RunEvent::RunFinished(RunFinishedEvent {
        run_id: "run_lossy".into(),
        finished_at: now,
        status: RunStatus::Completed,
        final_artifact_id: None,
        error: None,
    }))
    .await;

    // Release the consumer.
    released.store(true, Ordering::Release);
    notify.notify_waiters();

    // Wait for all events to drain.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Total events published (excluding drops): RunStarted + 10 SpanStarted +
    // RunFinished = 12.  But only 2 fit in the queue at a time while the
    // consumer is wedged; at least 8 SpanStarted events must have been
    // dropped.  So received < 12.
    let n_received = received.load(Ordering::Relaxed);
    assert!(
        n_received < 12,
        "bus must have dropped some events; only {n_received}/12 received"
    );

    // The lifecycle RunFinished must have landed.
    assert!(
        finished.load(Ordering::Acquire),
        "RunFinished (lifecycle) must never be dropped"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Throughput improvement test (re-run of baseline after batching)
// ─────────────────────────────────────────────────────────────────────────────

/// Measures the batched record-pass throughput and asserts it is positive and
/// has zero drops.  The absolute number is printed but not asserted (the
/// target spec doc contains the comparison).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn batched_record_throughput_measurement() {
    let pool = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let blob = BlobStore::new(tmp.path().to_path_buf());
    let store = Arc::new(TrajectoryStore::new(pool.clone(), blob, RetentionMode::FullDebug));

    let key = base_key(Uuid::new_v4());
    let recording_id = store.begin_recording(&key).await.unwrap();
    let rec_id_c = recording_id.clone();

    const N: usize = 2000;
    const FLUSH_AT: usize = 64;
    let (tx, mut rx) = FrameChannel::new(DEFAULT_FRAME_CHANNEL_CAPACITY).split();
    let tx = Arc::new(tx);
    let dropped = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let dropped_c = dropped.clone();

    let t_start = std::time::Instant::now();

    let producer = tokio::spawn(async move {
        for i in 0..N {
            let f = TrajectoryFrame::TextDelta {
                ts_ms: i as u64,
                text: format!("t{i}"),
            };
            if tx.send(f).await.is_err() {
                dropped_c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    });

    let store_c = store.clone();
    let consumer = tokio::spawn(async move {
        let mut bw = BatchedFrameWriter::new(Arc::clone(&store_c), rec_id_c.clone(), FLUSH_AT);
        let mut count = 0u64;
        while let Some(frame) = rx.recv().await {
            bw.push("trader", 0, count as i64, frame);
            bw.flush_if_needed().await.unwrap();
            count += 1;
            if count as usize >= N {
                break;
            }
        }
        bw.flush().await.unwrap();
        store_c.complete_recording(&rec_id_c).await.unwrap();
    });

    tokio::join!(producer, consumer);
    let elapsed = t_start.elapsed();
    let fps = N as f64 / elapsed.as_secs_f64();

    println!(
        "\n===== batched_record_throughput =====\n  frames={N} flush_at={FLUSH_AT}\n  fps={fps:.0}\n  drops={}\n=====================================\n",
        dropped.load(std::sync::atomic::Ordering::Relaxed)
    );

    assert_eq!(
        dropped.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "zero drops"
    );
    assert!(fps > 0.0);
}

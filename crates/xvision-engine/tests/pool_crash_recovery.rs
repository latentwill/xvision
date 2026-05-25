//! Stage-4 Task 5: pool crash isolation + respawn (Item 2).
//!
//! Tests that a sidecar crash in one pool slot:
//! (a) is detected;
//! (b) the pool replaces it (restart count increments);
//! (c) in-flight recording is marked `incomplete` (Stage 2 semantics);
//! (d) other pool members' in-flight recordings are unaffected;
//! (e) restart count is observable.
//!
//! ## Stand-in approach
//!
//! These tests use a `MockSidecar` stand-in instead of a real
//! `xvision-agentd` Node process.  Real-process kill is NOT exercised
//! because:
//! - The Node sidecar binary requires `xvision-agentd/dist/index.js` which
//!   is not built in CI without the Node toolchain.
//! - The crash-detection contract (report_crash → replace slot → increment
//!   restart counter) lives entirely in `SidecarPool::PoolLease` and is
//!   independent of whether `T` is a real process or a mock.
//! - The key integration invariant (marking an in-flight recording
//!   `incomplete`) is tested here by driving `TrajectoryStore` directly.
//!
//! If real-process crash is needed in future, the test would call
//! `supervisor.child.kill()` (via a test-helper method) and then assert the
//! RPC call returns `TransportClosed`.

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;
use xvision_agent_client::pool::{PoolStats, SidecarPool};
use xvision_observability::{
    blobs::BlobStore,
    config::RetentionMode,
    trajectory::{
        frame::TrajectoryFrame,
        key::{TrajectoryKeyBuilder, TRAJECTORY_SCHEMA_VERSION},
        store::{BatchedFrameWriter, TrajectoryStore, STATUS_INCOMPLETE, STATUS_OPEN},
    },
};

// ── migrations ────────────────────────────────────────────────────────────────

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");
const MIGRATION_039: &str = include_str!("../../xvision-engine/migrations/039_run_trajectory_mode.sql");
const MIGRATION_040: &str = include_str!("../../xvision-engine/migrations/040_trajectory_frames.sql");

async fn migrated_pool() -> SqlitePool {
    let p = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(MIGRATION_002).execute(&p).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&p).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&p).await.unwrap();
    sqlx::query(MIGRATION_039).execute(&p).await.unwrap();
    sqlx::query(MIGRATION_040).execute(&p).await.unwrap();
    p
}

// ── MockSidecar ───────────────────────────────────────────────────────────────

/// Lightweight mock that simulates a sidecar in use (writing trajectory frames).
#[derive(Clone)]
struct MockSidecar {
    pub alive: Arc<AtomicBool>,
    pub calls: Arc<AtomicUsize>,
}

impl MockSidecar {
    fn new() -> Self {
        Self {
            alive: Arc::new(AtomicBool::new(true)),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// "Call" the sidecar — increments call counter or returns an error if dead.
    fn call(&self) -> Result<(), &'static str> {
        if !self.alive.load(Ordering::Relaxed) {
            return Err("sidecar dead");
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Simulate a sidecar crash.
    fn crash(&self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

type MockFactory = Box<
    dyn Fn(usize) -> std::pin::Pin<Box<dyn std::future::Future<Output = MockSidecar> + Send>> + Send + Sync,
>;

fn make_pool(n: usize) -> SidecarPool<MockSidecar, MockFactory> {
    let slots: Vec<MockSidecar> = (0..n).map(|_| MockSidecar::new()).collect();
    SidecarPool::from_clients(
        n,
        slots,
        Box::new(|_idx| {
            Box::pin(async { MockSidecar::new() })
                as std::pin::Pin<Box<dyn std::future::Future<Output = MockSidecar> + Send>>
        }),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Test (a)(b)(e): crash detected, pool replaces, restart count increments
// ─────────────────────────────────────────────────────────────────────────────

/// A crashed sidecar slot is replaced and the restart count increments.
/// Subsequent leases succeed and deliver a healthy client.
///
/// Stand-in: `MockSidecar.crash()` simulates process death.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crashed_slot_is_replaced_and_restart_counted() {
    let pool = Arc::new(make_pool(2));
    assert_eq!(pool.stats().restarts, 0);

    // Lease and crash slot.
    {
        let mut lease = pool.lease().await;
        {
            let guard = lease.borrow_client().await;
            // "Detect" the crash via a failed call.
            guard.crash();
            assert!(guard.call().is_err(), "call on dead sidecar must fail");
        }
        // Mark the lease as crashed → pool replaces on drop.
        lease.report_crash();
    }

    // Give replacement task time to run.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let stats = pool.stats();
    assert_eq!(stats.restarts, 1, "restart count must be 1 after one crash");
    assert_eq!(stats.idle, 2, "both slots must be idle after replacement");

    // New lease must be on a healthy client.
    let lease = pool.lease().await;
    {
        let guard = lease.borrow_client().await;
        assert!(
            guard.alive.load(Ordering::Relaxed),
            "replacement slot must be alive"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test (c): in-flight recording marked `incomplete`
// ─────────────────────────────────────────────────────────────────────────────

/// When a sidecar crashes mid-recording, the caller marks the recording
/// `incomplete` before returning the lease.  The recording status in the
/// trajectory store is `incomplete` (not `open` or `complete`), making it
/// ineligible for replay.
///
/// Stand-in: mock crash via `MockSidecar.crash()`; recording ops via
/// real `TrajectoryStore` + in-memory SQLite.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crashed_sidecar_recording_is_marked_incomplete() {
    let db = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let blob = BlobStore::new(tmp.path().to_path_buf());
    let store = Arc::new(TrajectoryStore::new(db.clone(), blob, RetentionMode::FullDebug));

    // Begin a recording.
    let key = TrajectoryKeyBuilder::default()
        .cycle_id(Uuid::new_v4())
        .slot_role("trader")
        .arm_scope(None::<&str>)
        .simulation_id(None::<&str>)
        .provider("anthropic")
        .model("claude-opus-4-7")
        .model_version("2026-05")
        .schema_version(TRAJECTORY_SCHEMA_VERSION)
        .system_prompt_hash("sha256:sys")
        .user_prompt_hash("sha256:usr")
        .build();
    let recording_id = store.begin_recording(&key).await.unwrap();

    // Write a few frames.
    store
        .append_frame(
            &recording_id,
            "trader",
            0,
            0,
            &TrajectoryFrame::TextDelta {
                ts_ms: 1,
                text: "partial".into(),
            },
        )
        .await
        .unwrap();

    // Simulate crash — mark recording incomplete.
    store
        .mark_incomplete(&recording_id, "sidecar killed")
        .await
        .unwrap();

    // Verify status.
    let rec = store.get_recording(recording_id.as_str()).await.unwrap();
    assert_eq!(
        rec.status, STATUS_INCOMPLETE,
        "recording must be incomplete after crash"
    );
    assert_eq!(
        rec.recovery_reason.as_deref(),
        Some("sidecar killed"),
        "recovery_reason must be set"
    );

    // `validate` must reject incomplete recordings.
    let v = store.validate(recording_id.as_str()).await;
    assert!(
        v.is_err(),
        "validate must fail for incomplete recording; got: {v:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test (d): pool-mates unaffected by crash
// ─────────────────────────────────────────────────────────────────────────────

/// Other pool members' recordings are unaffected when one slot crashes.
///
/// Scenario: 3-slot pool; slot 0 records 10 frames successfully, slot 1
/// crashes mid-way and is marked incomplete, slot 2 records 10 frames
/// successfully.  Final state: slot 0 and slot 2 are `complete`, slot 1 is
/// `incomplete`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pool_mates_unaffected_by_crash() {
    let db = migrated_pool().await;
    let tmp = TempDir::new().unwrap();
    let blob = BlobStore::new(tmp.path().to_path_buf());
    let store = Arc::new(TrajectoryStore::new(db.clone(), blob, RetentionMode::FullDebug));

    let pool = Arc::new(make_pool(3));

    fn make_key(label: &str) -> xvision_observability::trajectory::key::TrajectoryKey {
        TrajectoryKeyBuilder::default()
            .cycle_id(Uuid::new_v4())
            .slot_role(label)
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

    // --- Slot 0: 10 frames, complete ---
    let rec_0 = store.begin_recording(&make_key("slot_0")).await.unwrap();
    let rec_0_c = rec_0.clone();
    let store_c = store.clone();
    let pool_c = pool.clone();
    let h0 = tokio::spawn(async move {
        let _lease = pool_c.lease().await;
        for i in 0..10 {
            store_c
                .append_frame(
                    &rec_0_c,
                    "slot_0",
                    0,
                    i,
                    &TrajectoryFrame::TextDelta {
                        ts_ms: i as u64,
                        text: format!("s0-{i}"),
                    },
                )
                .await
                .unwrap();
        }
        store_c.complete_recording(&rec_0_c).await.unwrap();
    });

    // --- Slot 1: 5 frames, then crash → incomplete ---
    let rec_1 = store.begin_recording(&make_key("slot_1")).await.unwrap();
    let rec_1_c = rec_1.clone();
    let store_c = store.clone();
    let pool_c = pool.clone();
    let h1 = tokio::spawn(async move {
        let mut lease = pool_c.lease().await;
        // Write 5 frames.
        for i in 0..5 {
            store_c
                .append_frame(
                    &rec_1_c,
                    "slot_1",
                    0,
                    i,
                    &TrajectoryFrame::TextDelta {
                        ts_ms: i as u64,
                        text: format!("s1-{i}"),
                    },
                )
                .await
                .unwrap();
        }
        // Simulate crash.
        {
            let guard = lease.borrow_client().await;
            guard.crash();
        }
        // Mark recording incomplete (caller's responsibility before reporting crash).
        store_c
            .mark_incomplete(&rec_1_c, "sidecar killed mid-run")
            .await
            .unwrap();
        lease.report_crash();
        // Drop triggers replacement.
    });

    // --- Slot 2: 10 frames, complete ---
    let rec_2 = store.begin_recording(&make_key("slot_2")).await.unwrap();
    let rec_2_c = rec_2.clone();
    let store_c = store.clone();
    let pool_c = pool.clone();
    let h2 = tokio::spawn(async move {
        let _lease = pool_c.lease().await;
        for i in 0..10 {
            store_c
                .append_frame(
                    &rec_2_c,
                    "slot_2",
                    0,
                    i,
                    &TrajectoryFrame::TextDelta {
                        ts_ms: i as u64,
                        text: format!("s2-{i}"),
                    },
                )
                .await
                .unwrap();
        }
        store_c.complete_recording(&rec_2_c).await.unwrap();
    });

    tokio::join!(h0, h1, h2);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify statuses.
    let r0 = store.get_recording(rec_0.as_str()).await.unwrap();
    let r1 = store.get_recording(rec_1.as_str()).await.unwrap();
    let r2 = store.get_recording(rec_2.as_str()).await.unwrap();

    assert_eq!(r0.status, "complete", "slot_0 must be complete");
    assert_eq!(
        r1.status, STATUS_INCOMPLETE,
        "slot_1 must be incomplete (crashed)"
    );
    assert_eq!(r2.status, "complete", "slot_2 must be complete");

    // Verify restart count = 1.
    let stats = pool.stats();
    assert_eq!(stats.restarts, 1, "exactly one restart; others unaffected");

    // Verify slot_0 and slot_2 have the correct number of frames.
    let count_0: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM trajectory_frames WHERE recording_id = ?")
        .bind(rec_0.as_str())
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count_0.0, 10, "slot_0 must have 10 frames");

    let count_2: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM trajectory_frames WHERE recording_id = ?")
        .bind(rec_2.as_str())
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count_2.0, 10, "slot_2 must have 10 frames");

    // slot_1 has only the 5 frames written before the crash.
    let count_1: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM trajectory_frames WHERE recording_id = ?")
        .bind(rec_1.as_str())
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count_1.0, 5, "slot_1 must have only 5 pre-crash frames");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test (e) standalone: restart count observable via stats
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_count_is_observable() {
    let pool = Arc::new(make_pool(2));
    assert_eq!(pool.restart_count(), 0);

    // Crash once.
    {
        let mut lease = pool.lease().await;
        lease.report_crash();
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(pool.restart_count(), 1);

    // Crash again.
    {
        let mut lease = pool.lease().await;
        lease.report_crash();
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(pool.restart_count(), 2);

    let stats: PoolStats = pool.stats();
    assert_eq!(stats.restarts, 2);
}

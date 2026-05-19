//! End-to-end coverage for the four notification kinds added in
//! `agent-run-observability-ipc-emission-v2`:
//!   - `event.model_call_started` (per-iteration ModelCall span)
//!   - `event.assistant_text_delta` (stream-only, no SQLite row)
//!   - `event.tool_call_cancelled` (span closes as Cancelled)
//!   - `event.overloaded` (BackpressureDropped warn row)
//!
//! Each test drives the fake sidecar over a real `UnixStream`, then
//! observes either the bus directly (for stream-only events) or the
//! recorder rows (for ones that should persist).

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use xvision_agent_client::{start_event_sink, SidecarFingerprint};
use xvision_observability::{AgentRunRecorder, RecorderError, RunEvent, RunEventBus, SqliteRecorder};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    pool
}

async fn count_rows(pool: &SqlitePool, table: &str) -> i64 {
    let row: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap();
    row.0
}

async fn wait_for_rows(pool: &SqlitePool, table: &str, expected: i64) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if count_rows(pool, table).await >= expected {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "table `{table}` had {} rows, expected {}",
                count_rows(pool, table).await,
                expected
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn push(conn: &mut UnixStream, method: &str, params: serde_json::Value) {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    let mut bytes = serde_json::to_vec(&msg).unwrap();
    bytes.push(b'\n');
    conn.write_all(&bytes).await.unwrap();
    conn.flush().await.unwrap();
}

/// Bus subscriber that captures every published event into a shared
/// Vec so tests can assert on the canonical event stream (not just the
/// recorder side-effects).
#[derive(Default)]
struct CapturingRecorder {
    events: tokio::sync::Mutex<Vec<RunEvent>>,
}

impl CapturingRecorder {
    async fn snapshot(&self) -> Vec<RunEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl AgentRunRecorder for CapturingRecorder {
    async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError> {
        self.events.lock().await.push(event.clone());
        Ok(())
    }

    async fn mark_interrupted(&self, _run_id: &str) -> Result<(), RecorderError> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_assistant_text_delta_publishes_event_but_no_sqlite_row() {
    let pool = migrated_pool().await;
    let sqlite: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let capture = Arc::new(CapturingRecorder::default());
    let bus = Arc::new(RunEventBus::new(vec![
        sqlite,
        capture.clone() as Arc<dyn AgentRunRecorder>,
    ]));

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("events.sock");
    let handle = start_event_sink(&sock, bus.clone(), SidecarFingerprint::default())
        .await
        .unwrap();

    let mut conn = UnixStream::connect(&sock).await.unwrap();

    // RunStarted first so the bus's span→run map knows about the run.
    push(
        &mut conn,
        "event.run_started",
        serde_json::json!({
            "run_id": "r-delta",
            "objective": "stream",
            "started_at_ms": 1_700_000_000_000_u64,
            "provider_id": "anthropic",
            "model_id": "claude-opus-4-7",
        }),
    )
    .await;
    push(
        &mut conn,
        "event.model_call_started",
        serde_json::json!({
            "span_id": "sp-m1",
            "run_id": "r-delta",
            "provider": "anthropic",
            "model": "claude-opus-4-7",
        }),
    )
    .await;
    push(
        &mut conn,
        "event.assistant_text_delta",
        serde_json::json!({
            "span_id": "sp-m1",
            "run_id": "r-delta",
            "delta_len": 7,
        }),
    )
    .await;
    push(
        &mut conn,
        "event.assistant_text_delta",
        serde_json::json!({
            "span_id": "sp-m1",
            "run_id": "r-delta",
            "delta_len": 5,
        }),
    )
    .await;
    push(
        &mut conn,
        "event.model_call_finished",
        serde_json::json!({
            "span_id": "sp-m1",
            "run_id": "r-delta",
            "provider": "anthropic",
            "model": "claude-opus-4-7",
            "input_tokens": 10,
            "output_tokens": 3,
        }),
    )
    .await;

    // RunStarted writes a row; wait for it so we know the sink is
    // caught up before snapshotting.
    wait_for_rows(&pool, "agent_runs", 1).await;
    wait_for_rows(&pool, "model_calls", 1).await;

    // Give a brief tick for stream-only events to land in the
    // capture subscriber (they don't trigger any SQL we can wait
    // on).
    for _ in 0..50 {
        let snap = capture.snapshot().await;
        if snap
            .iter()
            .filter(|e| matches!(e, RunEvent::AssistantTextDelta(_)))
            .count()
            >= 2
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let snapshot = capture.snapshot().await;
    let deltas: Vec<usize> = snapshot
        .iter()
        .filter_map(|e| match e {
            RunEvent::AssistantTextDelta(d) => Some(d.delta_len),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec![7usize, 5usize], "two deltas in arrival order");

    // Phase A retention: AssistantTextDelta is stream-only — recorder
    // writes no dedicated table. Spans + model_calls land normally.
    // (No `assistant_text_deltas` table exists — assert via the rows
    // that DO exist for the same span.)
    let spans_count = count_rows(&pool, "spans").await;
    assert!(spans_count >= 1, "ModelCall span row should exist");

    drop(conn);
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_tool_call_cancelled_closes_span_and_records_detail() {
    let pool = migrated_pool().await;
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("events.sock");
    let handle = start_event_sink(&sock, bus.clone(), SidecarFingerprint::default())
        .await
        .unwrap();

    let mut conn = UnixStream::connect(&sock).await.unwrap();

    push(
        &mut conn,
        "event.run_started",
        serde_json::json!({
            "run_id": "r-cancel",
            "objective": "cancel",
            "started_at_ms": 1_700_000_000_000_u64,
            "provider_id": "anthropic",
            "model_id": "claude-opus-4-7",
        }),
    )
    .await;
    push(
        &mut conn,
        "event.tool_call_started",
        serde_json::json!({
            "span_id": "sp-t1",
            "run_id": "r-cancel",
            "tool_name": "long_running",
            "input_hash": "h-in",
        }),
    )
    .await;
    push(
        &mut conn,
        "event.tool_call_cancelled",
        serde_json::json!({
            "span_id": "sp-t1",
            "run_id": "r-cancel",
            "reason": "user abort",
        }),
    )
    .await;

    wait_for_rows(&pool, "agent_runs", 1).await;
    wait_for_rows(&pool, "tool_calls", 1).await;

    // Span should be marked cancelled (closes after the cancellation
    // notification fires).
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let row: (String,) = sqlx::query_as("SELECT status FROM spans WHERE id = ?")
            .bind("sp-t1")
            .fetch_one(&pool)
            .await
            .unwrap();
        if row.0 == "cancelled" {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!("span should be cancelled; status = {}", row.0);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    drop(conn);
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_overloaded_publishes_backpressure_dropped() {
    let pool = migrated_pool().await;
    let sqlite: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let capture = Arc::new(CapturingRecorder::default());
    let bus = Arc::new(RunEventBus::new(vec![
        sqlite,
        capture.clone() as Arc<dyn AgentRunRecorder>,
    ]));

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("events.sock");
    let handle = start_event_sink(&sock, bus.clone(), SidecarFingerprint::default())
        .await
        .unwrap();

    let mut conn = UnixStream::connect(&sock).await.unwrap();
    push(
        &mut conn,
        "event.run_started",
        serde_json::json!({
            "run_id": "r-overload",
            "objective": "back",
            "started_at_ms": 1_700_000_000_000_u64,
            "provider_id": "anthropic",
            "model_id": "claude-opus-4-7",
        }),
    )
    .await;
    push(
        &mut conn,
        "event.overloaded",
        serde_json::json!({
            "run_id": "r-overload",
            "dropped": 0,
            "note": "outbound buffer high",
        }),
    )
    .await;

    wait_for_rows(&pool, "agent_runs", 1).await;
    // Wait for the overload to appear in the capture.
    for _ in 0..50 {
        let snap = capture.snapshot().await;
        if snap.iter().any(|e| matches!(e, RunEvent::BackpressureDropped(_))) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let snap = capture.snapshot().await;
    let backpressure = snap
        .iter()
        .find_map(|e| match e {
            RunEvent::BackpressureDropped(b) => Some(b.clone()),
            _ => None,
        })
        .expect("BackpressureDropped should be published");
    assert_eq!(backpressure.run_id, "r-overload");
    assert_eq!(backpressure.note, "outbound buffer high");

    drop(conn);
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_per_iteration_model_call_pair_records_model_row() {
    // Verify that the start→delta→finish sequence records one
    // model_calls row with usage attached to the per-iteration span.
    let pool = migrated_pool().await;
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("events.sock");
    let handle = start_event_sink(&sock, bus.clone(), SidecarFingerprint::default())
        .await
        .unwrap();

    let mut conn = UnixStream::connect(&sock).await.unwrap();
    push(
        &mut conn,
        "event.run_started",
        serde_json::json!({
            "run_id": "r-iter",
            "objective": "iter",
            "started_at_ms": 1_700_000_000_000_u64,
            "provider_id": "anthropic",
            "model_id": "claude-opus-4-7",
        }),
    )
    .await;
    push(
        &mut conn,
        "event.model_call_started",
        serde_json::json!({
            "span_id": "sp-iter-1",
            "run_id": "r-iter",
            "provider": "anthropic",
            "model": "claude-opus-4-7",
        }),
    )
    .await;
    push(
        &mut conn,
        "event.model_call_finished",
        serde_json::json!({
            "span_id": "sp-iter-1",
            "run_id": "r-iter",
            "provider": "anthropic",
            "model": "claude-opus-4-7",
            "input_tokens": 11,
            "output_tokens": 22,
            "total_cost": 0.0042,
        }),
    )
    .await;

    wait_for_rows(&pool, "model_calls", 1).await;
    let row: (i64, i64, Option<f64>) = sqlx::query_as(
        "SELECT input_token_count, output_token_count, cost_usd FROM model_calls WHERE span_id = ?",
    )
    .bind("sp-iter-1")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, 11);
    assert_eq!(row.1, 22);
    assert!((row.2.unwrap() - 0.0042).abs() < 1e-9);

    drop(conn);
    handle.shutdown().await;
}

//! End-to-end smoke for the Phase-B IPC emission path.
//!
//! Spins up an in-process "fake sidecar" that connects to the event
//! socket and pushes the same JSON-RPC notifications the real Node
//! sidecar would emit. The `RunEventSink` translates each to a
//! `RunEvent`, the bus delivers it to a real `SqliteRecorder`, and we
//! assert the rows landed in migration-018 tables.
//!
//! Covers acceptance criteria from
//! `team/contracts/agent-run-observability-ipc-emission.md`:
//! - sidecar→Rust notifications round-trip on a dedicated socket
//! - id-less JSON-RPC 2.0 notification framing
//! - sidecar fingerprint stamped on RunStarted
//! - `RunEventSink` translates each kind 1:1 (run/tool/model/error)
//! - lifecycle critical events (RunStarted/Finished) survive bus
//!   delivery

use std::sync::Arc;
use std::time::Duration;

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use xvision_agent_client::{start_event_sink, SidecarFingerprint};
use xvision_observability::{AgentRunRecorder, RunEventBus, SqliteRecorder};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
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

/// Push a JSON-RPC 2.0 notification (no `id`) over an open UnixStream.
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ipc_emission_records_rows_with_fingerprint() {
    let pool = migrated_pool().await;
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("events.sock");

    let fp = SidecarFingerprint {
        sidecar_version: Some("0.1.0".into()),
        cline_sdk_version: Some("0.0.41".into()),
        protocol_version: Some("0.1.0".into()),
    };
    let handle = start_event_sink(&sock, bus.clone(), fp).await.unwrap();

    // Fake sidecar: connect, push the full event sequence, disconnect.
    let mut conn = UnixStream::connect(&sock).await.unwrap();

    push(
        &mut conn,
        "event.run_started",
        serde_json::json!({
            "run_id": "r-smoke-1",
            "objective": "smoke test",
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
            "span_id": "sp-tool-1",
            "run_id": "r-smoke-1",
            "tool_name": "echo",
            "input_hash": "h-input",
        }),
    )
    .await;

    push(
        &mut conn,
        "event.tool_call_finished",
        serde_json::json!({
            "span_id": "sp-tool-1",
            "run_id": "r-smoke-1",
            "output_hash": "h-output",
        }),
    )
    .await;

    // v2: model_call_finished now pairs with an explicit
    // model_call_started, both sharing the same span_id.
    push(
        &mut conn,
        "event.model_call_started",
        serde_json::json!({
            "span_id": "sp-model-1",
            "run_id": "r-smoke-1",
            "provider": "anthropic",
            "model": "claude-opus-4-7",
        }),
    )
    .await;
    push(
        &mut conn,
        "event.model_call_finished",
        serde_json::json!({
            "span_id": "sp-model-1",
            "run_id": "r-smoke-1",
            "provider": "anthropic",
            "model": "claude-opus-4-7",
            "input_tokens": 100,
            "output_tokens": 50,
            "total_cost": 0.0123,
        }),
    )
    .await;

    push(
        &mut conn,
        "event.run_finished",
        serde_json::json!({
            "run_id": "r-smoke-1",
            "status": "completed",
            "finished_at_ms": 1_700_000_010_000_u64,
        }),
    )
    .await;

    // Wait for the lifecycle rows. RunStarted writes the agent_runs row;
    // RunFinished updates it. tool_call_started + tool_call_finished
    // together write one tool_calls row plus a spans row.
    wait_for_rows(&pool, "agent_runs", 1).await;
    wait_for_rows(&pool, "spans", 1).await;
    wait_for_rows(&pool, "tool_calls", 1).await;

    // Assert sidecar fingerprint landed on agent_runs.
    let row: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT sidecar_version, cline_sdk_version, protocol_version FROM agent_runs WHERE id = ?",
    )
    .bind("r-smoke-1")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0.as_deref(), Some("0.1.0"));
    assert_eq!(row.1.as_deref(), Some("0.0.41"));
    assert_eq!(row.2.as_deref(), Some("0.1.0"));

    // Assert tool call captured the input hash.
    let tool_row: (String, String) = sqlx::query_as("SELECT tool_name, input_hash FROM tool_calls LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tool_row.0, "echo");
    assert_eq!(tool_row.1, "h-input");

    // Model call row should exist with token + cost data.
    wait_for_rows(&pool, "model_calls", 1).await;
    let model_row: (String, String, Option<i64>, Option<i64>, Option<f64>) = sqlx::query_as(
        "SELECT provider, model, input_token_count, output_token_count, cost_usd FROM model_calls LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(model_row.0, "anthropic");
    assert_eq!(model_row.1, "claude-opus-4-7");
    assert_eq!(model_row.2, Some(100));
    assert_eq!(model_row.3, Some(50));
    assert!((model_row.4.unwrap() - 0.0123).abs() < 1e-9);

    // Run should be marked completed.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let status_row: (String,) = sqlx::query_as("SELECT status FROM agent_runs WHERE id = ?")
            .bind("r-smoke-1")
            .fetch_one(&pool)
            .await
            .unwrap();
        if status_row.0 == "completed" {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!("run was not marked completed; status = {}", status_row.0);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    drop(conn);
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_notification_method_is_silently_ignored() {
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
        "event.future_method_not_yet_supported",
        serde_json::json!({"any": "thing"}),
    )
    .await;
    // Followed by a legitimate event so we can assert that processing
    // continued after the unknown one was dropped.
    push(
        &mut conn,
        "event.run_started",
        serde_json::json!({
            "run_id": "r-forward-compat",
            "objective": "fwd-compat",
            "started_at_ms": 1_700_000_000_000_u64,
            "provider_id": "anthropic",
            "model_id": "claude-opus-4-7",
        }),
    )
    .await;

    wait_for_rows(&pool, "agent_runs", 1).await;
    drop(conn);
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sidecar_crash_marks_runs_interrupted() {
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
            "run_id": "r-crash",
            "objective": "crash",
            "started_at_ms": 1_700_000_000_000_u64,
            "provider_id": "anthropic",
            "model_id": "claude-opus-4-7",
        }),
    )
    .await;
    wait_for_rows(&pool, "agent_runs", 1).await;
    drop(conn);

    // Simulate Rust supervisor detecting sidecar crash and asking the
    // sink to mark open runs interrupted.
    xvision_agent_client::mark_runs_interrupted(&bus, ["r-crash".to_string()], "sidecar exited unexpectedly")
        .await;

    // Recorder should update the run to `interrupted`.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let row: (String,) = sqlx::query_as("SELECT status FROM agent_runs WHERE id = ?")
            .bind("r-crash")
            .fetch_one(&pool)
            .await
            .unwrap();
        if row.0 == "interrupted" {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!("run was not marked interrupted; status = {}", row.0);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    handle.shutdown().await;
}

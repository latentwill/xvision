//! Round-trip regression for `trace-obs-ws5` Gap 1 — tool-payload
//! parity.
//!
//! Before this track, `emit_tool_call_started` / `emit_tool_call_finished`
//! hardcoded `input_payload_ref: None` / `output_payload_ref: None`, so
//! tool input/output was hash-only forever — no plaintext path, even
//! under `redacted` / `full_debug`. This closes that gap by giving tool
//! calls the SAME blob + side-row plaintext path model calls already
//! have.
//!
//! Two surfaces are exercised:
//!
//! 1. Emitter → event: `emit_tool_call_started_with_payload` /
//!    `emit_tool_call_finished_with_payload` populate
//!    `input_payload_ref` / `output_payload_ref` (and carry the
//!    plaintext `input_text` / `output_text`) under `full_debug` /
//!    `redacted`, and leave them `None` under `hash_only`. A secret in
//!    the tool I/O is scrubbed under `redacted`.
//!
//! 2. Recorder → export: driving the events through `SqliteRecorder`
//!    writes a `tool_call_payload` side-row, and `build_export`
//!    reconstructs `input_text` / `output_text` on the exported
//!    `ToolCallRow` — mirroring `model_call_payload`.

use std::sync::Arc;

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_engine::agent::observability::{ObsEmitter, ObsRetentionPolicy};
use xvision_observability::{
    build_export, AgentRunRecorder, BlobRef, BlobStore, NoopRecorder, ObservabilityConfig, RetentionMode,
    RunEvent, RunEventBus, RunStartedEvent, SpanKind, SpanStartedEvent, SqliteRecorder,
    ToolCallFinishedEvent, ToolCallStartedEvent,
};

const MIGRATION_002: &str = include_str!("../migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");

const TOOL_INPUT: &str = r#"{"symbol":"BTC","timeframe":"1h"}"#;
const TOOL_OUTPUT: &str = r#"{"rsi":61.2,"close":64000.0}"#;

fn policy_for(mode: RetentionMode) -> ObsRetentionPolicy {
    let mut cfg = ObservabilityConfig::default();
    cfg.retention.mode = mode;
    cfg.retention.store_prompts = true;
    cfg.retention.store_responses = true;
    cfg.retention.max_payload_bytes = 200_000;
    ObsRetentionPolicy::from_config(&cfg)
}

/// Drain helper mirroring `agent_observability_blob.rs`.
async fn collect_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    recorder.snapshot().await
}

fn find_started(events: &[RunEvent]) -> &ToolCallStartedEvent {
    events
        .iter()
        .find_map(|e| match e {
            RunEvent::ToolCallStarted(t) => Some(t),
            _ => None,
        })
        .expect("ToolCallStarted event published")
}

fn find_finished(events: &[RunEvent]) -> &ToolCallFinishedEvent {
    events
        .iter()
        .find_map(|e| match e {
            RunEvent::ToolCallFinished(t) => Some(t),
            _ => None,
        })
        .expect("ToolCallFinished event published")
}

/// `full_debug` → tool input/output blobs land in the `BlobStore`, refs
/// are populated, and the plaintext round-trips verbatim.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_debug_persists_tool_input_and_output_blobs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let emitter = ObsEmitter::new(bus.clone(), "run-tool-full-debug")
        .with_retention(policy_for(RetentionMode::FullDebug))
        .with_blob_store(store.clone());

    emitter
        .emit_tool_call_started_with_payload("span-t1", None, "get_indicator", TOOL_INPUT)
        .await;
    emitter
        .emit_tool_call_finished_with_payload("span-t1", TOOL_OUTPUT)
        .await;

    let events = collect_events(&bus, &recorder).await;
    let started = find_started(&events);
    let finished = find_finished(&events);

    let iref = started
        .input_payload_ref
        .as_ref()
        .expect("full_debug must populate input_payload_ref");
    let oref = finished
        .output_payload_ref
        .as_ref()
        .expect("full_debug must populate output_payload_ref");

    let in_bytes = store.read(&BlobRef(iref.clone())).expect("input blob");
    assert_eq!(std::str::from_utf8(&in_bytes).unwrap(), TOOL_INPUT);
    let out_bytes = store.read(&BlobRef(oref.clone())).expect("output blob");
    assert_eq!(std::str::from_utf8(&out_bytes).unwrap(), TOOL_OUTPUT);
}

/// `hash_only` → both refs `None`, nothing written to the blob store.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hash_only_leaves_tool_payload_refs_none_and_writes_no_blobs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let emitter = ObsEmitter::new(bus.clone(), "run-tool-hash-only")
        .with_retention(policy_for(RetentionMode::HashOnly))
        .with_blob_store(store.clone());

    emitter
        .emit_tool_call_started_with_payload("span-t2", None, "get_indicator", TOOL_INPUT)
        .await;
    emitter
        .emit_tool_call_finished_with_payload("span-t2", TOOL_OUTPUT)
        .await;

    let events = collect_events(&bus, &recorder).await;
    let started = find_started(&events);
    let finished = find_finished(&events);

    assert!(
        started.input_payload_ref.is_none(),
        "hash_only must leave input_payload_ref None, got {:?}",
        started.input_payload_ref
    );
    assert!(
        finished.output_payload_ref.is_none(),
        "hash_only must leave output_payload_ref None, got {:?}",
        finished.output_payload_ref
    );

    let entries: Vec<_> = std::fs::read_dir(tmp.path())
        .map(|d| d.collect::<Result<Vec<_>, _>>().unwrap_or_default())
        .unwrap_or_default();
    assert!(
        entries.is_empty(),
        "hash_only must not touch the blob store, found: {entries:?}"
    );
}

/// `redacted` → a secret in tool I/O is scrubbed before the blob is
/// written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn redacted_scrubs_secret_in_tool_io() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = BlobStore::new(tmp.path());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    let secret_input = "auth=sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-end";
    let secret_output = "leak: sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB-end";

    let emitter = ObsEmitter::new(bus.clone(), "run-tool-redacted")
        .with_retention(policy_for(RetentionMode::Redacted))
        .with_blob_store(store.clone());

    emitter
        .emit_tool_call_started_with_payload("span-t3", None, "broker_submit", secret_input)
        .await;
    emitter
        .emit_tool_call_finished_with_payload("span-t3", secret_output)
        .await;

    let events = collect_events(&bus, &recorder).await;
    let started = find_started(&events);
    let finished = find_finished(&events);

    let iref = started
        .input_payload_ref
        .as_ref()
        .expect("redacted must populate input_payload_ref");
    let oref = finished
        .output_payload_ref
        .as_ref()
        .expect("redacted must populate output_payload_ref");

    let in_bytes = store.read(&BlobRef(iref.clone())).expect("input blob");
    let in_str = std::str::from_utf8(&in_bytes).unwrap();
    assert!(
        !in_str.contains("sk-ant-api03-AAAAAAAA"),
        "redacted tool input blob must not contain raw secret, got: {in_str}"
    );
    assert!(
        in_str.contains("[redacted:anthropic_api_key]"),
        "redacted tool input blob must carry the redaction marker, got: {in_str}"
    );

    let out_bytes = store.read(&BlobRef(oref.clone())).expect("output blob");
    let out_str = std::str::from_utf8(&out_bytes).unwrap();
    assert!(
        !out_str.contains("sk-ant-api03-BBBBBBBB"),
        "redacted tool output blob must not contain raw secret, got: {out_str}"
    );
}

// --- Recorder → export side-row round-trip -------------------------------

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

/// `SqliteRecorder` writes a `tool_call_payload` side-row when the
/// events carry plaintext, and `build_export` reconstructs
/// `input_text` / `output_text` on the exported `ToolCallRow`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn export_reconstructs_tool_input_and_output_text_from_side_row() {
    let pool = migrated_pool().await;
    let run_id = "run_tool_payload_export";
    let span_id = "sp_tool_payload";

    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = RunEventBus::new(vec![sqlite.clone() as Arc<dyn AgentRunRecorder>]);

    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.into(),
        objective: "tool-payload-export".into(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: chrono::Utc::now(),
        retention_mode: "full_debug".into(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: span_id.into(),
        run_id: run_id.into(),
        parent_span_id: None,
        kind: SpanKind::ToolCall,
        name: "get_indicator".into(),
        started_at: chrono::Utc::now(),
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;

    bus.publish(RunEvent::ToolCallStarted(ToolCallStartedEvent {
        span_id: span_id.into(),
        tool_name: "get_indicator".into(),
        origin: xvision_observability::ToolOrigin::Native,
        tool_version: None,
        tool_hash: None,
        side_effect_level: xvision_observability::SideEffectLevel::ReadOnly,
        risk_level: xvision_observability::RiskLevel::SafeRead,
        requires_approval: false,
        is_run_terminator: false,
        input_hash: "sha256:in".into(),
        input_payload_ref: Some("blob:in".into()),
        input_text: Some(TOOL_INPUT.into()),
    }))
    .await;

    bus.publish(RunEvent::ToolCallFinished(ToolCallFinishedEvent {
        span_id: span_id.into(),
        output_hash: Some("sha256:out".into()),
        output_payload_ref: Some("blob:out".into()),
        exit_code: Some(0),
        output_text: Some(TOOL_OUTPUT.into()),
    }))
    .await;

    // Drain the bus consumer task. Input and output arrive on separate
    // events, so two `tool_call_payload` side-rows are written (one
    // carrying `input`, one carrying `output`); export coalesces them.
    drop(bus);
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE kind = 'tool_call_payload'")
            .fetch_one(&pool)
            .await
            .unwrap();
        if count >= 2 {
            break;
        }
    }

    let side_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE kind = 'tool_call_payload'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        side_rows, 2,
        "one tool_call_payload side-row per direction (input + output)"
    );

    let bundle = build_export(&pool, run_id).await.expect("build_export");
    let tc = bundle
        .tool_calls
        .iter()
        .find(|t| t.span_id == span_id)
        .expect("tool call row in export");
    assert_eq!(
        tc.input_text.as_deref(),
        Some(TOOL_INPUT),
        "export must reconstruct tool input_text from the side-row"
    );
    assert_eq!(
        tc.output_text.as_deref(),
        Some(TOOL_OUTPUT),
        "export must reconstruct tool output_text from the side-row"
    );
}

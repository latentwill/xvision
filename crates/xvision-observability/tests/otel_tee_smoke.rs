//! Smoke test for `OtelTeeRecorder` — wires
//! `OtelTeeRecorder + SqliteRecorder` to a shared `RunEventBus` and
//! drives a synthetic event stream through it. Asserts:
//!
//! 1. The SQLite ledger receives every row the canonical recorder would
//!    have written on its own — i.e. the tee is transparent.
//! 2. The in-memory OTel exporter captured a parallel span tree, one
//!    `tracing::span!()` per recorder call.
//! 3. **No payload-string attribute** ever appears on any exported
//!    OTel span. This is the plan's hard rule: prompts / responses /
//!    tool inputs / tool outputs never leave via OTel.
//!
//! Compiled only with `--features otel` (the file requires the symbols
//! published from `src/otel.rs`).

#![cfg(feature = "otel")]

use chrono::{Duration, Utc};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::Value;
use opentelemetry_sdk::testing::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::TracerProvider;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tracing_subscriber::prelude::*;
use xvision_observability::types::{RiskLevel, RunStatus, SideEffectLevel, SpanKind, SpanStatus, ToolOrigin};
use xvision_observability::{
    events::{
        ModelCallFinishedEvent, RunFinishedEvent, RunStartedEvent, SpanFinishedEvent, SpanStartedEvent,
        ToolCallFinishedEvent, ToolCallStartedEvent,
    },
    AgentRunRecorder, OtelTeeRecorder, RunEvent, RunEventBus, SqliteRecorder,
};

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

async fn wait_for_rows(pool: &SqlitePool, table: &str, expected: i64) {
    let deadline = std::time::Instant::now() + StdDuration::from_secs(2);
    loop {
        let row: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .unwrap();
        if row.0 >= expected || std::time::Instant::now() >= deadline {
            assert_eq!(
                row.0, expected,
                "table `{table}` had {} rows, expected {}",
                row.0, expected
            );
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
}

/// Synthetic payload strings the producer might carelessly try to leak.
/// The lint asserts none of these substrings appear in any OTel
/// attribute value or span name. (Hashes carrying these substrings
/// would fail the assertion too, but we deliberately keep the synthetic
/// hashes hex-only so a false positive can't mask a real leak.)
const PAYLOAD_NEEDLES: &[&str] = &[
    "PROMPT_SECRET_LEAK",
    "RESPONSE_SECRET_LEAK",
    "TOOL_INPUT_LEAK",
    "TOOL_OUTPUT_LEAK",
    "OBJECTIVE_SECRET_LEAK",
];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn otel_tee_records_sqlite_and_emits_parallel_span_tree() {
    // ── OTel pipeline with an in-memory exporter ────────────────────
    let exporter = InMemorySpanExporter::default();
    let provider = TracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let tracer = provider.tracer("xvision-observability-otel-tee-smoke");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let subscriber = tracing_subscriber::registry().with(otel_layer);
    // The bus consumer runs on a separate Tokio task, so a per-thread
    // `set_default` would not propagate. Use the global default so any
    // task in this test process picks up the OTel layer. This is the
    // only test in the crate that installs a global subscriber.
    tracing::subscriber::set_global_default(subscriber).expect("global subscriber already set");

    // ── Bus + tee on top of SqliteRecorder ──────────────────────────
    let pool = migrated_pool().await;
    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let tee: Arc<dyn AgentRunRecorder> = Arc::new(OtelTeeRecorder::new(sqlite.clone()));
    let bus = RunEventBus::new(vec![tee]);

    let run_id = "run_otel_smoke_01".to_string();
    let started_at = Utc::now();

    // Carries a deliberately-named "secret" objective. The tee MUST
    // NOT mirror this string into any OTel attribute.
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.clone(),
        objective: "OBJECTIVE_SECRET_LEAK do not export me".to_string(),
        strategy_id: Some("strat_otel".to_string()),
        eval_run_id: None,
        source_cli_job_id: None,
        started_at,
        retention_mode: "hash_only".to_string(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    let run_span_id = "span_otel_run".to_string();
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: run_span_id.clone(),
        run_id: run_id.clone(),
        parent_span_id: None,
        kind: SpanKind::AgentRun,
        name: "agent.run".to_string(),
        started_at,
        otel_trace_id: None,
        otel_span_id: None,
        // attributes_json could contain user text; the tee must NOT
        // mirror it. Stuff a needle into the JSON.
        attributes_json: Some(r#"{"note":"OBJECTIVE_SECRET_LEAK"}"#.to_string()),
    }))
    .await;

    // One model.call span — the prompt + response bodies the producer
    // would have are NOT carried on the event (only hashes), but we
    // also exercise the model-call path that DOES have a few payload
    // strings (provider, model name) and assert none of the needles
    // leak.
    let model_span = "span_otel_model".to_string();
    let ts = started_at + Duration::milliseconds(10);
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: model_span.clone(),
        run_id: run_id.clone(),
        parent_span_id: Some(run_span_id.clone()),
        kind: SpanKind::ModelCall,
        name: "model.call".to_string(),
        started_at: ts,
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;
    bus.publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
        span_id: model_span.clone(),
        provider: "anthropic".to_string(),
        model: "claude-opus-4-7".to_string(),
        input_token_count: Some(1024),
        output_token_count: Some(256),
        cost_usd: Some(0.02),
        prompt_hash: "sha256:deadbeef".to_string(),
        response_hash: Some("sha256:cafef00d".to_string()),
        prompt_text: None,
        response_text: None,
        prompt_payload_ref: None,
        response_payload_ref: None,
        // `tool_calls_requested` could be unbounded JSON. Stash a
        // needle to verify the tee does NOT mirror it.
        tool_calls_requested: Some(r#"[{"name":"x","args":"PROMPT_SECRET_LEAK"}]"#.to_string()),
        capability_path: None,
    }))
    .await;
    bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
        span_id: model_span,
        ended_at: ts + Duration::milliseconds(5),
        status: SpanStatus::Ok,
        error_json: None,
    }))
    .await;

    // One tool.call — exercise input_hash + output_hash. Needles go
    // on the `error_json` of a failed tool to verify even failure
    // paths don't mirror payloads.
    let tool_span = "span_otel_tool".to_string();
    let tts = started_at + Duration::milliseconds(40);
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: tool_span.clone(),
        run_id: run_id.clone(),
        parent_span_id: Some(run_span_id.clone()),
        kind: SpanKind::ToolCall,
        name: "tool.call".to_string(),
        started_at: tts,
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;
    bus.publish(RunEvent::ToolCallStarted(ToolCallStartedEvent {
        span_id: tool_span.clone(),
        tool_name: "search_files".to_string(),
        origin: ToolOrigin::Native,
        tool_version: Some("0.1".to_string()),
        tool_hash: Some("sha256:abcdef".to_string()),
        side_effect_level: SideEffectLevel::ReadOnly,
        risk_level: RiskLevel::SafeRead,
        requires_approval: false,
        is_run_terminator: false,
        input_hash: "sha256:beadfeed".to_string(),
        input_payload_ref: None,
        input_text: None,
    }))
    .await;
    bus.publish(RunEvent::ToolCallFinished(ToolCallFinishedEvent {
        span_id: tool_span.clone(),
        output_hash: Some("sha256:facefeed".to_string()),
        output_payload_ref: None,
        exit_code: Some(0),
        output_text: None,
    }))
    .await;
    bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
        span_id: tool_span,
        ended_at: tts + Duration::milliseconds(3),
        status: SpanStatus::Error,
        error_json: Some(r#"{"input":"TOOL_INPUT_LEAK","output":"TOOL_OUTPUT_LEAK"}"#.to_string()),
    }))
    .await;

    bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
        span_id: run_span_id,
        ended_at: started_at + Duration::milliseconds(100),
        status: SpanStatus::Ok,
        error_json: None,
    }))
    .await;
    bus.publish(RunEvent::RunFinished(RunFinishedEvent {
        run_id: run_id.clone(),
        finished_at: started_at + Duration::milliseconds(110),
        status: RunStatus::Completed,
        final_artifact_id: None,
        error: None,
    }))
    .await;

    // ── (1) SQLite side: every canonical row landed ─────────────────
    wait_for_rows(&pool, "model_calls", 1).await;
    wait_for_rows(&pool, "tool_calls", 1).await;
    wait_for_rows(&pool, "spans", 3).await;

    let (status,): (String,) = sqlx::query_as("SELECT status FROM agent_runs WHERE id = ?")
        .bind(&run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, RunStatus::Completed.as_db_str());

    // ── (2) OTel side: parallel span tree captured ──────────────────
    // Drop the SimpleSpanProcessor so it flushes synchronously. The
    // `OtelTeeRecorder` emits a span PER recorder call, so we expect at
    // least one exported span per event published above (12 events).
    drop(bus); // close bus, let consumer drain
    tokio::time::sleep(StdDuration::from_millis(50)).await;
    // SimpleSpanProcessor exports on span end synchronously, but
    // closing the provider guarantees a flush.
    let _ = provider.force_flush();

    let spans = exporter.get_finished_spans().expect("get spans");
    // 11 recorder calls were published above: 1 RunStarted, 3 SpanStarted,
    // 1 ModelCallFinished, 1 ToolCallStarted, 1 ToolCallFinished,
    // 3 SpanFinished, 1 RunFinished. The tee emits exactly one
    // tracing::span!() per call.
    assert_eq!(
        spans.len(),
        11,
        "expected exactly 11 OTel spans, got {}: {:?}",
        spans.len(),
        spans.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    // The span names we expect to see at minimum, mirroring each
    // recorder call.
    let names: Vec<String> = spans.iter().map(|s| s.name.to_string()).collect();
    for required in [
        "xvision.run.started",
        "xvision.span.started",
        "xvision.model.call",
        "xvision.tool.call.started",
        "xvision.tool.call.finished",
        "xvision.span.finished",
        "xvision.run.finished",
    ] {
        assert!(
            names.iter().any(|n| n == required),
            "missing OTel span `{}` in {:?}",
            required,
            names
        );
    }

    // ── (3) Hard rule: NO payload strings on any exported span ──────
    for span in &spans {
        for needle in PAYLOAD_NEEDLES {
            assert!(
                !span.name.contains(needle),
                "span name `{}` leaked payload `{}`",
                span.name,
                needle
            );
            for kv in &span.attributes {
                let val = kv_to_string(&kv.value);
                assert!(
                    !val.contains(needle),
                    "OTel attribute `{}` = `{}` on span `{}` leaked payload `{}`",
                    kv.key,
                    val,
                    span.name,
                    needle
                );
                let key = kv.key.as_str().to_string();
                assert!(
                    !key.contains(needle),
                    "OTel attribute key `{}` leaked payload `{}`",
                    key,
                    needle
                );
            }
        }
    }

    // Anti-overreach sanity: at least one span carries the
    // `xvision.run.id` attribute (otherwise we'd be asserting nothing).
    let has_run_id_attr = spans
        .iter()
        .any(|s| s.attributes.iter().any(|kv| kv.key.as_str() == "xvision.run.id"));
    assert!(
        has_run_id_attr,
        "no OTel span carries `xvision.run.id` — recorder did not attach any attributes"
    );
}

fn kv_to_string(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::I64(i) => i.to_string(),
        Value::F64(f) => f.to_string(),
        Value::String(s) => s.to_string(),
        Value::Array(_) => format!("{:?}", v),
    }
}

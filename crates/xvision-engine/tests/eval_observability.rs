//! End-to-end regression for `qa-eval-observability-wiring`.
//!
//! Before this track: an eval LLM call that failed inside
//! `dispatch.complete()` propagated `Err(_)` up to the eval executor
//! and the run was marked failed — but no span / model_call event
//! landed on the observability bus. `SpanInspector` (PR #238) renders
//! `error_message` on errored spans, but there were no spans to
//! render, so eval failures were invisible in the trace dock.
//!
//! This test exercises the wiring directly: `execute_slot` with a
//! failing `LlmDispatch` + a real `RunEventBus` + `SqliteRecorder`,
//! then asserts the `spans` table holds a row with `status='error'`
//! and an `error_json` containing the dispatch message. That's the
//! exact data the trace dock consumes from `/api/agent-runs/<id>`.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{LlmDispatch, LlmRequest, LlmResponse};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;
use xvision_observability::{
    AgentRunRecorder, RunEvent, RunEventBus, RunStartedEvent, RunStatus, SqliteRecorder,
};

const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");

/// Failing dispatch: always returns the operator-flagged error
/// shape so the test exercises the same code path that produced
/// `[unclassified] error decoding response body: EOF while parsing
/// a value at line 1145 column 0` in production.
struct FailingDispatch;

#[async_trait]
impl LlmDispatch for FailingDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        Err(anyhow::anyhow!(
            "[unclassified] error decoding response body: EOF while parsing a value at line 1145 column 0"
        ))
    }
}

fn slot() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        prompt: "decide".into(),
        model_requirement: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    }
}

async fn setup_pool() -> SqlitePool {
    // Temp-file SQLite. `:memory:` would give each pool connection
    // its own private database — migration would land on one
    // connection, recorder writes on another, test queries on a
    // third, and the test would see an empty DB. A real on-disk
    // file is shared across the pool.
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    // Leak the TempDir so the file survives until process exit;
    // a per-process tempdir is fine for a unit test.
    Box::leak(Box::new(tmp));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    // Disable FK enforcement: migration 018 references `cli_jobs`
    // and `eval_runs`, which live in migrations 013 / 002 we don't
    // apply here. The test only exercises the recorder's INSERT
    // semantics, not cross-table integrity.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    pool
}

/// QA finding: a failing LLM dispatch from an eval run produces a
/// span row with `status='error'` and an `error_json` carrying the
/// dispatch's error message. `SpanInspector.parseErrorJson`
/// (PR #238) extracts the message from the `{message:...}` envelope.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failing_dispatch_emits_error_span_with_message() {
    let pool = setup_pool().await;
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));

    let run_id = "eval-run-test-1";
    let emitter = ObsEmitter::new(bus.clone(), run_id);

    // Register the run so SpanStarted has a valid FK.
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.to_string(),
        objective: "eval:Backtest:test-scenario".to_string(),
        strategy_id: None,
        eval_run_id: Some(run_id.to_string()),
        source_cli_job_id: None,
        started_at: chrono::Utc::now(),
        retention_mode: "hash_only".to_string(),
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    // Drive the failing dispatch through execute_slot.
    let slot = slot();
    let dispatch = Arc::new(FailingDispatch);
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let result = execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: None,
        obs: Some(emitter.clone()),
    })
    .await;
    assert!(result.is_err(), "failing dispatch must propagate Err");

    // Finish the run so the recorder closes it out (mirrors what
    // `api/eval.rs::run_inner` does on the error branch).
    emitter
        .emit_run_finished(RunStatus::Failed, Some("dispatch failed".to_string()))
        .await;

    // Drain the bus and yield enough times for the consumer task to
    // process every queued event. `quiesce()` is a single
    // `yield_now`; multi-event tests need a longer settle.
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    // Inspect the spans table. Exactly one ModelCall span should be
    // present, with status='error' and error_json containing the
    // dispatch's message text.
    let rows: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, status, error_json FROM spans WHERE run_id = ?")
            .bind(run_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "exactly one span recorded, got: {rows:?}");
    let (_span_id, status, error_json) = &rows[0];
    assert_eq!(status, "error", "span status must be 'error'");
    let error_text = error_json.as_deref().expect("error_json populated");
    assert!(
        error_text.contains("EOF while parsing"),
        "error_json must carry the dispatch error message, got: {error_text}"
    );
    assert!(
        error_text.contains("[unclassified]"),
        "error_json must preserve the classifier prefix, got: {error_text}"
    );

    // And the run row reflects 'failed'.
    let run_status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = ?")
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(run_status, "failed");
}

/// Companion: when `obs` is `None`, `execute_slot` runs the same code
/// path without touching the bus. This pins the no-op invariant —
/// every existing caller (CLI, unit tests, legacy pipeline) inherits
/// it without recompiling for the new field.
#[tokio::test]
async fn execute_slot_with_no_emitter_does_not_touch_bus() {
    let pool = setup_pool().await;
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let _bus = Arc::new(RunEventBus::new(vec![recorder]));

    let slot = slot();
    let dispatch = Arc::new(FailingDispatch);
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let result = execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: None,
        obs: None,
    })
    .await;
    assert!(result.is_err());

    // No spans should exist — emission was opted out.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spans")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "no spans emitted when obs is None");
}

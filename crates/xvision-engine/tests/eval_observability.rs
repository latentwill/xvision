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

use std::{sync::Arc, time::Duration};

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

const MIGRATION_002: &str = include_str!("../migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");

struct TestPool {
    pool: SqlitePool,
    _tmp: tempfile::TempDir,
}

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
        attested_with: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: Vec::new(),
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    }
}

async fn setup_pool() -> TestPool {
    // Temp-file SQLite. `:memory:` would give each pool connection
    // its own private database — migration would land on one
    // connection, recorder writes on another, test queries on a
    // third, and the test would see an empty DB. A real on-disk
    // file is shared across the pool.
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    TestPool { pool, _tmp: tmp }
}

async fn seed_eval_run(pool: &SqlitePool, run_id: &str) {
    sqlx::query(
        "INSERT INTO eval_runs (id, strategy_bundle_hash, scenario_id, mode, status, started_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind("strategy-test-bundle")
    .bind("test-scenario")
    .bind("backtest")
    .bind("running")
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .unwrap();
}

async fn wait_for_persisted_failure(
    pool: &SqlitePool,
    bus: &RunEventBus,
    run_id: &str,
) -> (Vec<(String, String, Option<String>)>, String) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    loop {
        bus.quiesce().await;

        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, status, error_json FROM spans WHERE run_id = ? AND kind = 'model.call'",
        )
        .bind(run_id)
        .fetch_all(pool)
        .await
        .unwrap();
        let run_status: Option<String> = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(pool)
            .await
            .unwrap();

        if rows.len() == 1
            && rows[0].1 == "error"
            && rows[0]
                .2
                .as_deref()
                .is_some_and(|error| error.contains("EOF while parsing") && error.contains("[unclassified]"))
            && run_status.as_deref() == Some("failed")
        {
            return (rows, run_status.unwrap());
        }

        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for persisted failed run; rows: {rows:?}, run_status: {run_status:?}");
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// QA finding: a failing LLM dispatch from an eval run produces a
/// span row with `status='error'` and an `error_json` carrying the
/// dispatch's error message. `SpanInspector.parseErrorJson`
/// (PR #238) extracts the message from the `{message:...}` envelope.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failing_dispatch_emits_error_span_with_message() {
    let pool = setup_pool().await;
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));

    let run_id = "eval-run-test-1";
    let emitter = ObsEmitter::new(bus.clone(), run_id);
    seed_eval_run(&pool.pool, run_id).await;

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
        system_prompt: String::new(),
        upstream_inputs: serde_json::json!({}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: Some(emitter.clone()),
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await;
    assert!(result.is_err(), "failing dispatch must propagate Err");

    // Finish the run so the recorder closes it out (mirrors what
    // `api/eval.rs::run_inner` does on the error branch).
    emitter
        .emit_run_finished(RunStatus::Failed, Some("dispatch failed".to_string()))
        .await;

    // Inspect the spans table. Exactly one ModelCall span should be
    // present, with status='error' and error_json containing the
    // dispatch's message text.
    let (rows, run_status) = wait_for_persisted_failure(&pool.pool, &bus, run_id).await;
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
    assert_eq!(run_status, "failed");
}

/// Companion: when `obs` is `None`, `execute_slot` runs the same code
/// path without touching the bus. This pins the no-op invariant —
/// every existing caller (CLI, unit tests, legacy pipeline) inherits
/// it without recompiling for the new field.
#[tokio::test]
async fn execute_slot_with_no_emitter_does_not_touch_bus() {
    let pool = setup_pool().await;

    let slot = slot();
    let dispatch = Arc::new(FailingDispatch);
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let result = execute_slot(SlotInput {
        slot: &slot,
        system_prompt: String::new(),
        upstream_inputs: serde_json::json!({}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: None,
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await;
    assert!(result.is_err());

    // No spans should exist — emission was opted out.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spans")
        .fetch_one(&pool.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "no spans emitted when obs is None");
}

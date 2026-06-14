//! Synthetic-event integration test — the contract for downstream Phase B
//! emission leaves. If their handlers produce the events we publish here,
//! the recorder will write the rows we assert below.
//!
//! Scenario (matches the `agent-run-observability-event-bus` contract):
//!   1 run, 3 model calls, 5 tool calls, 1 interrupted span, then a
//!   `RunInterrupted` close. The recorder must:
//!     - insert exactly one `agent_runs` row, transitioning to `interrupted`
//!     - insert one `spans` row per span (run-level + 3 + 5 + 1 = 10)
//!     - insert exactly 3 `model_calls` rows + 5 `tool_calls` rows
//!     - mark every still-open span as `interrupted`
//!     - preserve FIFO order per run_id (assertion: model_call N happens
//!       before tool_call N in the timeline order of `spans.started_at`)

use chrono::{Duration, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use xvision_observability::types::{RiskLevel, RunStatus, SideEffectLevel, SpanKind, SpanStatus, ToolOrigin};
use xvision_observability::{
    events::{
        ModelCallFinishedEvent, RunFinishedEvent, RunInterruptedEvent, RunStartedEvent, SpanFinishedEvent,
        SpanStartedEvent, ToolCallFinishedEvent, ToolCallStartedEvent,
    },
    AgentRunRecorder, RunEvent, RunEventBus, SqliteRecorder,
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

/// Wait for the recorder to write `expected` rows into the given table,
/// polling with a short backoff. Avoids racing the consumer task.
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

async fn wait_for_run_status(
    pool: &SqlitePool,
    run_id: &str,
    expected: RunStatus,
) -> (Option<String>, Option<String>) {
    let deadline = std::time::Instant::now() + StdDuration::from_secs(2);
    loop {
        let row: Option<(String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT status, finished_at, error FROM agent_runs WHERE id = ?")
                .bind(run_id)
                .fetch_optional(pool)
                .await
                .unwrap();
        if let Some((status, finished_at, error)) = row {
            if status == expected.as_db_str() {
                return (finished_at, error);
            }
        }
        if std::time::Instant::now() >= deadline {
            panic!("agent_runs row did not reach `{}` in time", expected.as_db_str());
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn synthetic_run_records_every_row_then_marks_interrupted() {
    let pool = migrated_pool().await;
    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = RunEventBus::new(vec![sqlite.clone() as Arc<dyn AgentRunRecorder>]);

    let run_id = "run_synth_01".to_string();
    let started_at = Utc::now();

    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.clone(),
        objective: "synthetic scenario".to_string(),
        strategy_id: Some("strat_test".to_string()),
        eval_run_id: None,
        source_cli_job_id: None,
        started_at,
        retention_mode: "hash_only".to_string(),
        trajectory_mode: None,
        sidecar_version: Some("sidecar-test-0".to_string()),
        cline_sdk_version: Some("cline-test-0".to_string()),
        protocol_version: Some("xvision/1".to_string()),
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    // Run-level span — sits above the per-call spans.
    let run_span_id = "span_run_01".to_string();
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: run_span_id.clone(),
        run_id: run_id.clone(),
        parent_span_id: None,
        kind: SpanKind::AgentRun,
        name: "agent.run".to_string(),
        started_at,
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;

    // 3 decision.model spans + their ModelCallFinished rows.
    for i in 0..3 {
        let span_id = format!("span_model_{i}");
        let ts = started_at + Duration::seconds(i as i64 + 1);
        bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
            span_id: span_id.clone(),
            run_id: run_id.clone(),
            parent_span_id: Some(run_span_id.clone()),
            kind: SpanKind::DecisionModel,
            name: format!("decision.model.{i}"),
            started_at: ts,
            otel_trace_id: None,
            otel_span_id: None,
            attributes_json: None,
        }))
        .await;
        bus.publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
            span_id: span_id.clone(),
            provider: "anthropic".to_string(),
            model: "claude-opus-4-7".to_string(),
            input_token_count: Some(1000 + i),
            output_token_count: Some(200 + i),
            cost_usd: Some(0.01 * (i as f64 + 1.0)),
            prompt_hash: format!("sha256:prompt_{i:064x}"),
            response_hash: Some(format!("sha256:resp_{i:064x}")),
            prompt_text: None,
            response_text: None,
            prompt_payload_ref: None,
            response_payload_ref: None,
            tool_calls_requested: None,
            capability_path: None,
        }))
        .await;
        bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
            span_id,
            ended_at: ts + Duration::milliseconds(5),
            status: SpanStatus::Ok,
            error_json: None,
        }))
        .await;
    }

    // 5 tool.call spans + their start/finish rows.
    for i in 0..5 {
        let span_id = format!("span_tool_{i}");
        let ts = started_at + Duration::seconds(10 + i as i64);
        bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
            span_id: span_id.clone(),
            run_id: run_id.clone(),
            parent_span_id: Some(run_span_id.clone()),
            kind: SpanKind::ToolCall,
            name: format!("tool.call.{i}"),
            started_at: ts,
            otel_trace_id: None,
            otel_span_id: None,
            attributes_json: None,
        }))
        .await;
        bus.publish(RunEvent::ToolCallStarted(ToolCallStartedEvent {
            span_id: span_id.clone(),
            tool_name: format!("tool_{i}"),
            origin: ToolOrigin::Native,
            tool_version: Some("0.1".to_string()),
            tool_hash: Some(format!("sha256:tool_{i:064x}")),
            side_effect_level: SideEffectLevel::ReadOnly,
            risk_level: RiskLevel::SafeRead,
            requires_approval: false,
            is_run_terminator: false,
            input_hash: format!("sha256:in_{i:064x}"),
            input_payload_ref: None,
            input_text: None,
        }))
        .await;
        bus.publish(RunEvent::ToolCallFinished(ToolCallFinishedEvent {
            span_id: span_id.clone(),
            output_hash: Some(format!("sha256:out_{i:064x}")),
            output_payload_ref: None,
            exit_code: Some(0),
            output_text: None,
        }))
        .await;
        bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
            span_id,
            ended_at: ts + Duration::milliseconds(3),
            status: SpanStatus::Ok,
            error_json: None,
        }))
        .await;
    }

    // One span that the sidecar never closes — must end up as
    // `interrupted` after RunInterrupted fires.
    let dangling_span = "span_dangling".to_string();
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: dangling_span.clone(),
        run_id: run_id.clone(),
        parent_span_id: Some(run_span_id.clone()),
        kind: SpanKind::ToolCall,
        name: "tool.call.never_finished".to_string(),
        started_at: started_at + Duration::seconds(20),
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;

    // Sidecar crash.
    bus.publish(RunEvent::RunInterrupted(RunInterruptedEvent {
        run_id: run_id.clone(),
        finished_at: started_at + Duration::seconds(30),
        reason: "sidecar crashed mid-run".to_string(),
    }))
    .await;

    // Acceptance: 3 model_calls + 5 tool_calls rows.
    wait_for_rows(&pool, "model_calls", 3).await;
    wait_for_rows(&pool, "tool_calls", 5).await;

    // 10 spans total (1 run-level + 3 model + 5 tool + 1 dangling).
    wait_for_rows(&pool, "spans", 10).await;

    let timeline: Vec<String> =
        sqlx::query_as("SELECT name FROM spans WHERE run_id = ? AND kind IN ('decision.model', 'tool.call') ORDER BY started_at, id")
            .bind(&run_id)
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|(name,): (String,)| name)
            .collect();
    assert_eq!(
        timeline,
        vec![
            "decision.model.0",
            "decision.model.1",
            "decision.model.2",
            "tool.call.0",
            "tool.call.1",
            "tool.call.2",
            "tool.call.3",
            "tool.call.4",
            "tool.call.never_finished",
        ],
        "model/tool span timeline must preserve FIFO order by started_at"
    );

    // The agent_runs row exists and is now `interrupted`.
    let (finished_at, error) = wait_for_run_status(&pool, &run_id, RunStatus::Interrupted).await;
    assert!(finished_at.is_some());
    assert_eq!(error.as_deref(), Some("sidecar crashed mid-run"));

    // Spans that were finished stay `ok`; the dangling span + the
    // run-level span are now `interrupted`.
    let (ok_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM spans WHERE run_id = ? AND status = 'ok'")
        .bind(&run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ok_count, 8, "expected 8 ok spans (3 model + 5 tool)");

    let (interrupted_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM spans WHERE run_id = ? AND status = 'interrupted'")
            .bind(&run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        interrupted_count, 2,
        "expected 2 interrupted spans (run-level + dangling tool)"
    );

    // The span→run map cleared on finalize (RunInterrupted triggers
    // drop_run_spans).
    let resolved = sqlite.resolve_run(&dangling_span).await;
    assert!(
        resolved.is_none(),
        "span→run map should be pruned after run finalize"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_finished_then_recorder_writes_completed_status() {
    let pool = migrated_pool().await;
    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = RunEventBus::new(vec![recorder]);

    let run_id = "run_clean_close".to_string();
    let now = Utc::now();
    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.clone(),
        objective: "happy path".to_string(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: now,
        retention_mode: "hash_only".to_string(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;
    bus.publish(RunEvent::RunFinished(RunFinishedEvent {
        run_id: run_id.clone(),
        finished_at: now + Duration::milliseconds(50),
        status: RunStatus::Completed,
        final_artifact_id: None,
        error: None,
    }))
    .await;

    // Wait for the row AND the status transition. The plain `agent_runs
    // count >= 1` check is satisfied after RunStarted's INSERT but
    // before RunFinished's UPDATE, leaving a race window where status
    // is still "running".
    let deadline = std::time::Instant::now() + StdDuration::from_secs(2);
    loop {
        let row: Option<(String,)> = sqlx::query_as("SELECT status FROM agent_runs WHERE id = ?")
            .bind(&run_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        if let Some((status,)) = row {
            if status == RunStatus::Completed.as_db_str() {
                return;
            }
        }
        if std::time::Instant::now() >= deadline {
            panic!("agent_runs row did not reach `completed` in time");
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
}

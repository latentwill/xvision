//! Snapshot test for the `xvn.agent_run.v2` JSON export.
//!
//! Inserts a deterministic fixture run via the `SqliteRecorder` event
//! path (so the rows exercise the same write surface production uses),
//! then calls `build_export` and asserts the serialized JSON matches
//! `tests/fixtures/xvn_run_v2.golden.json` byte-for-byte. A schema-
//! version bump is therefore the intentional way to land any breaking
//! change to the export shape.
//!
//! Also asserts the markdown report header carries `Retention: hash_only`
//! so reports never imply more retention than was on.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePoolOptions, Executor, SqlitePool};

use xvision_observability::{
    build_export, build_report,
    events::{
        ArtifactWrittenEvent, ModelCallFinishedEvent, RunFinishedEvent, RunStartedEvent, SpanFinishedEvent,
        SpanStartedEvent, SupervisorNoteEvent, ToolCallFinishedEvent, ToolCallStartedEvent,
    },
    types::{RiskLevel, RunStatus, SideEffectLevel, SpanKind, SpanStatus, ToolOrigin},
    AgentRunRecorder, RunEvent, RunEventBus, SqliteRecorder,
};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

const GOLDEN_JSON: &str = include_str!("fixtures/xvn_run_v2.golden.json");

const RUN_ID: &str = "run_export_fixture_01";
const ARTIFACT_ID: &str = "art_export_fixture_01";

/// Fixed timestamp the fixture pins so the golden file is stable across
/// runs. RFC3339 / Z so the recorder's timestamp-format round-trips.
///
/// The recorder truncates to whole seconds when it formats timestamps
/// back into SQLite (`SecondsFormat::Secs`), so we space events at
/// whole-second offsets to keep the round-tripped timestamps stable
/// across runs.
fn fixed_ts(offset_secs: i64) -> DateTime<Utc> {
    let base = DateTime::parse_from_rfc3339("2026-05-17T16:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    base + chrono::Duration::seconds(offset_secs)
}

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

/// Drive the recorder through a deterministic event stream and wait for
/// every row to be persisted. The bus is dropped at the end so the
/// background consumer task finishes before the assertions run.
async fn seed_run(pool: &SqlitePool) {
    let sqlite = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = RunEventBus::new(vec![sqlite.clone() as Arc<dyn AgentRunRecorder>]);

    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: RUN_ID.into(),
        objective: "Improve BTC mean reversion strategy".into(),
        strategy_id: Some("strat_export_fixture".into()),
        // Leave eval_run_id empty so the FK on agent_runs.eval_run_id
        // doesn't reject the insert — seeding the eval_runs row would
        // require migration 002's full create_run helpers.
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: fixed_ts(0),
        retention_mode: "hash_only".into(),
        trajectory_mode: None,
        sidecar_version: Some("sidecar-1.2.3".into()),
        cline_sdk_version: Some("cline-0.4.0".into()),
        protocol_version: Some("xvision/1".into()),
        skills_json: Some(r#"["financial-eval","supervisor-review"]"#.into()),
        mcp_servers_json: Some(r#"[{"name":"market_data","version":"0.1.0"}]"#.into()),
    }))
    .await;

    let root_span = "span_run_root".to_string();
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: root_span.clone(),
        run_id: RUN_ID.into(),
        parent_span_id: None,
        kind: SpanKind::AgentRun,
        name: "agent.run".into(),
        started_at: fixed_ts(1),
        otel_trace_id: Some("trace_export_fixture".into()),
        otel_span_id: Some("span_root_otel".into()),
        attributes_json: None,
    }))
    .await;

    // Model call.
    let model_span = "span_model_0".to_string();
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: model_span.clone(),
        run_id: RUN_ID.into(),
        parent_span_id: Some(root_span.clone()),
        kind: SpanKind::DecisionModel,
        name: "decision.model.plan".into(),
        started_at: fixed_ts(2),
        otel_trace_id: Some("trace_export_fixture".into()),
        otel_span_id: Some("span_model_otel".into()),
        attributes_json: None,
    }))
    .await;
    bus.publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
        span_id: model_span.clone(),
        provider: "anthropic".into(),
        model: "claude-opus-4-7".into(),
        input_token_count: Some(1234),
        output_token_count: Some(456),
        cost_usd: Some(0.0123),
        prompt_hash: "sha256:prompt_fixture".into(),
        response_hash: Some("sha256:response_fixture".into()),
        prompt_text: Some("prompt fixture plaintext".into()),
        response_text: Some("response fixture plaintext".into()),
        prompt_payload_ref: None,
        response_payload_ref: None,
        tool_calls_requested: Some(r#"["tool_fixture_0"]"#.into()),
        capability_path: None,
    }))
    .await;
    bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
        span_id: model_span.clone(),
        ended_at: fixed_ts(3),
        status: SpanStatus::Ok,
        error_json: None,
    }))
    .await;

    // Tool call.
    let tool_span = "span_tool_0".to_string();
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: tool_span.clone(),
        run_id: RUN_ID.into(),
        parent_span_id: Some(root_span.clone()),
        kind: SpanKind::ToolCall,
        name: "tool.call.fetch_bars".into(),
        started_at: fixed_ts(4),
        otel_trace_id: Some("trace_export_fixture".into()),
        otel_span_id: Some("span_tool_otel".into()),
        attributes_json: None,
    }))
    .await;
    bus.publish(RunEvent::ToolCallStarted(ToolCallStartedEvent {
        span_id: tool_span.clone(),
        tool_name: "fetch_bars".into(),
        origin: ToolOrigin::Native,
        tool_version: Some("0.1.0".into()),
        tool_hash: Some("sha256:tool_fixture".into()),
        side_effect_level: SideEffectLevel::ReadOnly,
        risk_level: RiskLevel::SafeRead,
        requires_approval: false,
        is_run_terminator: false,
        input_hash: "sha256:in_fixture".into(),
        input_payload_ref: None,
        input_text: None,
    }))
    .await;
    bus.publish(RunEvent::ToolCallFinished(ToolCallFinishedEvent {
        span_id: tool_span.clone(),
        output_hash: Some("sha256:out_fixture".into()),
        output_payload_ref: None,
        exit_code: Some(0),
        output_text: None,
    }))
    .await;
    bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
        span_id: tool_span,
        ended_at: fixed_ts(5),
        status: SpanStatus::Ok,
        error_json: None,
    }))
    .await;

    // Supervisor note. The recorder assigns a UUID id for notes; we
    // overwrite it to a deterministic value after the bus drains so
    // the golden file is stable.
    bus.publish(RunEvent::SupervisorNote(SupervisorNoteEvent {
        run_id: RUN_ID.into(),
        role: "reviewer".into(),
        content: "Sharpe drop on holdout suggests overfit.".into(),
        severity: "warn".into(),
        created_at: fixed_ts(6),
    }))
    .await;

    // Final artifact.
    bus.publish(RunEvent::ArtifactWritten(ArtifactWrittenEvent {
        artifact_id: ARTIFACT_ID.into(),
        run_id: RUN_ID.into(),
        kind: "final".into(),
        title: Some("Reduce overfitting in BTC mean reversion".into()),
        summary: Some("Tighten stops, drop a noisy feature.".into()),
        hypothesis: Some("Stops were too wide.".into()),
        recommendation: Some("Set stop ATR multiple to 1.5x.".into()),
        evidence_json: Some(
            r#"[{"label":"Sharpe drop on holdout","value":"0.4 -> 0.1","source_span_id":"span_model_0"}]"#
                .into(),
        ),
        next_experiments_json: Some(
            r#"[{"title":"Tighten stop ATR multiple","rationale":"Reduce drawdown."}]"#.into(),
        ),
        created_at: fixed_ts(7),
    }))
    .await;

    // Close run-level span + run.
    bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
        span_id: root_span,
        ended_at: fixed_ts(8),
        status: SpanStatus::Ok,
        error_json: None,
    }))
    .await;
    bus.publish(RunEvent::RunFinished(RunFinishedEvent {
        run_id: RUN_ID.into(),
        finished_at: fixed_ts(8),
        status: RunStatus::Completed,
        final_artifact_id: Some(ARTIFACT_ID.into()),
        error: None,
    }))
    .await;

    // Drop the bus so the consumer task exits and we know the recorder
    // drained every event before the test polls SQLite.
    drop(bus);
    // Tiny await yield so the consumer task finishes its last
    // `handle_event` after we drop the bus sender.
    wait_for_terminal_status(pool).await;

    // Patch the run row's `otel_trace_id`. The IPC-emission leaf owns
    // the recorder write that propagates `otel_trace_id` from
    // RunStarted; until that lands we set it via a small direct UPDATE
    // so the export surface can prove it round-trips.
    pool.execute(
        sqlx::query("UPDATE agent_runs SET otel_trace_id = 'trace_export_fixture' WHERE id = ?").bind(RUN_ID),
    )
    .await
    .unwrap();

    // Pin the supervisor note id so the golden file stays stable —
    // recorder generates a UUID per `SupervisorNote`, which would
    // otherwise diff on every run.
    pool.execute(
        sqlx::query("UPDATE supervisor_notes SET id = 'note_fixture_01' WHERE run_id = ?").bind(RUN_ID),
    )
    .await
    .unwrap();
}

async fn wait_for_terminal_status(pool: &SqlitePool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let row: Option<(String,)> = sqlx::query_as("SELECT status FROM agent_runs WHERE id = ?")
            .bind(RUN_ID)
            .fetch_optional(pool)
            .await
            .unwrap();
        if let Some((status,)) = row {
            if status == "completed" || std::time::Instant::now() >= deadline {
                return;
            }
        } else if std::time::Instant::now() >= deadline {
            panic!("agent_runs row never appeared");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn json_export_matches_golden_file() {
    let pool = migrated_pool().await;
    seed_run(&pool).await;

    let export = build_export(&pool, RUN_ID).await.unwrap();
    let actual = serde_json::to_string_pretty(&export).unwrap() + "\n";

    if actual != GOLDEN_JSON {
        // Print a unified-ish diff for the assertion message — full
        // file contents would dominate the cargo output.
        let actual_lines: Vec<&str> = actual.lines().collect();
        let expected_lines: Vec<&str> = GOLDEN_JSON.lines().collect();
        let mut diff = String::new();
        let max = actual_lines.len().max(expected_lines.len());
        for i in 0..max {
            let a = actual_lines.get(i).copied().unwrap_or("<missing>");
            let e = expected_lines.get(i).copied().unwrap_or("<missing>");
            if a != e {
                diff.push_str(&format!("line {}:\n  expected: {e}\n  actual:   {a}\n", i + 1));
            }
        }
        panic!(
            "xvn_run.json drift from golden file. \
             Bump `schema_version` if this is intentional.\n\n\
             --- diff ---\n{diff}\n\n\
             --- full actual ---\n{actual}\n"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn markdown_report_header_carries_retention_mode() {
    let pool = migrated_pool().await;
    seed_run(&pool).await;

    let report = build_report(&pool, RUN_ID).await.unwrap();
    assert!(
        report.markdown.contains("Retention: hash_only"),
        "report header must surface retention mode so consumers don't \
         assume more retention than was on:\n{}",
        report.markdown
    );
}

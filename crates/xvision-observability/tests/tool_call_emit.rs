//! F43 (`trace-dock-emitters`): asserts that the `tool_calls` table
//! has a writer path that lands rows, and that the `Redactor`
//! contract is honored by the producer (we test the redactor itself
//! here so callers can rely on it). The engine-side call site
//! invokes the redactor before passing args to
//! `ObsEmitter::emit_tool_call_started`; this test exercises both
//! the SQL writer + the redactor in isolation.

use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;
use xvision_observability::{
    AgentRunRecorder, Redactor, RiskLevel, RunEvent, RunStartedEvent, SideEffectLevel, SpanFinishedEvent,
    SpanKind, SpanStartedEvent, SpanStatus, SqliteRecorder, ToolCallFailedEvent, ToolCallFinishedEvent,
    ToolCallStartedEvent, ToolOrigin,
};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

async fn pool_with_run(run_id: &str) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    let rec = SqliteRecorder::new(pool.clone());
    rec.handle_event(&RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.into(),
        objective: "tool-emit-smoke".into(),
        strategy_id: None,
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: Utc::now(),
        retention_mode: "hash_only".into(),
        trajectory_mode: None,
        sidecar_version: None,
        cline_sdk_version: None,
        protocol_version: None,
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await
    .unwrap();
    pool
}

async fn open_tool_span(rec: &SqliteRecorder, run_id: &str, span_id: &str, tool_name: &str) {
    rec.handle_event(&RunEvent::SpanStarted(SpanStartedEvent {
        span_id: span_id.into(),
        run_id: run_id.into(),
        parent_span_id: None,
        kind: SpanKind::ToolCall,
        name: tool_name.into(),
        started_at: Utc::now(),
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await
    .unwrap();
}

#[tokio::test]
async fn tool_call_started_and_finished_land_a_row() {
    let run_id = "run_tool_1";
    let pool = pool_with_run(run_id).await;
    let rec = Arc::new(SqliteRecorder::new(pool.clone()));
    let span_id = "sp_tc_1";

    open_tool_span(&rec, run_id, span_id, "get_indicator").await;
    rec.handle_event(&RunEvent::ToolCallStarted(ToolCallStartedEvent {
        span_id: span_id.into(),
        tool_name: "get_indicator".into(),
        origin: ToolOrigin::Native,
        tool_version: None,
        tool_hash: None,
        side_effect_level: SideEffectLevel::ReadOnly,
        risk_level: RiskLevel::SafeRead,
        requires_approval: false,
        is_run_terminator: false,
        input_hash: "sha256:abc".into(),
        input_payload_ref: None,
        input_text: None,
    }))
    .await
    .unwrap();
    rec.handle_event(&RunEvent::ToolCallFinished(ToolCallFinishedEvent {
        span_id: span_id.into(),
        output_hash: Some("sha256:def".into()),
        output_payload_ref: None,
        exit_code: Some(0),
        output_text: None,
    }))
    .await
    .unwrap();
    rec.handle_event(&RunEvent::SpanFinished(SpanFinishedEvent {
        span_id: span_id.into(),
        ended_at: Utc::now(),
        status: SpanStatus::Ok,
        error_json: None,
    }))
    .await
    .unwrap();

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tool_calls WHERE span_id = ?")
        .bind(span_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "tool_call must persist as one tool_calls row");

    let (tool_name, input_hash, output_hash, exit_code, origin): (
        String,
        String,
        Option<String>,
        Option<i64>,
        String,
    ) = sqlx::query_as(
        "SELECT tool_name, input_hash, output_hash, exit_code, origin \
         FROM tool_calls WHERE span_id = ?",
    )
    .bind(span_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tool_name, "get_indicator");
    assert_eq!(input_hash, "sha256:abc");
    assert_eq!(output_hash.as_deref(), Some("sha256:def"));
    assert_eq!(exit_code, Some(0));
    assert_eq!(origin, "native");
}

#[tokio::test]
async fn tool_call_failed_marks_span_error_but_row_stays() {
    let run_id = "run_tool_2";
    let pool = pool_with_run(run_id).await;
    let rec = Arc::new(SqliteRecorder::new(pool.clone()));
    let span_id = "sp_tc_2";

    open_tool_span(&rec, run_id, span_id, "broken_tool").await;
    rec.handle_event(&RunEvent::ToolCallStarted(ToolCallStartedEvent {
        span_id: span_id.into(),
        tool_name: "broken_tool".into(),
        origin: ToolOrigin::Native,
        tool_version: None,
        tool_hash: None,
        side_effect_level: SideEffectLevel::ReadOnly,
        risk_level: RiskLevel::SafeRead,
        requires_approval: false,
        is_run_terminator: false,
        input_hash: "sha256:input".into(),
        input_payload_ref: None,
        input_text: None,
    }))
    .await
    .unwrap();
    rec.handle_event(&RunEvent::ToolCallFailed(ToolCallFailedEvent {
        span_id: span_id.into(),
        error_json: Some(r#"{"message":"boom"}"#.into()),
    }))
    .await
    .unwrap();

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tool_calls WHERE span_id = ?")
        .bind(span_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "failed tool call must still persist a row");

    let (status, error_json): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_json FROM spans WHERE id = ?")
            .bind(span_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "error");
    assert!(error_json.unwrap().contains("boom"));
}

#[tokio::test]
async fn redactor_strips_anthropic_and_openai_keys_from_tool_args() {
    // The engine call site runs `Redactor::new().redact(args)` on the
    // raw tool input JSON before passing it to `emit_tool_call_started`.
    // This test confirms the redactor catches the most common provider
    // and broker token shapes so a careless agent's tool argument can't
    // leak through to the tool_calls row's input hash / blob ref.
    let r = Redactor::new();

    // Anthropic admin key.
    let with_anthropic = r#"{"text":"sk-ant-api03-XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX-XXXXXXXXXXX"}"#;
    let out = r.redact(with_anthropic);
    assert!(!out.text.contains("sk-ant-api03-"));
    assert!(!out.matches.is_empty());

    // OpenAI key (legacy bare `sk-` form — the v1 redactor pattern is
    // `sk-[A-Za-z0-9]{32,}` so we match without `-proj-` segmentation).
    let with_openai = "OPENAI_API_KEY=sk-abcdEFGH1234567890ABCDEFGHIJ1234";
    let out = r.redact(with_openai);
    assert!(!out.text.contains("sk-abcdEFGH"));
    assert!(out.text.contains("[redacted:openai_api_key]"));

    // Alpaca-style broker key — covered by the v1 pattern set
    // (`PK` / `AK` prefix + 16+ alnum). Common in operator-typed tool
    // args; F43's redaction contract relies on this so a tool argument
    // can't sneak a broker key into the `tool_calls.input_hash` input.
    let with_alpaca = "PKABCDEF1234567890XYZ";
    let out = r.redact(with_alpaca);
    assert!(!out.text.contains("PKABCDEF12"));
}

#[tokio::test]
async fn tool_call_origin_round_trips_through_db() {
    // The `origin` column carries `native | mcp:<server> | cline_builtin`.
    // F43's emit path defaults to `native`; this test pins the round-trip
    // so the dashboard's origin filter has a stable schema contract.
    let run_id = "run_tool_3";
    let pool = pool_with_run(run_id).await;
    let rec = Arc::new(SqliteRecorder::new(pool.clone()));

    for (span_id, origin, expected) in [
        ("sp_native", ToolOrigin::Native, "native"),
        ("sp_mcp", ToolOrigin::Mcp("alpaca".into()), "mcp:alpaca"),
        ("sp_builtin", ToolOrigin::ClineBuiltin, "cline_builtin"),
    ] {
        open_tool_span(&rec, run_id, span_id, "t").await;
        rec.handle_event(&RunEvent::ToolCallStarted(ToolCallStartedEvent {
            span_id: span_id.into(),
            tool_name: "t".into(),
            origin,
            tool_version: None,
            tool_hash: None,
            side_effect_level: SideEffectLevel::ReadOnly,
            risk_level: RiskLevel::SafeRead,
            requires_approval: false,
            is_run_terminator: false,
            input_hash: "sha256:x".into(),
            input_payload_ref: None,
            input_text: None,
        }))
        .await
        .unwrap();

        let (db_origin,): (String,) = sqlx::query_as("SELECT origin FROM tool_calls WHERE span_id = ?")
            .bind(span_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(db_origin, expected);
    }
}

//! F43 (`trace-dock-emitters`): SQL writer integration tests for the
//! new `events` table writer and the broadened `supervisor_notes`
//! emission paths. Asserts that an `EngineEvent` published onto the
//! bus lands as an `events` row with the expected kind / payload,
//! and that the supervisor-note emitter writes through the same path
//! the engine eval executor uses.

use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;
use xvision_observability::{
    AgentRunRecorder, EngineEvent, RunEvent, RunFinishedEvent, RunStartedEvent, RunStatus, SpanKind,
    SpanStartedEvent, SqliteRecorder, SupervisorNoteEvent,
};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

async fn run_status(pool: &SqlitePool, run_id: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM agent_runs WHERE id = ?")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn finished(run_id: &str, status: RunStatus, error: Option<&str>) -> RunEvent {
    RunEvent::RunFinished(RunFinishedEvent {
        run_id: run_id.into(),
        finished_at: Utc::now(),
        status,
        final_artifact_id: None,
        error: error.map(|s| s.to_string()),
    })
}

// xvnej F5 (2026-06-04): a failed step row must not be downgraded to
// `completed` by a late, unconditional sidecar `run_finished(completed)`.
// The recorder makes `failed` terminal-sticky, so the child row ends up
// `failed` regardless of which event lands first.

#[tokio::test]
async fn failed_is_sticky_against_late_completed() {
    let run_id = "run::trader::cycle0";
    let pool = pool_with_run(run_id).await;
    let rec = SqliteRecorder::new(pool.clone());

    rec.handle_event(&finished(
        run_id,
        RunStatus::Failed,
        Some("step did not complete"),
    ))
    .await
    .unwrap();
    // Late, unconditional `completed` from the sidecar — must NOT win.
    rec.handle_event(&finished(run_id, RunStatus::Completed, None))
        .await
        .unwrap();

    assert_eq!(run_status(&pool, run_id).await, "failed");
}

#[tokio::test]
async fn failed_overwrites_prior_completed() {
    let run_id = "run::trader::cycle0";
    let pool = pool_with_run(run_id).await;
    let rec = SqliteRecorder::new(pool.clone());

    // Opposite arrival order: `completed` lands first, then the engine's
    // `emit_child_run_failed`. `failed` must still win.
    rec.handle_event(&finished(run_id, RunStatus::Completed, None))
        .await
        .unwrap();
    rec.handle_event(&finished(
        run_id,
        RunStatus::Failed,
        Some("step did not complete"),
    ))
    .await
    .unwrap();

    assert_eq!(run_status(&pool, run_id).await, "failed");
}

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
        objective: "smoke".into(),
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

#[tokio::test]
async fn engine_event_lands_as_events_row() {
    let run_id = "run_evt_1";
    let pool = pool_with_run(run_id).await;
    let rec = Arc::new(SqliteRecorder::new(pool.clone()));

    // Open a span first so the events.span_id FK has a target.
    rec.handle_event(&RunEvent::SpanStarted(SpanStartedEvent {
        span_id: "sp_dec_001".into(),
        run_id: run_id.into(),
        parent_span_id: None,
        kind: SpanKind::AgentDecision,
        name: "decision#7".into(),
        started_at: Utc::now(),
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await
    .unwrap();

    let ev = RunEvent::EngineEvent(EngineEvent {
        run_id: run_id.into(),
        span_id: Some("sp_dec_001".into()),
        kind: "decision_started".into(),
        payload_json: Some(r#"{"decision_index":7,"asset":"BTC/USD"}"#.into()),
        created_at: Utc::now(),
    });
    rec.handle_event(&ev).await.unwrap();

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE run_id = ?")
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "engine event must persist as one events row");

    let (kind, span_id, payload): (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT kind, span_id, payload_json FROM events WHERE run_id = ?")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind, "decision_started");
    assert_eq!(span_id.as_deref(), Some("sp_dec_001"));
    assert!(payload.unwrap().contains("BTC/USD"));
}

#[tokio::test]
async fn multiple_lifecycle_event_kinds_round_trip() {
    let run_id = "run_evt_2";
    let pool = pool_with_run(run_id).await;
    let rec = Arc::new(SqliteRecorder::new(pool.clone()));

    for kind in [
        "decision_started",
        "decision_completed",
        "fill_attempted",
        "guardrail_fired",
        "early_stop_triggered",
        "flat_skip_fired",
        "broker_rule_violation",
        "cost_cap_warning",
    ] {
        rec.handle_event(&RunEvent::EngineEvent(EngineEvent {
            run_id: run_id.into(),
            span_id: None,
            kind: kind.into(),
            payload_json: None,
            created_at: Utc::now(),
        }))
        .await
        .unwrap();
    }

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE run_id = ?")
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 8, "all 8 F43 engine-event kinds must persist");

    // Spot-check that each unique kind is represented.
    let kinds: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT kind FROM events WHERE run_id = ?")
        .bind(run_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    let kind_names: std::collections::HashSet<_> = kinds.into_iter().map(|(k,)| k).collect();
    for required in ["decision_started", "guardrail_fired", "flat_skip_fired"] {
        assert!(kind_names.contains(required), "missing kind {required}");
    }
}

#[tokio::test]
async fn supervisor_note_broadening_persists_through_writer() {
    // F43 § 3 broadens the supervisor_notes emit surface beyond the
    // F-7 guardrail rewrite path. This test confirms the writer path
    // a guard/preflight/broker producer would use lands a row, and
    // that severity / role columns reflect the input.
    let run_id = "run_evt_3";
    let pool = pool_with_run(run_id).await;
    let rec = Arc::new(SqliteRecorder::new(pool.clone()));

    for (role, severity, content) in [
        ("guard", "warn", "broker rule rejected order at decision 5"),
        ("system", "info", "preflight warning: catalog stale"),
        ("guard", "info", "trader-noop-skip fired at cycle 12"),
    ] {
        rec.handle_event(&RunEvent::SupervisorNote(SupervisorNoteEvent {
            run_id: run_id.into(),
            role: role.into(),
            content: content.into(),
            severity: severity.into(),
            created_at: Utc::now(),
        }))
        .await
        .unwrap();
    }

    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT role, severity, content FROM supervisor_notes WHERE run_id = ?")
            .bind(run_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 3);
    // Ensure the broker_rule warning and noop_skip note both made it.
    let contents: Vec<&str> = rows.iter().map(|(_, _, c)| c.as_str()).collect();
    assert!(contents.iter().any(|c| c.contains("broker rule rejected order")));
    assert!(contents.iter().any(|c| c.contains("trader-noop-skip")));
}

#[tokio::test]
async fn agent_decision_span_kind_serializes() {
    // The new `agent.decision` SpanKind variant must round-trip the
    // dotted wire form so the dashboard's `kind` filter matches.
    let v = serde_json::to_value(SpanKind::AgentDecision).unwrap();
    assert_eq!(v, serde_json::json!("agent.decision"));
    assert_eq!(SpanKind::AgentDecision.as_db_str(), "agent.decision");
    let back: SpanKind = serde_json::from_value(v).unwrap();
    assert_eq!(back, SpanKind::AgentDecision);
}

#[tokio::test]
async fn engine_event_routing_returns_run_and_span() {
    let ev = RunEvent::EngineEvent(EngineEvent {
        run_id: "run_xyz".into(),
        span_id: Some("sp_abc".into()),
        kind: "decision_completed".into(),
        payload_json: None,
        created_at: Utc::now(),
    });
    assert_eq!(ev.run_id(), "run_xyz");
    assert_eq!(ev.span_id(), Some("sp_abc"));

    let ev_no_span = RunEvent::EngineEvent(EngineEvent {
        run_id: "run_xyz".into(),
        span_id: None,
        kind: "early_stop_triggered".into(),
        payload_json: None,
        created_at: Utc::now(),
    });
    assert_eq!(ev_no_span.run_id(), "run_xyz");
    assert_eq!(ev_no_span.span_id(), None);
}

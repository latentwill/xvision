//! Regression: `agent_runs` baseline ordering for View Trace +
//! supervisor_notes FK integrity.
//!
//! Bug summary (2026-05-26 QA): eval kickoff used to write
//! `supervisor_notes` (provider_override receipt, preflight passed/
//! skipped notes) BEFORE the async observability bus delivered
//! `RunStarted`, which is what wrote the parent `agent_runs` row.
//! With production FK enforcement on, every note insert hit `FOREIGN
//! KEY constraint failed`, was swallowed by a "best-effort" WARN log,
//! and downstream observability events also orphaned. The frontend's
//! "View Trace" button then 404'd because the `agent_runs` row never
//! existed.
//!
//! Fix: `RunStore::ensure_agent_run_baseline` synchronously seeds the
//! `agent_runs` row immediately after `eval_runs` is created, and the
//! bus recorder's `RunStarted` handler upserts metadata onto that
//! baseline (rather than UNIQUE-conflicting). This test exercises
//! both halves on a FK-enabled pool.

use std::{sync::Arc, time::Duration};

use chrono::Utc;
use sqlx::{Row, SqlitePool};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::store::RunStore;
use xvision_observability::{
    AgentRunRecorder, RunEvent, RunEventBus, RunStartedEvent, SqliteRecorder,
};

async fn fk_on_pool() -> (SqlitePool, tempfile::TempDir) {
    // Temp-file SQLite — see eval_observability::setup_pool for the
    // reason in-memory doesn't work with a multi-connection pool.
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    // Mirror eval_guardrail_summary's fresh_store migration set so
    // `RunStore::create` (which writes all current eval_runs columns)
    // has every required column. FK is ON here — the whole point of
    // this test is that production-shaped pools satisfy the FK
    // ordering invariant.
    for sql in [
        include_str!("../migrations/001_api_audit.sql"),
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/013_cli_jobs.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/017_eval_findings_review_columns.sql"),
        include_str!("../migrations/018_agent_run_observability.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/026_trace_surface_foundation.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    (pool, tmp)
}

/// Single-id pattern: `agent_runs.id == eval_runs.id`. The frontend's
/// `traceRunId = agent_run_id ?? eval_run.id` fallback relies on it.
fn single_id() -> String {
    "01TESTAGENTRUNBASELINE000001".to_string()
}

#[tokio::test]
async fn ensure_agent_run_baseline_creates_parent_with_fk_on() {
    let (pool, _tmp) = fk_on_pool().await;
    let store = RunStore::new(pool.clone());

    let run_id = single_id();
    let mut run = Run::new_queued("a".into(), "s".into(), RunMode::Backtest);
    run.id = run_id.clone();

    store.create(&run).await.expect("create eval_runs row");
    store
        .ensure_agent_run_baseline(&run_id, "hash_only")
        .await
        .expect("baseline insert must succeed with FK on");

    // After the baseline call, a supervisor_note write (the exact call
    // pattern the eval kickoff uses for provider_override and
    // preflight receipts) must succeed without an FK violation.
    store
        .record_supervisor_note(&run_id, "provider_override", "info", "{\"smoke\":true}")
        .await
        .expect("supervisor_notes insert succeeds when baseline ran first");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_runs WHERE id = ?")
        .bind(&run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "exactly one agent_runs row for the eval id");

    let note_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM supervisor_notes WHERE run_id = ?")
            .bind(&run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(note_count, 1, "supervisor_notes row must have landed");
}

#[tokio::test]
async fn supervisor_note_without_baseline_fails_loudly() {
    // Inverse: confirm the "fail loudly" contract. Pre-fix, this call
    // would log a WARN and return Ok(()). After the fix it returns
    // Err so the kickoff site can't silently ship a broken trace.
    let (pool, _tmp) = fk_on_pool().await;
    let store = RunStore::new(pool.clone());

    let run_id = "01TESTNOBASELINE000000000000";
    let res = store
        .record_supervisor_note(run_id, "preflight", "info", "no parent on purpose")
        .await;
    assert!(
        res.is_err(),
        "record_supervisor_note must error when parent agent_runs row is missing; \
         silently swallowing this is the bug that hid View Trace breakage across \
         multiple QA cycles. got: {res:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_started_event_upserts_onto_baseline() {
    // The bus recorder used to plain-INSERT `agent_runs` on
    // RunStarted, which would have UNIQUE-conflicted with the
    // synchronous baseline row. The fix made it an UPSERT that
    // backfills metadata (objective, sidecar_version, …) onto the
    // existing row. Status is deliberately preserved (the run may
    // have already terminated by the time the bus delivers
    // RunStarted; we must not regress its status back to 'running').

    let (pool, _tmp) = fk_on_pool().await;
    let store = RunStore::new(pool.clone());
    let run_id = "01TESTUPSERT0000000000000001";

    let mut run = Run::new_queued("a".into(), "s".into(), RunMode::Backtest);
    run.id = run_id.to_string();
    store.create(&run).await.unwrap();
    store
        .ensure_agent_run_baseline(run_id, "hash_only")
        .await
        .unwrap();

    let recorder: Arc<dyn AgentRunRecorder> = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = Arc::new(RunEventBus::new(vec![recorder]));

    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.to_string(),
        objective: "eval:Backtest:test-scenario".to_string(),
        strategy_id: Some("strategy-test".to_string()),
        eval_run_id: Some(run_id.to_string()),
        source_cli_job_id: None,
        started_at: Utc::now(),
        retention_mode: "full_debug".to_string(),
        trajectory_mode: None,
        sidecar_version: Some("sidecar-x.y.z".to_string()),
        cline_sdk_version: Some("cline-1.2.3".to_string()),
        protocol_version: Some("xvision/1".to_string()),
        skills_json: Some("[]".to_string()),
        mcp_servers_json: Some("[]".to_string()),
    }))
    .await;

    // Drain the bus so the recorder has committed before we read.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let row = loop {
        bus.quiesce().await;
        let r = sqlx::query(
            "SELECT objective, strategy_id, retention_mode, sidecar_version, \
                    cline_sdk_version, protocol_version, status \
             FROM agent_runs WHERE id = ?",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let obj: String = r.get("objective");
        if obj == "eval:Backtest:test-scenario" {
            break r;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for RunStarted UPSERT to backfill objective; \
                 still saw: {obj}"
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };

    assert_eq!(row.get::<String, _>("objective"), "eval:Backtest:test-scenario");
    assert_eq!(row.get::<String, _>("strategy_id"), "strategy-test");
    assert_eq!(row.get::<String, _>("retention_mode"), "full_debug");
    assert_eq!(row.get::<String, _>("sidecar_version"), "sidecar-x.y.z");
    assert_eq!(row.get::<String, _>("cline_sdk_version"), "cline-1.2.3");
    assert_eq!(row.get::<String, _>("protocol_version"), "xvision/1");
    // Status must remain 'running' from the baseline — RunStarted's
    // 'running' value is fine here (no terminal transition happened),
    // but the contract we care about is that the recorder did NOT
    // UNIQUE-conflict and the row count is still one.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_runs WHERE id = ?")
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "UPSERT must not duplicate the row");
}

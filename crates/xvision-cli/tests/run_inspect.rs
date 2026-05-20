//! End-to-end check for `xvn run inspect <id>`.
//!
//! Seeds an `agent_runs` row + its detail rows via the observability
//! crate's `SqliteRecorder`, then invokes the CLI against an isolated
//! `$XVN_HOME` and asserts that both `xvn_run.json` and `xvn_report.md`
//! land in the requested output directory with the correct top-level
//! keys and headers.

use std::process::Command;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tempfile::tempdir;

use xvision_observability::{
    events::{RunFinishedEvent, RunStartedEvent, SpanFinishedEvent, SpanStartedEvent},
    types::{RunStatus, SpanKind, SpanStatus},
    AgentRunRecorder, RunEvent, RunEventBus, SqliteRecorder,
};

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

fn fixed_ts(offset_secs: i64) -> DateTime<Utc> {
    let base = DateTime::parse_from_rfc3339("2026-05-17T16:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    base + chrono::Duration::seconds(offset_secs)
}

async fn seed_run(db_path: &std::path::Path, run_id: &str) {
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url).await.expect("open sqlite");
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();

    let recorder = Arc::new(SqliteRecorder::new(pool.clone()));
    let bus = RunEventBus::new(vec![recorder as Arc<dyn AgentRunRecorder>]);

    bus.publish(RunEvent::RunStarted(RunStartedEvent {
        run_id: run_id.into(),
        objective: "CLI inspect smoke".into(),
        strategy_id: Some("strat_inspect_smoke".into()),
        eval_run_id: None,
        source_cli_job_id: None,
        started_at: fixed_ts(0),
        retention_mode: "hash_only".into(),
        sidecar_version: Some("sidecar-test".into()),
        cline_sdk_version: Some("cline-test".into()),
        protocol_version: Some("xvision/1".into()),
        skills_json: None,
        mcp_servers_json: None,
    }))
    .await;

    let span = "span_root".to_string();
    bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
        span_id: span.clone(),
        run_id: run_id.into(),
        parent_span_id: None,
        kind: SpanKind::AgentRun,
        name: "agent.run".into(),
        started_at: fixed_ts(1),
        otel_trace_id: None,
        otel_span_id: None,
        attributes_json: None,
    }))
    .await;
    bus.publish(RunEvent::SpanFinished(SpanFinishedEvent {
        span_id: span,
        ended_at: fixed_ts(2),
        status: SpanStatus::Ok,
        error_json: None,
    }))
    .await;
    bus.publish(RunEvent::RunFinished(RunFinishedEvent {
        run_id: run_id.into(),
        finished_at: fixed_ts(3),
        status: RunStatus::Completed,
        final_artifact_id: None,
        error: None,
    }))
    .await;

    // Drop the bus so the consumer task drains every event, then poll
    // for the terminal status before we let the CLI process open the
    // file. Eviction is impossible on this tiny stream so the wait is
    // bounded by the recorder's own write latency.
    drop(bus);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let row: Option<(String,)> = sqlx::query_as("SELECT status FROM agent_runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        if let Some((status,)) = row {
            if status == "completed" {
                break;
            }
        }
        if std::time::Instant::now() >= deadline {
            panic!("run never reached completed status");
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    pool.close().await;
}

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

#[test]
fn inspect_writes_both_files_into_out_dir() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

    let run_id = "run_inspect_cli_01";
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(seed_run(&db_path, run_id));

    let out = xvn(
        &[
            "run",
            "inspect",
            run_id,
            "--db",
            db_path.to_str().unwrap(),
            "--out",
            out_dir.path().to_str().unwrap(),
        ],
        home.path(),
    );
    assert!(
        out.status.success(),
        "xvn run inspect failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json_path = out_dir.path().join("xvn_run.json");
    let md_path = out_dir.path().join("xvn_report.md");
    assert!(json_path.exists(), "xvn_run.json was not written");
    assert!(md_path.exists(), "xvn_report.md was not written");

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&json_path).unwrap()).expect("json parse");
    assert_eq!(json["schema_version"], "xvn.agent_run.v1");
    assert_eq!(json["run_id"], run_id);
    for key in [
        "schema_version",
        "run_id",
        "objective",
        "status",
        "retention_mode",
        "started_at",
        "totals",
        "spans",
        "model_calls",
        "tool_calls",
        "approvals",
        "sandbox_results",
        "supervisor_notes",
        "sidecar_version",
        "cline_sdk_version",
        "protocol_version",
        "mcp_servers",
        "skills",
    ] {
        assert!(
            json.get(key).is_some(),
            "missing top-level key `{key}` in xvn_run.json"
        );
    }

    let md = std::fs::read_to_string(&md_path).unwrap();
    assert!(
        md.contains("Retention: hash_only"),
        "report header must surface retention mode:\n{md}"
    );
}

#[test]
fn inspect_is_idempotent_on_finished_runs() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

    let run_id = "run_inspect_cli_idem";
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(seed_run(&db_path, run_id));

    let inspect_args = [
        "run",
        "inspect",
        run_id,
        "--db",
        db_path.to_str().unwrap(),
        "--out",
        out_dir.path().to_str().unwrap(),
    ];
    let out = xvn(&inspect_args, home.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json_path = out_dir.path().join("xvn_run.json");
    let md_path = out_dir.path().join("xvn_report.md");
    let first_json = std::fs::read(&json_path).unwrap();
    let first_md = std::fs::read(&md_path).unwrap();

    let out = xvn(&inspect_args, home.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Second invocation produced identical bytes: that's the
    // idempotency contract from acceptance criteria.
    let second_json = std::fs::read(&json_path).unwrap();
    let second_md = std::fs::read(&md_path).unwrap();
    assert_eq!(first_json, second_json);
    assert_eq!(first_md, second_md);
}

#[test]
fn inspect_unknown_run_id_returns_not_found() {
    let home = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // Seed something to ensure the DB exists; ask for a different id.
    rt.block_on(seed_run(&db_path, "run_exists"));

    let out_dir = tempdir().unwrap();
    let out = xvn(
        &[
            "run",
            "inspect",
            "run_does_not_exist",
            "--db",
            db_path.to_str().unwrap(),
            "--out",
            out_dir.path().to_str().unwrap(),
        ],
        home.path(),
    );
    let code = out.status.code().expect("clean exit");
    assert_eq!(
        code,
        4,
        "expected XvnExit::NotFound=4 for unknown run id, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn inspect_stdout_requires_json_format() {
    let home = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(seed_run(&db_path, "run_stdout"));

    let out = xvn(
        &[
            "run",
            "inspect",
            "run_stdout",
            "--db",
            db_path.to_str().unwrap(),
            "--out",
            "-",
        ],
        home.path(),
    );
    assert_eq!(
        out.status.code().expect("clean exit"),
        2,
        "expected XvnExit::Usage=2 when --out - is used without --format json; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

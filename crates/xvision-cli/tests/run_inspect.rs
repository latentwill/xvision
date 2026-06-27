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
        trajectory_mode: None,
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

async fn migrated_eval_db(db_path: &std::path::Path) -> SqlitePool {
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url).await.expect("open sqlite");
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    pool
}

async fn migrated_legacy_agent_db_without_eval_table(db_path: &std::path::Path) -> SqlitePool {
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url).await.expect("open sqlite");
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    let legacy_agent_run_migration = MIGRATION_018.replace(
        "    FOREIGN KEY (eval_run_id)       REFERENCES eval_runs(id),\n",
        "",
    );
    sqlx::query(&legacy_agent_run_migration)
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn insert_eval_run(
    pool: &SqlitePool,
    eval_run_id: &str,
    mode: &str,
    status: &str,
    completed_at: Option<DateTime<Utc>>,
    actual_input_tokens: Option<i64>,
    actual_output_tokens: Option<i64>,
) {
    sqlx::query(
        "INSERT INTO eval_runs \
         (id, strategy_bundle_hash, scenario_id, params_override_json, mode, status, \
          started_at, completed_at, metrics_json, error, \
          estimated_total_tokens, actual_input_tokens, actual_output_tokens) \
         VALUES (?, ?, ?, NULL, ?, ?, ?, ?, NULL, NULL, NULL, ?, ?)",
    )
    .bind(eval_run_id)
    .bind("strat_eval_fixture")
    .bind("scenario_eval_fixture")
    .bind(mode)
    .bind(status)
    .bind(fixed_ts(0).to_rfc3339())
    .bind(completed_at.map(|ts| ts.to_rfc3339()))
    .bind(actual_input_tokens)
    .bind(actual_output_tokens)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_agent_run_row(
    pool: &SqlitePool,
    run_id: &str,
    eval_run_id: Option<&str>,
    status: &str,
    finished_at: Option<DateTime<Utc>>,
) {
    sqlx::query(
        "INSERT INTO agent_runs \
         (id, objective, strategy_id, eval_run_id, source_cli_job_id, status, started_at, finished_at, \
          retention_mode, sidecar_version, cline_sdk_version, protocol_version, skills_json, \
          mcp_servers_json, otel_trace_id, final_artifact_id, error) \
         VALUES (?, ?, ?, ?, NULL, ?, ?, ?, 'hash_only', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(run_id)
    .bind("Eval-linked inspect fixture")
    .bind("strat_inspect_eval")
    .bind(eval_run_id)
    .bind(status)
    .bind(fixed_ts(1).to_rfc3339())
    .bind(finished_at.map(|ts| ts.to_rfc3339()))
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_stale_sidecar_eval(
    db_path: &std::path::Path,
    run_id: &str,
    eval_run_id: &str,
    mode: &str,
) -> DateTime<Utc> {
    let pool = migrated_eval_db(db_path).await;
    let completed_at = fixed_ts(9);
    insert_eval_run(
        &pool,
        eval_run_id,
        mode,
        "completed",
        Some(completed_at),
        Some(46473),
        Some(991),
    )
    .await;
    insert_agent_run_row(&pool, run_id, Some(eval_run_id), "running", None).await;
    pool.close().await;
    completed_at
}

async fn seed_status_precedence_eval(
    db_path: &std::path::Path,
    run_id: &str,
    eval_run_id: &str,
    eval_status: &str,
    sidecar_status: &str,
    eval_finished_at: Option<DateTime<Utc>>,
    sidecar_finished_at: Option<DateTime<Utc>>,
) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let pool = migrated_eval_db(db_path).await;
    insert_eval_run(
        &pool,
        eval_run_id,
        "backtest",
        eval_status,
        eval_finished_at,
        Some(100),
        Some(25),
    )
    .await;
    insert_agent_run_row(
        &pool,
        run_id,
        Some(eval_run_id),
        sidecar_status,
        sidecar_finished_at,
    )
    .await;
    pool.close().await;
    (eval_finished_at, sidecar_finished_at)
}

async fn seed_linked_model_call_eval(db_path: &std::path::Path, run_id: &str, eval_run_id: &str) {
    let pool = migrated_eval_db(db_path).await;
    insert_eval_run(
        &pool,
        eval_run_id,
        "backtest",
        "completed",
        Some(fixed_ts(12)),
        Some(999),
        Some(999),
    )
    .await;
    insert_agent_run_row(&pool, run_id, Some(eval_run_id), "completed", Some(fixed_ts(12))).await;
    sqlx::query(
        "INSERT INTO spans \
         (id, run_id, parent_span_id, otel_trace_id, otel_span_id, kind, name, status, \
          started_at, ended_at, duration_ms, attributes_json, error_json) \
         VALUES ('span_model_linked', ?, NULL, NULL, NULL, 'model.call', 'model.call', 'ok', ?, ?, 1000, NULL, NULL)",
    )
    .bind(run_id)
    .bind(fixed_ts(2).to_rfc3339())
    .bind(fixed_ts(3).to_rfc3339())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_calls \
         (span_id, provider, model, input_token_count, output_token_count, cost_usd, \
          prompt_hash, response_hash, prompt_payload_ref, response_payload_ref, \
          tool_calls_requested, capability_path) \
         VALUES ('span_model_linked', 'openai', 'gpt-5', 123, 45, 0.067, \
                 'sha256:prompt', 'sha256:response', NULL, NULL, NULL, NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

async fn seed_direct_eval_only(db_path: &std::path::Path, eval_run_id: &str) -> DateTime<Utc> {
    let pool = migrated_eval_db(db_path).await;
    let completed_at = fixed_ts(14);
    insert_eval_run(
        &pool,
        eval_run_id,
        "live",
        "completed",
        Some(completed_at),
        Some(777),
        Some(55),
    )
    .await;
    pool.close().await;
    completed_at
}

async fn seed_legacy_agent_only(db_path: &std::path::Path, run_id: &str) {
    let pool = migrated_legacy_agent_db_without_eval_table(db_path).await;
    insert_agent_run_row(&pool, run_id, None, "completed", Some(fixed_ts(8))).await;
    pool.close().await;
}

fn inspect_to_dir(
    db_path: &std::path::Path,
    run_id: &str,
    home: &std::path::Path,
    out_dir: &std::path::Path,
) -> serde_json::Value {
    let out = xvn(
        &[
            "run",
            "inspect",
            run_id,
            "--db",
            db_path.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ],
        home,
    );
    assert!(
        out.status.success(),
        "xvn run inspect failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&std::fs::read(out_dir.join("xvn_run.json")).unwrap()).expect("json parse")
}

fn fmt_ts(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
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
    assert_eq!(json["schema_version"], "xvn.agent_run.v3");
    assert_eq!(json["run_id"], run_id);
    for key in [
        "schema_version",
        "run_id",
        "objective",
        "status",
        "retention_mode",
        "started_at",
        "totals",
        "accounting",
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
    assert_eq!(json["accounting"]["source"], "none");

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

#[test]
fn inspect_reconciles_completed_eval_accounting_when_sidecar_is_stale() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let run_id = "agent_stale_backtest";
    let eval_run_id = "eval_completed_backtest";
    let completed_at = rt.block_on(seed_stale_sidecar_eval(&db_path, run_id, eval_run_id, "backtest"));

    let json = inspect_to_dir(&db_path, run_id, home.path(), out_dir.path());
    assert_eq!(json["schema_version"], "xvn.agent_run.v3");
    assert_eq!(json["status"], "completed");
    assert_eq!(json["finished_at"], fmt_ts(completed_at));
    assert_eq!(json["eval_run_id"], eval_run_id);
    assert_eq!(json["totals"]["input_tokens"], 46473);
    assert_eq!(json["totals"]["output_tokens"], 991);
    assert_eq!(json["totals"]["model_calls"], 0);
    assert_eq!(json["accounting"]["source"], "eval_actuals");
    assert_eq!(json["accounting"]["eval_status"], "completed");
    assert_eq!(json["accounting"]["eval_mode"], "backtest");

    let md = std::fs::read_to_string(out_dir.path().join("xvn_report.md")).unwrap();
    assert!(md.contains("- Status: completed"), "{md}");
    assert!(
        md.contains(&format!("- Finished at: {}", fmt_ts(completed_at))),
        "{md}"
    );
    assert!(md.contains(&format!("- Eval run: {eval_run_id}")), "{md}");
    assert!(md.contains("eval_actuals"), "{md}");
}

#[test]
fn inspect_reconciles_live_eval_accounting_when_sidecar_is_stale() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let run_id = "agent_stale_live";
    let eval_run_id = "eval_completed_live";
    let completed_at = rt.block_on(seed_stale_sidecar_eval(&db_path, run_id, eval_run_id, "live"));

    let json = inspect_to_dir(&db_path, run_id, home.path(), out_dir.path());
    assert_eq!(json["schema_version"], "xvn.agent_run.v3");
    assert_eq!(json["status"], "completed");
    assert_eq!(json["finished_at"], fmt_ts(completed_at));
    assert_eq!(json["totals"]["input_tokens"], 46473);
    assert_eq!(json["totals"]["output_tokens"], 991);
    assert_eq!(json["accounting"]["source"], "eval_actuals");
    assert_eq!(json["accounting"]["eval_mode"], "fwd");
    assert_eq!(json["accounting"]["eval_status"], "completed");
}

#[test]
fn inspect_stdout_json_reconciles_eval_accounting() {
    let home = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let run_id = "agent_stale_stdout";
    let eval_run_id = "eval_completed_stdout";
    let completed_at = rt.block_on(seed_stale_sidecar_eval(&db_path, run_id, eval_run_id, "backtest"));

    let out = xvn(
        &[
            "run",
            "inspect",
            run_id,
            "--db",
            db_path.to_str().unwrap(),
            "--out",
            "-",
            "--format",
            "json",
        ],
        home.path(),
    );
    assert!(
        out.status.success(),
        "xvn run inspect stdout failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout json");
    assert_eq!(json["schema_version"], "xvn.agent_run.v3");
    assert_eq!(json["status"], "completed");
    assert_eq!(json["finished_at"], fmt_ts(completed_at));
    assert_eq!(json["totals"]["input_tokens"], 46473);
    assert_eq!(json["accounting"]["source"], "eval_actuals");
}

#[test]
fn inspect_preserves_agent_model_call_details_when_eval_is_linked() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let run_id = "agent_linked_model_call";
    let eval_run_id = "eval_linked_model_call";
    rt.block_on(seed_linked_model_call_eval(&db_path, run_id, eval_run_id));

    let json = inspect_to_dir(&db_path, run_id, home.path(), out_dir.path());
    assert_eq!(json["schema_version"], "xvn.agent_run.v3");
    assert_eq!(json["model_calls"][0]["provider"], "openai");
    assert_eq!(json["model_calls"][0]["model"], "gpt-5");
    assert_eq!(json["totals"]["model_calls"], 1);
    assert_eq!(json["totals"]["input_tokens"], 123);
    assert_eq!(json["totals"]["output_tokens"], 45);
    assert_eq!(json["accounting"]["source"], "eval_model_calls");
    assert_eq!(json["accounting"]["eval_model_calls"], 1);
}

#[test]
fn inspect_direct_eval_run_id_without_sidecar_uses_eval_projection() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let eval_run_id = "eval_direct_only";
    let completed_at = rt.block_on(seed_direct_eval_only(&db_path, eval_run_id));

    let json = inspect_to_dir(&db_path, eval_run_id, home.path(), out_dir.path());
    assert_eq!(json["schema_version"], "xvn.agent_run.v3");
    assert_eq!(json["run_id"], eval_run_id);
    assert_eq!(json["eval_run_id"], eval_run_id);
    assert_eq!(json["status"], "completed");
    assert_eq!(json["finished_at"], fmt_ts(completed_at));
    assert_eq!(json["retention_mode"], "hash_only");
    assert_eq!(json["totals"]["input_tokens"], 777);
    assert_eq!(json["totals"]["output_tokens"], 55);
    assert_eq!(json["accounting"]["source"], "eval_actuals");
    assert_eq!(json["accounting"]["eval_mode"], "fwd");
}

#[test]
fn inspect_legacy_agent_db_without_eval_table_still_exports() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let run_id = "legacy_agent_no_eval_table";
    rt.block_on(seed_legacy_agent_only(&db_path, run_id));

    let json = inspect_to_dir(&db_path, run_id, home.path(), out_dir.path());
    assert_eq!(json["schema_version"], "xvn.agent_run.v3");
    assert_eq!(json["run_id"], run_id);
    assert_eq!(json["status"], "completed");
    assert_eq!(json["accounting"]["source"], "none");
}

#[test]
fn inspect_uses_failed_eval_status_when_sidecar_is_running() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let eval_finished = Some(fixed_ts(20));
    rt.block_on(seed_status_precedence_eval(
        &db_path,
        "agent_failed_eval",
        "eval_failed",
        "failed",
        "running",
        eval_finished,
        None,
    ));

    let json = inspect_to_dir(&db_path, "agent_failed_eval", home.path(), out_dir.path());
    assert_eq!(json["status"], "failed");
    assert_eq!(json["finished_at"], fmt_ts(eval_finished.unwrap()));
    assert_eq!(json["accounting"]["eval_status"], "failed");
}

#[test]
fn inspect_uses_cancelled_eval_status_when_sidecar_is_running() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let eval_finished = Some(fixed_ts(21));
    rt.block_on(seed_status_precedence_eval(
        &db_path,
        "agent_cancelled_eval",
        "eval_cancelled",
        "cancelled",
        "running",
        eval_finished,
        None,
    ));

    let json = inspect_to_dir(&db_path, "agent_cancelled_eval", home.path(), out_dir.path());
    assert_eq!(json["status"], "cancelled");
    assert_eq!(json["finished_at"], fmt_ts(eval_finished.unwrap()));
    assert_eq!(json["accounting"]["eval_status"], "cancelled");
}

#[test]
fn inspect_nonterminal_eval_does_not_downgrade_completed_sidecar() {
    let home = tempdir().unwrap();
    let out_dir = tempdir().unwrap();
    let db_path = home.path().join("data").join("store.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let sidecar_finished = Some(fixed_ts(22));
    rt.block_on(seed_status_precedence_eval(
        &db_path,
        "agent_completed_sidecar",
        "eval_still_running",
        "running",
        "completed",
        None,
        sidecar_finished,
    ));

    let json = inspect_to_dir(&db_path, "agent_completed_sidecar", home.path(), out_dir.path());
    assert_eq!(json["status"], "completed");
    assert_eq!(json["finished_at"], fmt_ts(sidecar_finished.unwrap()));
    assert_eq!(json["accounting"]["eval_status"], "running");
}

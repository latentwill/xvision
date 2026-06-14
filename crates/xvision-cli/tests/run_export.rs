//! End-to-end check for `xvn run export-run <id>` — the full-fidelity
//! "flywheel document".
//!
//! Seeds an `agent_runs` row + spans + a variety of `events` rows + a
//! model call with blob-backed prompt/response into an isolated
//! `$XVN_HOME`, then invokes the CLI and asserts the emitted document:
//!   - JSON carries the v3 schema + an `events` array with every kind;
//!   - Markdown contains the actual prompt text (inlined from the blob)
//!     and the event kinds (decision / risk / filter / order).

use std::process::Command;

use sqlx::SqlitePool;
use tempfile::tempdir;

use xvision_observability::BlobStore;

const MIGRATION_002: &str = include_str!("../../xvision-engine/migrations/002_eval.sql");
const MIGRATION_013: &str = include_str!("../../xvision-engine/migrations/013_cli_jobs.sql");
const MIGRATION_018: &str = include_str!("../../xvision-engine/migrations/018_agent_run_observability.sql");

const RUN_ID: &str = "run_export_cli_01";

/// Seed a full_debug run with spans + events + a blob-backed model call.
/// Returns the prompt text written into the blob store.
async fn seed_run(db_path: &std::path::Path, blob_root: &std::path::Path) -> String {
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url).await.expect("open sqlite");
    sqlx::query(MIGRATION_002).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_013).execute(&pool).await.unwrap();
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();

    let store = BlobStore::new(blob_root);
    let prompt = "SYSTEM: full-fidelity flywheel prompt for BTC/USD.";
    let prompt_ref = store.write(prompt.as_bytes()).unwrap();
    let response_ref = store.write(b"DECISION: long, 0.5 size.").unwrap();

    sqlx::query(
        "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
         VALUES (?1, 'flywheel export', 'completed', '2026-06-13T10:00:00Z', 'full_debug')",
    )
    .bind(RUN_ID)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO spans (id, run_id, kind, name, status, started_at) \
         VALUES ('span_root', ?1, 'agent.run', 'agent.run', 'ok', '2026-06-13T10:00:01Z')",
    )
    .bind(RUN_ID)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO spans (id, run_id, parent_span_id, kind, name, status, started_at) \
         VALUES ('span_model', ?1, 'span_root', 'model.call', 'model.call', 'ok', '2026-06-13T10:00:02Z')",
    )
    .bind(RUN_ID)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO model_calls (span_id, provider, model, prompt_hash, \
             prompt_payload_ref, response_payload_ref) \
         VALUES ('span_model', 'anthropic', 'claude', 'sha256:p', ?1, ?2)",
    )
    .bind(prompt_ref.as_str())
    .bind(response_ref.as_str())
    .execute(&pool)
    .await
    .unwrap();

    for (id, span, kind, payload, ts) in [
        (
            "evt_decision",
            Some("span_model"),
            "decision_completed",
            r#"{"decision_index":3,"action":"long"}"#,
            "2026-06-13T10:00:03Z",
        ),
        (
            "evt_risk",
            Some("span_model"),
            "risk_veto",
            r#"{"reason":"exposure_cap"}"#,
            "2026-06-13T10:00:04Z",
        ),
        (
            "evt_filter",
            None,
            "filter_fired",
            r#"{"filter":"min_volume"}"#,
            "2026-06-13T10:00:05Z",
        ),
        (
            "evt_order",
            None,
            "order_state",
            r#"{"state":"filled","broker_order_id":"ORD-9"}"#,
            "2026-06-13T10:00:06Z",
        ),
    ] {
        sqlx::query(
            "INSERT INTO events (id, run_id, span_id, kind, payload_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(id)
        .bind(RUN_ID)
        .bind(span)
        .bind(kind)
        .bind(payload)
        .bind(ts)
        .execute(&pool)
        .await
        .unwrap();
    }

    pool.close().await;
    prompt.to_string()
}

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

#[test]
fn export_run_markdown_is_full_fidelity() {
    let home = tempdir().unwrap();
    let db_path = home.path().join("xvn.db");
    let blob_root = home.path().join("agent_runs").join("blobs");
    std::fs::create_dir_all(&blob_root).unwrap();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let prompt = rt.block_on(seed_run(&db_path, &blob_root));

    let out_path = home.path().join("flywheel.md");
    let out = xvn(
        &[
            "run",
            "export-run",
            RUN_ID,
            "--db",
            db_path.to_str().unwrap(),
            "--format",
            "md",
            "--out",
            out_path.to_str().unwrap(),
        ],
        home.path(),
    );
    assert!(
        out.status.success(),
        "xvn run export-run failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let md = std::fs::read_to_string(&out_path).expect("read markdown");
    assert!(
        md.contains(&prompt),
        "markdown must inline the blob-backed prompt:\n{md}"
    );
    for needle in ["decision_completed", "risk_veto", "filter_fired", "order_state"] {
        assert!(md.contains(needle), "markdown missing `{needle}`:\n{md}");
    }
}

#[test]
fn export_run_json_carries_v3_events_array() {
    let home = tempdir().unwrap();
    let db_path = home.path().join("xvn.db");
    let blob_root = home.path().join("agent_runs").join("blobs");
    std::fs::create_dir_all(&blob_root).unwrap();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(seed_run(&db_path, &blob_root));

    // No `--out` → JSON to stdout.
    let out = xvn(
        &[
            "run",
            "export-run",
            RUN_ID,
            "--db",
            db_path.to_str().unwrap(),
            "--format",
            "json",
        ],
        home.path(),
    );
    assert!(
        out.status.success(),
        "xvn run export-run --format json failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json parse");
    assert_eq!(json["schema_version"], "xvn.agent_run.v3");
    let events = json["events"].as_array().expect("events array");
    let kinds: Vec<&str> = events.iter().filter_map(|e| e["kind"].as_str()).collect();
    for required in ["decision_completed", "risk_veto", "filter_fired", "order_state"] {
        assert!(
            kinds.contains(&required),
            "events missing `{required}`: {kinds:?}"
        );
    }
    // Blob-backed prompt is inlined into the model call.
    assert_eq!(
        json["model_calls"][0]["prompt_text"],
        "SYSTEM: full-fidelity flywheel prompt for BTC/USD."
    );
}

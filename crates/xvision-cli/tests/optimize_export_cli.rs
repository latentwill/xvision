//! End-to-end check for `xvn optimize export <cycle_id>` — the WS-11c
//! optimizer-cycle "flywheel feedback document".
//!
//! Seeds a representative `autooptimizer_events` event sequence for a cycle into
//! an isolated `$XVN_HOME/xvn.db`, then invokes the CLI and asserts the emitted
//! document:
//!   - Markdown carries the OPERATOR-surface labels, the gate outcome
//!     (Active/Suspect/Rejected), the day-Sharpe delta, the nested eval-run id,
//!     and the compiled prompt-pattern summary;
//!   - JSON carries the `xvn.optimizer_cycle.v2` schema + the per-experiment
//!     summary;
//!   - an unknown cycle yields a graceful (non-empty, non-panicking) document.

use std::process::Command;

use sqlx::SqlitePool;
use tempfile::tempdir;

const CYCLE_ID: &str = "optimize_export_cli_cyc1";
const SESSION_ID: &str = "optimize_export_cli_sess1";

/// Seed the `autooptimizer_events` table with a representative cycle sequence:
/// started → parent → proposed → gated(kept) → honesty → judge → flywheel →
/// finished.
async fn seed_cycle(db_path: &std::path::Path) {
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url).await.expect("open sqlite");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS autooptimizer_events (
          seq         INTEGER PRIMARY KEY AUTOINCREMENT,
          session_id  TEXT NOT NULL,
          cycle_id    TEXT,
          kind        TEXT NOT NULL,
          payload_json TEXT NOT NULL,
          ts          TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    // (kind, payload_json) pairs — the full serialized CycleProgressEvent JSON.
    let events: &[(&str, String)] = &[
        (
            "cycle_started",
            format!(
                r#"{{"type":"cycle_started","session_id":"{SESSION_ID}","cycle_id":"{CYCLE_ID}","parent_count":1}}"#
            ),
        ),
        (
            "parent_selected",
            format!(
                r#"{{"type":"parent_selected","session_id":"{SESSION_ID}","cycle_id":"{CYCLE_ID}","parent_hash":"parent-abc"}}"#
            ),
        ),
        (
            "mutation_proposed",
            format!(
                r#"{{"type":"mutation_proposed","session_id":"{SESSION_ID}","cycle_id":"{CYCLE_ID}","parent_hash":"parent-abc","child_hash":"child-xyz","mutator_model":"claude-haiku-4-5"}}"#
            ),
        ),
        (
            "mutation_gated",
            format!(
                r#"{{"type":"mutation_gated","session_id":"{SESSION_ID}","cycle_id":"{CYCLE_ID}","child_hash":"child-xyz","passed":true,"outcome":"kept","delta_day":0.0420,"eval_run_id":"01EVALRUNULID"}}"#
            ),
        ),
        (
            "honesty_check_run",
            format!(
                r#"{{"type":"honesty_check_run","session_id":"{SESSION_ID}","cycle_id":"{CYCLE_ID}","passed":true,"sabotage_variant":"kill-trades","message":"sabotaged variant correctly rejected"}}"#
            ),
        ),
        (
            "judge_finding",
            format!(
                r#"{{"type":"judge_finding","session_id":"{SESSION_ID}","cycle_id":"{CYCLE_ID}","child_hash":"child-xyz","severity":"low","code":"J001"}}"#
            ),
        ),
        (
            "flywheel_compiled",
            format!(
                r#"{{"type":"flywheel_compiled","session_id":"{SESSION_ID}","cycle_id":"{CYCLE_ID}","optimization_run_id":"opt-run-7","pattern_id":"pat-9"}}"#
            ),
        ),
        (
            "cycle_finished",
            format!(
                r#"{{"type":"cycle_finished","session_id":"{SESSION_ID}","cycle_id":"{CYCLE_ID}","active_count":1,"suspect_count":0,"rejected_count":0}}"#
            ),
        ),
    ];

    for (kind, payload) in events {
        sqlx::query(
            "INSERT INTO autooptimizer_events (session_id, cycle_id, kind, payload_json, ts) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(SESSION_ID)
        .bind(CYCLE_ID)
        .bind(kind)
        .bind(payload)
        .bind("2026-06-13T10:00:00Z")
        .execute(&pool)
        .await
        .unwrap();
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
fn export_cycle_markdown_is_full_fidelity_to_file() {
    let home = tempdir().unwrap();
    let db_path = home.path().join("xvn.db");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(seed_cycle(&db_path));

    let out_path = home.path().join("cycle.md");
    let out = xvn(
        &[
            "optimize",
            "export",
            CYCLE_ID,
            "--db",
            db_path.to_str().unwrap(),
            "--format",
            "md",
            "--path",
            out_path.to_str().unwrap(),
        ],
        home.path(),
    );
    assert!(
        out.status.success(),
        "xvn optimize export failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let md = std::fs::read_to_string(&out_path).expect("read markdown");
    assert!(!md.trim().is_empty(), "document must be non-empty");

    // Header + cycle id + schema.
    assert!(md.contains(CYCLE_ID), "missing cycle id:\n{md}");
    assert!(md.contains("xvn.optimizer_cycle.v2"), "missing schema:\n{md}");

    // Operator-surface labels (NOT developer wire names).
    for label in [
        "Optimizer run started",
        "Experiment proposed",
        "Experiment kept",
        "Honesty check result",
        "Reviewer finished notes",
        "Findings compiled into prompt pattern",
        "Optimizer run finished",
    ] {
        assert!(
            md.contains(label),
            "markdown missing operator label `{label}`:\n{md}"
        );
    }

    // Gate outcome (Active), day-Sharpe delta, nested eval-run id, flywheel.
    assert!(md.contains("Active"), "missing Active outcome:\n{md}");
    assert!(md.contains("+0.0420"), "missing day-Sharpe delta:\n{md}");
    assert!(md.contains("01EVALRUNULID"), "missing nested eval_run_id:\n{md}");
    assert!(md.contains("opt-run-7"), "missing flywheel run id:\n{md}");
    assert!(md.contains("pat-9"), "missing pattern id:\n{md}");

    // Developer wire name must NOT leak.
    assert!(!md.contains("mutation_gated"), "wire name leaked:\n{md}");
}

#[test]
fn export_cycle_json_carries_schema_and_experiments() {
    let home = tempdir().unwrap();
    let db_path = home.path().join("xvn.db");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(seed_cycle(&db_path));

    // No --path → document to stdout.
    let out = xvn(
        &[
            "optimize",
            "export",
            CYCLE_ID,
            "--db",
            db_path.to_str().unwrap(),
            "--format",
            "json",
        ],
        home.path(),
    );
    assert!(
        out.status.success(),
        "xvn optimize export --format json failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON on stdout");

    assert_eq!(value["schema_version"], "xvn.optimizer_cycle.v2");
    assert_eq!(value["cycle_id"], CYCLE_ID);
    assert_eq!(value["active_count"], 1);

    let experiments = value["experiments"].as_array().expect("experiments array");
    assert_eq!(experiments.len(), 1, "one experiment in the cycle");
    assert_eq!(experiments[0]["child_hash"], "child-xyz");
    assert_eq!(experiments[0]["outcome"], "Active");
    assert_eq!(experiments[0]["eval_run_id"], "01EVALRUNULID");

    let events = value["events"].as_array().expect("events array");
    assert_eq!(events.len(), 8, "all 8 events present");
}

#[test]
fn export_unknown_cycle_is_graceful() {
    let home = tempdir().unwrap();
    let db_path = home.path().join("xvn.db");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(seed_cycle(&db_path)); // seeds a DIFFERENT cycle

    let out = xvn(
        &[
            "optimize",
            "export",
            "no-such-cycle",
            "--db",
            db_path.to_str().unwrap(),
        ],
        home.path(),
    );
    // Graceful: exits 0, prints a non-empty "no events" document, no panic.
    assert!(
        out.status.success(),
        "unknown cycle must exit 0, got {:?}\nstderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("No events recorded"),
        "unknown cycle must render a graceful empty doc, got:\n{stdout}"
    );
    assert!(stdout.contains("no-such-cycle"));
}

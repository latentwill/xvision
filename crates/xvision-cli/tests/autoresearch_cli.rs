use std::path::Path;
use std::process::{Command, Output};

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::tempdir;

fn xvn(args: &[&str], home: &Path, memory_db: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .env("XVN_MEMORY_DB", memory_db)
        .output()
        .expect("xvn invocation")
}

fn assert_ok(out: &Output) {
    assert!(
        out.status.success(),
        "xvn failed (exit {:?}): stdout={} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn paths() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().expect("tempdir");
    let mem = dir.path().join("memory.db");
    (dir, mem)
}

async fn seed_observation(memory_db: &Path, id: &str, namespace: &str, source_end: &str) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", memory_db.display()))
        .await
        .expect("open memory db");
    sqlx::query(
        "INSERT INTO memory_items \
         (id, namespace, tier, text, embedding, embedding_dim, embedder_id, created_at, \
          run_id, scenario_id, cycle_idx, source_window_start, source_window_end, training_window_end) \
         VALUES (?, ?, 'observation', ?, ?, 0, 'test-embedder', ?, \
                 'run-1', 'scenario-1', 0, '2024-01-01T00:00:00Z', ?, NULL)",
    )
    .bind(id)
    .bind(namespace)
    .bind(format!("observation {id}"))
    .bind(Vec::<u8>::new())
    .bind(source_end)
    .bind(source_end)
    .execute(&pool)
    .await
    .expect("seed observation");
}

#[tokio::test]
async fn autoresearch_run_creates_staged_pattern_and_inspectable_run() {
    let (dir, mem) = paths();
    assert_ok(&xvn(&["memory", "ls", "--json"], dir.path(), &mem));
    seed_observation(&mem, "ar-obs-1", "agent:AR", "2024-01-02T00:00:00Z").await;
    seed_observation(&mem, "ar-obs-2", "agent:AR", "2024-01-05T12:00:00Z").await;

    let out = xvn(
        &[
            "autoresearch",
            "run",
            "--agent",
            "AR",
            "--pattern-text",
            "When this cohort appears, reduce risk.",
            "--embedding-json",
            "[1.0,0.0]",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let run: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse run json");
    assert_eq!(run["namespace"], "agent:AR");
    assert_eq!(run["status"], "completed");
    assert_eq!(run["promotion_state"], "staged");
    assert_eq!(run["observation_ids"].as_array().unwrap().len(), 2);
    let pattern_id = run["pattern_id"].as_str().expect("pattern_id");

    let out = xvn(&["memory", "show", pattern_id, "--json"], dir.path(), &mem);
    assert_ok(&out);
    let pattern: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse pattern json");
    assert_eq!(pattern["tier"], "pattern");
    assert_eq!(pattern["promotion_state"], "staged");
    assert!(
        pattern["training_window_end"]
            .as_str()
            .map(|s| s.starts_with("2024-01-05T12:00:00"))
            .unwrap_or(false),
        "expected latest source_window_end, got {pattern:?}"
    );

    let run_id = run["id"].as_str().expect("run id");
    let out = xvn(&["autoresearch", "inspect", run_id, "--json"], dir.path(), &mem);
    assert_ok(&out);
    let inspected: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse inspected json");
    assert_eq!(inspected["id"], run_id);
    assert_eq!(inspected["pattern_id"], pattern_id);

    let out = xvn(
        &["autoresearch", "ls", "--agent", "AR", "--json"],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let listed: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse listed json");
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["items"][0]["id"], run_id);

    let out = xvn(
        &[
            "autoresearch",
            "gate",
            run_id,
            "--metric",
            "sharpe_delta",
            "--parent-day-score",
            "0.7",
            "--child-day-score",
            "0.9",
            "--parent-holdout-score",
            "1.0",
            "--child-holdout-score",
            "1.25",
            "--gate-epsilon",
            "0.1",
            "--finding-text",
            "Blind Finding: coherent risk reduction.",
            "--qualitative-finding-json",
            "{\"summary\":\"coherent risk reduction\"}",
            "--judge-model",
            "cli-test-judge",
            "--judge-token-cost",
            "21",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let gated: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse gated json");
    assert_eq!(gated["gate_passed"], true);
    assert_eq!(gated["gate_verdict"], "passed");
    assert!((gated["delta_day"].as_f64().unwrap() - 0.2).abs() < 1e-9);
    assert!((gated["delta_holdout"].as_f64().unwrap() - 0.25).abs() < 1e-9);
    assert_eq!(gated["finding_blind"], true);
    assert_eq!(gated["finding_blinded_metrics"], true);
    assert_eq!(gated["judge_model"], "cli-test-judge");
    assert_eq!(gated["judge_token_cost"], 21);
    assert_eq!(gated["promotion_state"], "staged");

    let out = xvn(&["autoresearch", "promote", run_id, "--json"], dir.path(), &mem);
    assert_ok(&out);
    let promoted: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse promoted json");
    assert_eq!(promoted["promotion_state"], "active");

    let out = xvn(&["autoresearch", "demote", run_id, "--json"], dir.path(), &mem);
    assert_ok(&out);
    let demoted: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse demoted json");
    assert_eq!(demoted["promotion_state"], "demoted");

    let out = xvn(&["memory", "show", pattern_id, "--json"], dir.path(), &mem);
    assert_ok(&out);
    let demoted_pattern: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse demoted pattern json");
    assert!(
        demoted_pattern["forgotten_at"].as_str().is_some(),
        "demotion should soft-delete the pattern, got {demoted_pattern:?}"
    );

    let out = xvn(
        &["flywheel", "status", "--agent", "AR", "--json"],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let status: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse status json");
    assert_eq!(status["namespace"], "agent:AR");
    assert_eq!(status["observations"], 2);
    assert_eq!(status["staged_patterns"], 0);
    assert_eq!(status["forgotten_patterns"], 1);
    assert_eq!(status["autoresearch_runs"], 1);
    assert_eq!(status["latest_autoresearch_run_id"], run_id);

    let out = xvn(
        &["flywheel", "velocity", "--agent", "AR", "--days", "7", "--json"],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let velocity: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse velocity json");
    assert_eq!(velocity["namespace"], "agent:AR");
    assert_eq!(velocity["autoresearch_runs"], 1);
    assert_eq!(velocity["patterns_demoted"], 1);
}

#[tokio::test]
async fn autoresearch_run_rejects_one_observation_cohort() {
    let (dir, mem) = paths();
    assert_ok(&xvn(&["memory", "ls", "--json"], dir.path(), &mem));
    seed_observation(&mem, "ar-single-obs", "global", "2024-01-02T00:00:00Z").await;

    let out = xvn(
        &[
            "autoresearch",
            "run",
            "--namespace",
            "global",
            "--pattern-text",
            "one row should fail",
            "--embedding-json",
            "[1.0]",
        ],
        dir.path(),
        &mem,
    );
    assert!(!out.status.success(), "single-observation autoresearch must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not enough Observations"),
        "expected cohort-size error, got: {stderr}"
    );
}

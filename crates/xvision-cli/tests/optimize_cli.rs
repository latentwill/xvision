//! `xvn optimize *` end-to-end CLI integration tests (Phase 3.6).
//!
//! Drives the real `xvn` binary so the clap surface, `--json`-stdout
//! discipline, the distinct failure-class exit codes, and the
//! optimization-store persistence are exercised the way an operator hits them.
//!
//! NO NETWORK: tests that do not set up a real agent in the DB pass `--test-model`
//! to use the dummy/dummy identity instead of resolving from the agent store.
//! Each test points `XVN_HOME` at a fresh tempdir so the store is created +
//! migrated in isolation.

use std::path::Path;
use std::process::{Command, Output};

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::tempdir;

fn xvn(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        // Keep the memory recorder/embedder from reaching out; non-fatal anyway.
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

/// Write a small valid corpus file (2 trader exemplars) and return its path.
fn write_corpus(home: &Path) -> String {
    let path = home.join("corpus.json");
    std::fs::write(
        &path,
        r#"[
          {"inputs":{"briefing":"BTC breaking out on volume","position_context":"flat, 2% budget"},
           "outputs":{"action":"buy","size_fraction":0.5,"rationale":"momentum"}},
          {"inputs":{"briefing":"choppy range, low conviction","position_context":"flat, 2% budget"},
           "outputs":{"action":"hold","size_fraction":0.0,"rationale":"no edge"}}
        ]"#,
    )
    .unwrap();
    path.to_str().unwrap().to_string()
}

// --- success: dry-run validates without mutating ---------------------------

#[test]
fn dry_run_validates_without_persisting() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    let out = xvn(
        &[
            "optimize",
            "run",
            "--agent",
            "01AGENT",
            "--slot",
            "trader",
            "--capability",
            "trader",
            "--corpus",
            &corpus,
            "--optimizer",
            "mipro",
            "--metric",
            "delta_sharpe",
            "--rng-seed",
            "42",
            "--dry-run",
            "--test-model",
            "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout JSON");
    assert_eq!(json["mode"], "dry-run");
    assert_eq!(json["valid"], true);
    assert_eq!(json["corpus_demo_count"], 2);
    assert!(json["signature_hash"].as_str().unwrap().len() == 64);
    assert_eq!(json["model_provider"], "dummy");
}

// --- success: a full run persists a run + candidates + snapshot -------------

#[test]
fn run_persists_and_is_inspectable_deterministically() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());

    let out = xvn(
        &[
            "optimize",
            "run",
            "--agent",
            "01AGENT",
            "--slot",
            "trader",
            "--capability",
            "trader",
            "--corpus",
            &corpus,
            "--optimizer",
            "copro",
            "--metric",
            "delta_sharpe",
            "--rng-seed",
            "7",
            "--max-rounds",
            "3",
            "--test-model",
            "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let run_json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("run JSON");
    let run_id = run_json["run_id"].as_str().unwrap().to_string();
    let snapshot_id = run_json["snapshot_id"].as_str().unwrap().to_string();
    assert_eq!(run_json["candidate_count"], 3);
    assert_eq!(run_json["status"], "completed");
    assert_eq!(run_json["model_provider"], "dummy");
    let demo_set = run_json["demo_set"].as_str().unwrap().to_string();
    assert_eq!(demo_set.len(), 64, "demo_set is a sha256 hex");

    // inspect round-trips the persisted run + recipe + candidates + snapshot.
    let out = xvn(&["optimize", "inspect", "--run", &run_id, "--json"], home.path());
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let insp: serde_json::Value = serde_json::from_slice(&out.stdout).expect("inspect JSON");
    assert_eq!(insp["run"]["id"], run_id);
    assert_eq!(insp["candidates"].as_array().unwrap().len(), 3);
    assert_eq!(insp["snapshots"].as_array().unwrap().len(), 1);
    // exactly one candidate is marked the selected winner.
    let selected: Vec<_> = insp["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["selected"].as_bool().unwrap())
        .collect();
    assert_eq!(selected.len(), 1, "exactly one winner must be selected");
    assert_eq!(
        selected[0]["candidate_index"].as_i64().unwrap(),
        run_json["selected_candidate_index"].as_i64().unwrap()
    );
    // reproduction recipe carries the inputs needed to re-derive the run.
    assert_eq!(insp["reproduction_recipe"]["rng_seed"], 7);
    assert_eq!(insp["reproduction_recipe"]["corpus_query"], corpus);
    assert_eq!(insp["reproduction_recipe"]["optimizer"], "copro");

    // export the snapshot's demos; re-importing the same payload is a no-op and
    // yields the SAME content hash (content-addressed determinism).
    let out = xvn(
        &["optimize", "export-demos", "--snapshot", &snapshot_id],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let exported = String::from_utf8(out.stdout).unwrap();
    let exported_path = home.path().join("exported.json");
    std::fs::write(&exported_path, exported.trim()).unwrap();
    let out = xvn(
        &[
            "optimize",
            "import-demos",
            "--file",
            exported_path.to_str().unwrap(),
            "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let imp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        imp["demo_set"], demo_set,
        "re-import is content-addressed identical"
    );

    // accept the snapshot as a child agent → lineage edge; then revert.
    let out = xvn(
        &[
            "optimize",
            "accept-as-child-agent",
            "--snapshot",
            &snapshot_id,
            "--child-agent",
            "01CHILD",
            "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let acc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(acc["accepted"], true);
    assert_eq!(acc["child_agent_id"], "01CHILD");
    assert_eq!(acc["parent_agent_id"], "01AGENT");

    let out = xvn(
        &[
            "optimize",
            "revert-accepted",
            "--snapshot",
            &snapshot_id,
            "--child-agent",
            "01CHILD",
            "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

/// Determinism: same seed + same corpus ⇒ same selected candidate index +
/// snapshot instruction (modulo the random snapshot id/run id).
#[test]
fn same_seed_yields_same_winner() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    let run_once = || {
        let out = xvn(
            &[
                "optimize",
                "run",
                "--agent",
                "01AGENT",
                "--slot",
                "trader",
                "--capability",
                "trader",
                "--corpus",
                &corpus,
                "--optimizer",
                "mipro",
                "--metric",
                "delta_sharpe",
                "--rng-seed",
                "99",
                "--max-rounds",
                "5",
                "--test-model",
                "--json",
            ],
            home.path(),
        );
        assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["selected_candidate_index"].as_i64().unwrap()
    };
    assert_eq!(
        run_once(),
        run_once(),
        "winner must be deterministic for a fixed seed"
    );
}

// --- failure-class exit codes ----------------------------------------------

#[test]
fn missing_data_exit_10() {
    let home = tempdir().unwrap();
    // a query string that is not a file ⇒ 0 rows ⇒ missing data on a real run.
    let out = xvn(
        &[
            "optimize",
            "run",
            "--agent",
            "01AGENT",
            "--slot",
            "trader",
            "--capability",
            "trader",
            "--corpus",
            "scenario:does-not-exist",
            "--optimizer",
            "mipro",
            "--metric",
            "delta_sharpe",
            "--rng-seed",
            "1",
            "--test-model",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 10, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn missing_capability_exit_11() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    // decision_grader is a declared stub → typed missing_capability_optimizer.
    let out = xvn(
        &[
            "optimize",
            "run",
            "--agent",
            "01AGENT",
            "--slot",
            "grader",
            "--capability",
            "decision_grader",
            "--corpus",
            &corpus,
            "--optimizer",
            "mipro",
            "--metric",
            "grader_score",
            "--rng-seed",
            "1",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 11, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no optimizer for capability"),
        "stderr should carry the typed missing-capability message"
    );
}

#[test]
fn missing_agent_exits_not_found() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    // Agent does not exist in the store → not-found (exit 4).
    let out = xvn(
        &[
            "optimize",
            "run",
            "--agent",
            "01AGENT-DOES-NOT-EXIST",
            "--slot",
            "trader",
            "--capability",
            "trader",
            "--corpus",
            &corpus,
            "--optimizer",
            "mipro",
            "--metric",
            "delta_sharpe",
            "--rng-seed",
            "1",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 4, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not found"),
        "stderr should mention not found"
    );
}

#[test]
fn metric_failure_exit_13() {
    let home = tempdir().unwrap();
    let corpus = write_corpus(home.path());
    let out = xvn(
        &[
            "optimize",
            "run",
            "--agent",
            "01AGENT",
            "--slot",
            "trader",
            "--capability",
            "trader",
            "--corpus",
            &corpus,
            "--optimizer",
            "mipro",
            "--metric",
            "not_a_real_metric",
            "--rng-seed",
            "1",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 13, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn validation_failure_exit_14() {
    let home = tempdir().unwrap();
    // corpus path that exists but is malformed JSON ⇒ validation failure.
    let bad = home.path().join("bad.json");
    std::fs::write(&bad, "{ not an array }").unwrap();
    let out = xvn(
        &[
            "optimize",
            "run",
            "--agent",
            "01AGENT",
            "--slot",
            "trader",
            "--capability",
            "trader",
            "--corpus",
            bad.to_str().unwrap(),
            "--optimizer",
            "mipro",
            "--metric",
            "delta_sharpe",
            "--rng-seed",
            "1",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 14, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn persistence_failure_exit_15() {
    // Point XVN_HOME at an existing regular FILE: ApiContext::open's
    // create_dir_all + DB open fails, mapping to a persistence failure.
    let dir = tempdir().unwrap();
    let file_home = dir.path().join("not-a-dir");
    std::fs::write(&file_home, "i am a file").unwrap();
    let corpus = dir.path().join("corpus.json");
    std::fs::write(
        &corpus,
        r#"[{"inputs":{"briefing":"x","position_context":"y"},"outputs":{"action":"hold","size_fraction":0.0,"rationale":"z"}}]"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args([
            "optimize",
            "run",
            "--agent",
            "01AGENT",
            "--slot",
            "trader",
            "--capability",
            "trader",
            "--corpus",
            corpus.to_str().unwrap(),
            "--optimizer",
            "mipro",
            "--metric",
            "delta_sharpe",
            "--rng-seed",
            "1",
            "--test-model",
        ])
        .env("XVN_HOME", &file_home)
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("xvn invocation");
    assert_eq!(code(&out), 15, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn inspect_missing_run_is_not_found_exit_4() {
    let home = tempdir().unwrap();
    let out = xvn(&["optimize", "inspect", "--run", "01NOPE", "--json"], home.path());
    assert_eq!(code(&out), 4, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn explain_missing_data_query_guidance() {
    let home = tempdir().unwrap();
    let out = xvn(
        &[
            "optimize",
            "explain-missing-data",
            "--corpus",
            "scenario:nope",
            "--json",
        ],
        home.path(),
    );
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["resolved_as"], "query");
    assert_eq!(v["demo_count"], 0);
    assert!(v["remediation"].as_str().unwrap().contains("JSON file"));
}

fn xvn_with_memory(args: &[&str], home: &Path, memory_db: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .env("XVN_MEMORY_DB", memory_db)
        .env_remove("OPENAI_API_KEY")
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

async fn seed_pattern(memory_db: &Path, id: &str, namespace: &str) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", memory_db.display()))
        .await
        .expect("open memory db");
    sqlx::query(
        "INSERT INTO memory_items \
         (id, namespace, tier, text, embedding, embedding_dim, embedder_id, created_at, \
          training_window_end, promotion_state) \
         VALUES (?, ?, 'pattern', 'Prior Pattern: reduce size.', ?, 0, 'test-embedder', \
                 '2024-01-05T00:00:00Z', '2024-01-05T00:00:00Z', 'active')",
    )
    .bind(id)
    .bind(namespace)
    .bind(Vec::<u8>::new())
    .execute(&pool)
    .await
    .expect("seed pattern");
}

async fn seed_autooptimizer_run(memory_db: &Path, id: &str, namespace: &str, pattern_id: &str) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", memory_db.display()))
        .await
        .expect("open memory db");
    sqlx::query(
        "INSERT INTO autooptimizer_runs \
         (id, namespace, observation_ids_json, pattern_id, pattern_text, promotion_state, \
          min_observations, created_at, status, error) \
         VALUES (?, ?, ?, ?, 'Demo-source Pattern', 'active', 2, \
                 '2024-01-05T00:00:00Z', 'staged', NULL)",
    )
    .bind(id)
    .bind(namespace)
    .bind(serde_json::json!(["opt-cli-obs-3"]).to_string())
    .bind(pattern_id)
    .execute(&pool)
    .await
    .expect("seed autooptimizer run");
}

async fn seed_memory_recall_event(
    home: &Path,
    id: &str,
    namespace: &str,
    pattern_id: &str,
    created_at: &str,
) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", home.join("xvn.db").display()))
        .await
        .expect("open xvn db");
    let payload = serde_json::json!({
        "run_id": "run-cli-auto-prior",
        "flywheel_cycle_id": "run-cli-auto-prior:1",
        "decision_id": 1,
        "namespace": namespace,
        "items": [{
            "id": pattern_id,
            "score": 0.9,
            "text_preview": "auto prior"
        }]
    })
    .to_string();
    let mut conn = pool.acquire().await.expect("acquire xvn db connection");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .expect("disable fk for event fixture");
    sqlx::query(
        "INSERT INTO events (id, run_id, span_id, kind, payload_json, created_at) \
         VALUES (?, 'run-cli-auto-prior', NULL, 'memory_recall', ?, ?)",
    )
    .bind(id)
    .bind(payload)
    .bind(created_at)
    .execute(&mut *conn)
    .await
    .expect("seed memory recall event");
}

#[tokio::test]
async fn optimize_memory_demos_mints_child_agent_prompt() {
    let dir = tempdir().expect("tempdir");
    let mem = dir.path().join("memory.db");
    assert_ok(&xvn_with_memory(&["memory", "ls", "--json"], dir.path(), &mem));

    let out = xvn_with_memory(
        &[
            "agent",
            "create",
            "--name",
            "opt-target",
            "--tools",
            "market_data",
            "--provider",
            "mock",
            "--model",
            "mock",
            "--system-prompt",
            "base prompt",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let agent: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse agent");
    let agent_id = agent["agent_id"].as_str().expect("agent id");
    let namespace = format!("agent:{agent_id}");
    seed_observation(&mem, "opt-cli-obs-1", &namespace, "2024-01-02T00:00:00Z").await;
    seed_observation(&mem, "opt-cli-obs-2", &namespace, "2024-01-03T00:00:00Z").await;
    seed_observation(&mem, "opt-cli-obs-3", &namespace, "2024-01-04T00:00:00Z").await;
    seed_pattern(&mem, "opt-cli-prior-1", &namespace).await;
    seed_pattern(&mem, "opt-cli-auto-prior-1", &namespace).await;
    seed_pattern(&mem, "opt-cli-demo-pattern-1", &namespace).await;
    seed_autooptimizer_run(&mem, "opt-cli-run-1", &namespace, "opt-cli-demo-pattern-1").await;
    seed_memory_recall_event(
        dir.path(),
        "opt-cli-auto-prior-event-1",
        &namespace,
        "opt-cli-auto-prior-1",
        "2024-01-06T00:00:00Z",
    )
    .await;

    let out = xvn_with_memory(
        &[
            "optimize",
            "memory-demos",
            "--agent",
            agent_id,
            "--demo-source",
            "frozen-snapshot",
            "--holdout-split",
            "70/15/15",
            "--cohort-query",
            "scenario_id=scenario-1",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let plan: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse dry run");
    assert_eq!(plan["status"], "planned");
    assert_eq!(plan["demo_source"], "frozen-snapshot");
    assert_eq!(plan["holdout_split"], "70/15/15");
    assert_eq!(plan["demo_count"], 1);
    assert_eq!(
        plan["demo_source_pattern_ids"],
        serde_json::json!(["opt-cli-demo-pattern-1"])
    );
    assert_eq!(plan["pattern_demo_source_count"], 1);
    assert_eq!(
        plan["train_observation_ids"],
        serde_json::json!(["opt-cli-obs-3"])
    );
    assert_eq!(plan["dev_observation_ids"], serde_json::json!(["opt-cli-obs-2"]));
    assert_eq!(
        plan["holdout_observation_ids"],
        serde_json::json!(["opt-cli-obs-1"])
    );
    assert!(plan["train_hash"].as_str().unwrap().starts_with("sha256:"));
    assert!(plan["child_agent_id"].is_null());

    let out = xvn_with_memory(
        &[
            "optimize",
            "memory-demos",
            "--agent",
            agent_id,
            "--child-name",
            "opt-target child",
            "--prior-pattern",
            "opt-cli-prior-1",
            "--auto-priors",
            "--prior-limit",
            "1",
            "--yes",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let minted: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse minted");
    assert_eq!(minted["status"], "minted");
    assert!(minted["optimization_id"].as_str().is_some());
    assert_eq!(
        minted["prior_pattern_ids"],
        serde_json::json!(["opt-cli-prior-1", "opt-cli-auto-prior-1"])
    );
    assert_eq!(minted["pattern_prior_count"], 2);
    assert_eq!(
        minted["demo_source_pattern_ids"],
        serde_json::json!(["opt-cli-demo-pattern-1"])
    );
    assert_eq!(minted["pattern_demo_source_count"], 1);
    let child_id = minted["child_agent_id"].as_str().expect("child id");
    assert_eq!(minted["demo_count"], 1);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", dir.path().join("xvn.db").display()))
        .await
        .expect("open xvn db");
    let lineage_child_id: String =
        sqlx::query_scalar("SELECT child_agent_id FROM agent_slot_optimizations WHERE optimization_id = ?")
            .bind(minted["optimization_id"].as_str().unwrap())
            .fetch_one(&pool)
            .await
            .expect("read optimization lineage");
    assert_eq!(lineage_child_id, child_id);
    let linked_demo_source: String = sqlx::query_scalar(
        "SELECT pattern_id FROM pattern_optimizations WHERE optimization_id = ? AND role = 'demo_source'",
    )
    .bind(minted["optimization_id"].as_str().unwrap())
    .fetch_one(&pool)
    .await
    .expect("read demo-source pattern optimization lineage");
    assert_eq!(linked_demo_source, "opt-cli-demo-pattern-1");
    let linked_priors: Vec<(String,)> = sqlx::query_as(
        "SELECT pattern_id FROM pattern_optimizations WHERE optimization_id = ? AND role = 'prior' \
         ORDER BY pattern_id ASC",
    )
    .bind(minted["optimization_id"].as_str().unwrap())
    .fetch_all(&pool)
    .await
    .expect("read pattern optimization lineage");
    assert_eq!(
        linked_priors.into_iter().map(|row| row.0).collect::<Vec<_>>(),
        vec!["opt-cli-auto-prior-1", "opt-cli-prior-1"]
    );

    let out = xvn_with_memory(
        &[
            "optimize",
            "memory-demos-gate",
            minted["optimization_id"].as_str().unwrap(),
            "--dev-metric",
            "sharpe_delta",
            "--parent-dev-score",
            "0.7",
            "--child-dev-score",
            "0.95",
            "--baseline-untouched-score",
            "1.0",
            "--candidate-untouched-score",
            "1.2",
            "--min-improvement",
            "0.1",
            "--reason",
            "child beat parent on dev and holdout",
            "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let gate: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse gate");
    assert_eq!(gate["gate_verdict"], "passed");
    assert!((gate["delta_dev"].as_f64().unwrap() - 0.25).abs() < 1e-9);
    assert!((gate["delta_holdout"].as_f64().unwrap() - 0.2).abs() < 1e-9);

    let out = xvn_with_memory(
        &[
            "flywheel", "lineage", "--agent", agent_id, "--limit", "5", "--json",
        ],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let lineage: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse lineage");
    assert_eq!(lineage["namespace"], namespace);
    assert_eq!(lineage["total"], 1);
    assert_eq!(lineage["items"][0]["optimization_id"], minted["optimization_id"]);
    assert_eq!(lineage["items"][0]["train_hash"], minted["train_hash"]);
    assert_eq!(lineage["items"][0]["dev_hash"], minted["dev_hash"]);
    assert_eq!(lineage["items"][0]["holdout_hash"], minted["holdout_hash"]);
    assert_eq!(
        lineage["items"][0]["demo_source_pattern_ids"],
        serde_json::json!(["opt-cli-demo-pattern-1"])
    );
    assert_eq!(
        lineage["items"][0]["prior_pattern_ids"],
        serde_json::json!(["opt-cli-auto-prior-1", "opt-cli-prior-1"])
    );
    assert_eq!(lineage["items"][0]["gate_verdict"], "passed");
    assert!((lineage["items"][0]["delta_holdout"].as_f64().unwrap() - 0.2).abs() < 1e-9);
    let out = xvn_with_memory(
        &["flywheel", "lineage", "--agent", agent_id, "--limit", "5"],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let lineage_text = String::from_utf8_lossy(&out.stdout);
    assert!(lineage_text.contains("gate decision: Kept"));
    assert!(lineage_text.contains("validation improvement: 0.250000"));
    assert!(lineage_text.contains("untouched improvement: 0.200000"));

    let out = xvn_with_memory(&["agent", "get", child_id], dir.path(), &mem);
    assert_ok(&out);
    let child: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse child");
    let prompt = child["slots"][0]["system_prompt"].as_str().expect("child prompt");
    assert!(prompt.starts_with("<pattern_priors"));
    assert!(prompt.contains("Prior Pattern: reduce size."));
    assert!(prompt.contains("<memory_demos"));
    assert!(prompt.contains("opt-cli-obs-3"));
    assert!(!prompt.contains("opt-cli-obs-2"));
    assert!(!prompt.contains("opt-cli-obs-1"));
    assert!(prompt.ends_with("base prompt"));
}

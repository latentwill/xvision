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

async fn seed_autoresearch_run(memory_db: &Path, id: &str, namespace: &str, pattern_id: &str) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", memory_db.display()))
        .await
        .expect("open memory db");
    sqlx::query(
        "INSERT INTO autoresearch_runs \
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
    .expect("seed autoresearch run");
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
    assert_ok(&xvn(&["memory", "ls", "--json"], dir.path(), &mem));

    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            "opt-target",
            "--capability",
            "trader",
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
    seed_autoresearch_run(&mem, "opt-cli-run-1", &namespace, "opt-cli-demo-pattern-1").await;
    seed_memory_recall_event(
        dir.path(),
        "opt-cli-auto-prior-event-1",
        &namespace,
        "opt-cli-auto-prior-1",
        "2024-01-06T00:00:00Z",
    )
    .await;

    let out = xvn(
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

    let out = xvn(
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

    let out = xvn(
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
            "--parent-holdout-score",
            "1.0",
            "--child-holdout-score",
            "1.2",
            "--gate-epsilon",
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

    let out = xvn(
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
    let out = xvn(
        &["flywheel", "lineage", "--agent", agent_id, "--limit", "5"],
        dir.path(),
        &mem,
    );
    assert_ok(&out);
    let lineage_text = String::from_utf8_lossy(&out.stdout);
    assert!(lineage_text.contains("gate verdict=passed"));
    assert!(lineage_text.contains("delta_dev=0.250000"));
    assert!(lineage_text.contains("delta_holdout=0.200000"));

    let out = xvn(&["agent", "get", child_id], dir.path(), &mem);
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

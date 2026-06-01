//! Dashboard API coverage for the memory flywheel surfaces.

mod support;

use axum_test::TestServer;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::agents::Agent;
use xvision_engine::api::agents::{self, CreateAgentRequest};

struct EnvGuard {
    key: &'static str,
    prior: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &std::path::Path) -> Self {
        let prior = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, prior }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

async fn server_with_memory_db() -> (TestServer, TempDir, AppState, EnvGuard, std::path::PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let memory_db = tmp.path().join("memory.db");
    let guard = EnvGuard::set("XVN_MEMORY_DB", &memory_db);
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    state
        .run_dashboard_migrations()
        .await
        .expect("dashboard migrations");
    let server = TestServer::new(build_router(state.clone())).expect("test server");
    (server, tmp, state, guard, memory_db)
}

async fn seed_observation(memory_db: &std::path::Path, id: &str, namespace: &str, source_end: &str) {
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

async fn seed_pattern(memory_db: &std::path::Path, id: &str, namespace: &str) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", memory_db.display()))
        .await
        .expect("open memory db");
    sqlx::query(
        "INSERT INTO memory_items \
         (id, namespace, tier, text, embedding, embedding_dim, embedder_id, created_at, \
          training_window_end, promotion_state) \
         VALUES (?, ?, 'pattern', 'Prior Pattern: dashboard risk trim.', ?, 0, 'test-embedder', \
                 '2024-01-05T00:00:00Z', '2024-01-05T00:00:00Z', 'active')",
    )
    .bind(id)
    .bind(namespace)
    .bind(Vec::<u8>::new())
    .execute(&pool)
    .await
    .expect("seed pattern");
}

async fn seed_autooptimizer_run(memory_db: &std::path::Path, id: &str, namespace: &str, pattern_id: &str) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", memory_db.display()))
        .await
        .expect("open memory db");
    sqlx::query(
        "INSERT INTO autooptimizer_runs \
         (id, namespace, observation_ids_json, pattern_id, pattern_text, promotion_state, \
          min_observations, created_at, status, error) \
         VALUES (?, ?, ?, ?, 'Dashboard demo-source Pattern', 'active', 2, \
                 '2024-01-05T00:00:00Z', 'staged', NULL)",
    )
    .bind(id)
    .bind(namespace)
    .bind(serde_json::json!(["dash-obs-3"]).to_string())
    .bind(pattern_id)
    .execute(&pool)
    .await
    .expect("seed autooptimizer run");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flywheel_routes_cover_status_autooptimizer_and_memory_demo_optimize() {
    let (server, _tmp, state, _guard, memory_db) = server_with_memory_db().await;
    let mut slot = Agent::single_slot_default("unused", "target", "mock", "mock")
        .slots
        .remove(0);
    slot.system_prompt = "base prompt".into();
    let agent = agents::create(
        &state.api_context(),
        CreateAgentRequest {
            name: "dashboard flywheel target".into(),
            description: String::new(),
            tags: Vec::new(),
            slots: vec![slot],
            scope_strategy_id: None,
        },
    )
    .await
    .expect("create agent");
    let namespace = format!("agent:{}", agent.agent_id);
    seed_observation(&memory_db, "dash-obs-1", &namespace, "2024-01-02T00:00:00Z").await;
    seed_observation(&memory_db, "dash-obs-2", &namespace, "2024-01-03T00:00:00Z").await;
    seed_observation(&memory_db, "dash-obs-3", &namespace, "2024-01-04T00:00:00Z").await;
    seed_pattern(&memory_db, "dash-prior-1", &namespace).await;

    let namespaces = server.get("/api/memory/namespaces").await;
    namespaces.assert_status_ok();
    let namespaces_body: serde_json::Value = namespaces.json();
    assert_eq!(namespaces_body["total"], 1);
    assert_eq!(namespaces_body["items"][0]["namespace"], namespace);
    assert_eq!(namespaces_body["items"][0]["observations"], 3);
    assert_eq!(namespaces_body["items"][0]["active_patterns"], 1);
    assert_eq!(namespaces_body["items"][0]["live_total"], 4);

    let status = server
        .get(&format!("/api/flywheel/status?agent={}", agent.agent_id))
        .await;
    status.assert_status_ok();
    let body: serde_json::Value = status.json();
    assert_eq!(body["namespace"], namespace);
    assert_eq!(body["observations"], 3);

    let run = server
        .post("/api/autooptimizer/run")
        .json(&serde_json::json!({
            "agent": agent.agent_id,
            "pattern_text": "When this dashboard cohort appears, reduce risk.",
            "embedding": [1.0, 0.0],
            "embedder_id": "dashboard-test",
            "min_observations": 2
        }))
        .await;
    run.assert_status_ok();
    let run_body: serde_json::Value = run.json();
    assert_eq!(run_body["namespace"], namespace);
    assert_eq!(run_body["promotion_state"], "staged");
    let run_id = run_body["id"].as_str().expect("run id");
    let pattern_id = run_body["pattern_id"].as_str().expect("pattern id");

    let staged_list = server
        .get(&format!(
            "/api/memory?namespace={namespace}&tier=pattern&promotion_state=staged"
        ))
        .await;
    staged_list.assert_status_ok();
    let staged_body: serde_json::Value = staged_list.json();
    assert_eq!(staged_body["total"], 1);
    assert_eq!(staged_body["items"][0]["id"], pattern_id);

    let activated_pattern = server.post(&format!("/api/memory/{pattern_id}/activate")).await;
    activated_pattern.assert_status_ok();
    let activated_pattern_body: serde_json::Value = activated_pattern.json();
    assert_eq!(activated_pattern_body["promotion_state"], "active");

    let inspected = server.get(&format!("/api/autooptimizer/{run_id}")).await;
    inspected.assert_status_ok();
    let inspected_body: serde_json::Value = inspected.json();
    assert_eq!(inspected_body["id"], run_id);

    let listed = server
        .get(&format!("/api/autooptimizer?agent={}", agent.agent_id))
        .await;
    listed.assert_status_ok();
    let listed_body: serde_json::Value = listed.json();
    assert_eq!(listed_body["total"], 1);
    assert_eq!(listed_body["items"][0]["id"], run_id);

    let gated = server
        .post(&format!("/api/autooptimizer/{run_id}/gate"))
        .json(&serde_json::json!({
            "metric": "sharpe_delta",
            "parent_day_score": 0.7,
            "child_day_score": 0.9,
            "parent_holdout_score": 1.0,
            "child_holdout_score": 1.2,
            "gate_epsilon": 0.1,
            "finding_text": "Blind Finding: dashboard cohort is coherent.",
            "qualitative_finding_json": "{\"summary\":\"dashboard cohort is coherent\"}",
            "judge_model": "dashboard-test-judge",
            "judge_token_cost": 18,
            "promote_if_pass": false
        }))
        .await;
    gated.assert_status_ok();
    let gated_body: serde_json::Value = gated.json();
    assert_eq!(gated_body["gate_passed"], true);
    assert_eq!(gated_body["gate_verdict"], "passed");
    assert!((gated_body["delta_day"].as_f64().unwrap() - 0.2).abs() < 1e-9);
    assert!((gated_body["delta_holdout"].as_f64().unwrap() - 0.2).abs() < 1e-9);
    assert_eq!(gated_body["finding_blind"], true);
    assert_eq!(gated_body["finding_blinded_metrics"], true);
    assert_eq!(gated_body["judge_model"], "dashboard-test-judge");

    let promoted = server.post(&format!("/api/autooptimizer/{run_id}/promote")).await;
    promoted.assert_status_ok();
    let promoted_body: serde_json::Value = promoted.json();
    assert_eq!(promoted_body["promotion_state"], "active");

    let demoted = server.post(&format!("/api/autooptimizer/{run_id}/demote")).await;
    demoted.assert_status_ok();
    let demoted_body: serde_json::Value = demoted.json();
    assert_eq!(demoted_body["promotion_state"], "demoted");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", memory_db.display()))
        .await
        .expect("open memory db");
    let forgotten_at: Option<String> =
        sqlx::query_scalar("SELECT forgotten_at FROM memory_items WHERE id = ?")
            .bind(pattern_id)
            .fetch_one(&pool)
            .await
            .expect("read forgotten_at");
    assert!(forgotten_at.is_some());

    let forgotten_list = server
        .get(&format!(
            "/api/memory?namespace={namespace}&tier=pattern&forgotten_only=true"
        ))
        .await;
    forgotten_list.assert_status_ok();
    let forgotten_body: serde_json::Value = forgotten_list.json();
    assert_eq!(forgotten_body["total"], 1);

    seed_pattern(&memory_db, "dash-demo-pattern-1", &namespace).await;
    seed_autooptimizer_run(&memory_db, "dash-demo-run-1", &namespace, "dash-demo-pattern-1").await;

    let optimized = server
        .post("/api/optimize/memory-demos")
        .json(&serde_json::json!({
            "target_agent_id": agent.agent_id,
            "demo_source": "frozen-snapshot",
            "holdout_split": "70/15/15",
            "cohort_query": "scenario_id=scenario-1",
            "prior_pattern_ids": ["dash-prior-1"],
            "apply": true,
            "child_name": "dashboard flywheel child"
        }))
        .await;
    optimized.assert_status_ok();
    let optimized_body: serde_json::Value = optimized.json();
    assert_eq!(optimized_body["status"], "minted");
    let optimization_id = optimized_body["optimization_id"]
        .as_str()
        .expect("optimization id");
    assert_eq!(optimized_body["demo_source"], "frozen-snapshot");
    assert_eq!(optimized_body["holdout_split"], "70/15/15");
    assert_eq!(
        optimized_body["prior_pattern_ids"],
        serde_json::json!(["dash-prior-1"])
    );
    assert_eq!(optimized_body["pattern_prior_count"], 1);
    assert_eq!(
        optimized_body["demo_source_pattern_ids"],
        serde_json::json!(["dash-demo-pattern-1"])
    );
    assert_eq!(optimized_body["pattern_demo_source_count"], 1);
    assert_eq!(optimized_body["demo_count"], 1);
    assert_eq!(
        optimized_body["train_observation_ids"],
        serde_json::json!(["dash-obs-3"])
    );
    assert_eq!(
        optimized_body["dev_observation_ids"],
        serde_json::json!(["dash-obs-2"])
    );
    assert_eq!(
        optimized_body["holdout_observation_ids"],
        serde_json::json!(["dash-obs-1"])
    );
    assert!(optimized_body["train_hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(optimized_body["child_agent_id"].as_str().is_some());
    let lineage_child_id: String =
        sqlx::query_scalar("SELECT child_agent_id FROM agent_slot_optimizations WHERE optimization_id = ?")
            .bind(optimization_id)
            .fetch_one(&state.api_context().db)
            .await
            .expect("read optimization lineage");
    assert_eq!(
        Some(lineage_child_id.as_str()),
        optimized_body["child_agent_id"].as_str()
    );
    let linked_demo_source: String = sqlx::query_scalar(
        "SELECT pattern_id FROM pattern_optimizations WHERE optimization_id = ? AND role = 'demo_source'",
    )
    .bind(optimization_id)
    .fetch_one(&state.api_context().db)
    .await
    .expect("read demo-source pattern optimization lineage");
    assert_eq!(linked_demo_source, "dash-demo-pattern-1");
    let linked_prior: String = sqlx::query_scalar(
        "SELECT pattern_id FROM pattern_optimizations WHERE optimization_id = ? AND role = 'prior'",
    )
    .bind(optimization_id)
    .fetch_one(&state.api_context().db)
    .await
    .expect("read pattern optimization lineage");
    assert_eq!(linked_prior, "dash-prior-1");

    let gated_optimization = server
        .post(&format!("/api/optimize/memory-demos/{optimization_id}/gate"))
        .json(&serde_json::json!({
            "dev_metric": "sharpe_delta",
            "parent_dev_score": 0.7,
            "child_dev_score": 0.9,
            "parent_holdout_score": 1.0,
            "child_holdout_score": 1.2,
            "gate_epsilon": 0.1,
            "gate_reason": "child beat parent on dev and holdout"
        }))
        .await;
    gated_optimization.assert_status_ok();
    let gated_optimization_body: serde_json::Value = gated_optimization.json();
    assert_eq!(gated_optimization_body["gate_verdict"], "passed");
    assert!((gated_optimization_body["delta_holdout"].as_f64().unwrap() - 0.2).abs() < 1e-9);

    let velocity = server
        .get(&format!("/api/flywheel/velocity?agent={}&days=7", agent.agent_id))
        .await;
    velocity.assert_status_ok();
    let velocity_body: serde_json::Value = velocity.json();
    assert_eq!(velocity_body["namespace"], namespace);
    assert_eq!(velocity_body["observations_captured"], 0);
    assert_eq!(velocity_body["optimized_child_agents"], 1);
    assert_eq!(velocity_body["autooptimizer_runs"], 1);

    let lineage = server
        .get(&format!("/api/flywheel/lineage?agent={}&limit=5", agent.agent_id))
        .await;
    lineage.assert_status_ok();
    let lineage_body: serde_json::Value = lineage.json();
    assert_eq!(lineage_body["namespace"], namespace);
    assert_eq!(lineage_body["total"], 1);
    assert_eq!(lineage_body["items"][0]["optimization_id"], optimization_id);
    assert_eq!(
        lineage_body["items"][0]["train_hash"],
        optimized_body["train_hash"]
    );
    assert_eq!(lineage_body["items"][0]["dev_hash"], optimized_body["dev_hash"]);
    assert_eq!(
        lineage_body["items"][0]["holdout_hash"],
        optimized_body["holdout_hash"]
    );
    assert_eq!(
        lineage_body["items"][0]["demo_source_pattern_ids"],
        serde_json::json!(["dash-demo-pattern-1"])
    );
    assert_eq!(
        lineage_body["items"][0]["prior_pattern_ids"],
        serde_json::json!(["dash-prior-1"])
    );
    assert_eq!(lineage_body["items"][0]["gate_verdict"], "passed");
    assert!((lineage_body["items"][0]["delta_dev"].as_f64().unwrap() - 0.2).abs() < 1e-9);
}

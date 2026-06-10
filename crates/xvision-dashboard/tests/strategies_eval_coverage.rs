//! HTTP-level coverage for the eval-coverage / origin fields on
//! `GET /api/strategies` (beads xvision-eb5).
//!
//! The home dashboard's "N strategies have no completed evals yet" line was
//! undercounting because the client joined a *page* of runs against a *page*
//! of strategies, and CLI-launched runs key `eval_runs.agent_id` by the
//! strategy bundle hash rather than the workspace ULID. The list endpoint now
//! computes per-strategy coverage server-side:
//!
//!   - `bundle_hash`               blake3 canonical-JSON hash of the bundle
//!   - `evaluated`                 any COMPLETED run keyed by ULID or hash
//!   - `last_eval_completed_at`    most recent completed run timestamp
//!   - `origin`                    "optimizer" when the bundle hash appears
//!                                 in autooptimizer `lineage_nodes`

use axum::http::StatusCode;
use axum_test::TestServer;
use chrono::Utc;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::autooptimizer::{
    gate::GateVerdict, lineage::ensure_lineage_schema, ContentHash, LineageNode, LineageStatus,
    LineageStore,
};
use xvision_engine::eval::{
    run::{Run, RunMode},
    store::RunStore,
};

async fn boot() -> (TestServer, AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, state, tmp)
}

async fn create_strategy(server: &TestServer, name: &str) -> String {
    let response = server
        .post("/api/strategies")
        .json(&serde_json::json!({ "name": name, "creator": "@operator" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json::<serde_json::Value>()["id"]
        .as_str()
        .expect("created strategy returns id")
        .to_string()
}

async fn list_item(server: &TestServer, id: &str) -> serde_json::Value {
    let response = server.get("/api/strategies").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .find(|item| item["agent_id"] == id)
        .unwrap_or_else(|| panic!("strategy {id} present in list"))
        .clone()
}

/// Seed the `scenarios` row eval runs reference (`eval_runs.scenario_id` FK).
async fn seed_scenario(state: &AppState) {
    sqlx::query(
        "INSERT OR IGNORE INTO scenarios \
         (id, source, display_name, body_json, created_at, created_by) \
         VALUES ('scenario-x', 'test', 'Scenario X', '{}', '2026-01-01T00:00:00Z', 'test')",
    )
    .execute(&state.pool)
    .await
    .expect("seed scenarios row");
}

/// Insert a COMPLETED eval run keyed by `agent_id` (ULID or bundle hash).
async fn insert_completed_run(state: &AppState, agent_id: &str, completed_at: &str) -> String {
    seed_scenario(state).await;
    let store = RunStore::new(state.pool.clone());
    let run = Run::new_queued(agent_id.to_string(), "scenario-x".into(), RunMode::Backtest);
    store.create(&run).await.expect("create run");
    sqlx::query("UPDATE eval_runs SET status = 'completed', completed_at = ? WHERE id = ?")
        .bind(completed_at)
        .bind(&run.id)
        .execute(&state.pool)
        .await
        .expect("complete run");
    run.id
}

#[tokio::test]
async fn fresh_strategy_is_unevaluated_user_origin_with_bundle_hash() {
    let (server, _state, _tmp) = boot().await;
    let id = create_strategy(&server, "Fresh").await;

    let item = list_item(&server, &id).await;

    assert_eq!(item["evaluated"], false);
    assert_eq!(item["origin"], "user");
    assert!(item.get("last_eval_completed_at").is_none());
    let hash = item["bundle_hash"].as_str().expect("bundle_hash string");
    assert_eq!(hash.len(), 64, "bundle_hash is 32-byte hex");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test]
async fn completed_run_keyed_by_ulid_marks_strategy_evaluated() {
    let (server, state, _tmp) = boot().await;
    let id = create_strategy(&server, "ByUlid").await;

    insert_completed_run(&state, &id, "2026-06-01T00:00:00Z").await;
    insert_completed_run(&state, &id, "2026-06-03T00:00:00Z").await;

    let item = list_item(&server, &id).await;
    assert_eq!(item["evaluated"], true);
    assert_eq!(item["last_eval_completed_at"], "2026-06-03T00:00:00Z");
}

#[tokio::test]
async fn completed_run_keyed_by_bundle_hash_marks_strategy_evaluated() {
    // Older CLI-launched runs store the bundle hash in eval_runs.agent_id —
    // those must still count toward the strategy's coverage.
    let (server, state, _tmp) = boot().await;
    let id = create_strategy(&server, "ByHash").await;

    let hash = list_item(&server, &id).await["bundle_hash"]
        .as_str()
        .expect("bundle_hash")
        .to_string();
    insert_completed_run(&state, &hash, "2026-06-02T00:00:00Z").await;

    let item = list_item(&server, &id).await;
    assert_eq!(item["evaluated"], true);
    assert_eq!(item["last_eval_completed_at"], "2026-06-02T00:00:00Z");
}

#[tokio::test]
async fn non_completed_runs_do_not_count() {
    let (server, state, _tmp) = boot().await;
    let id = create_strategy(&server, "QueuedOnly").await;

    seed_scenario(&state).await;
    let store = RunStore::new(state.pool.clone());
    let run = Run::new_queued(id.clone(), "scenario-x".into(), RunMode::Backtest);
    store.create(&run).await.expect("create queued run");

    let item = list_item(&server, &id).await;
    assert_eq!(item["evaluated"], false);
}

#[tokio::test]
async fn lineage_membership_marks_strategy_optimizer_origin() {
    let (server, state, _tmp) = boot().await;
    let id = create_strategy(&server, "Seeded").await;

    let hash_hex = list_item(&server, &id).await["bundle_hash"]
        .as_str()
        .expect("bundle_hash")
        .to_string();

    ensure_lineage_schema(&state.pool)
        .await
        .expect("lineage schema");
    let lineage = LineageStore::new(state.pool.clone());
    lineage
        .insert(&LineageNode {
            bundle_hash: ContentHash::from_hex(&hash_hex).expect("parse hash"),
            parent_hash: None,
            gate_verdict: GateVerdict::Pass,
            status: LineageStatus::Active,
            cycle_id: None,
            created_at: Utc::now(),
            diversity_score: None,
        })
        .await
        .expect("insert lineage node");

    let item = list_item(&server, &id).await;
    assert_eq!(item["origin"], "optimizer");
    // Lineage membership alone does not flip `evaluated` — that stays a
    // direct-eval-run signal; the UI segments optimizer-origin separately.
    assert_eq!(item["evaluated"], false);
}

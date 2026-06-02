use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;

use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

#[tokio::test]
async fn lineage_route_includes_writer_attribution_without_flywheel_history() {
    let (server, _tmp, state) = boot().await;
    let bundle_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    sqlx::query(
        "INSERT INTO lineage_nodes \
         (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at, diversity_score) \
         VALUES (?, NULL, 'passed', 'active', 'cycle-1', '2026-06-02T00:00:00Z', 0.42)",
    )
    .bind(bundle_hash)
    .execute(&state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO mutator_attribution \
         (bundle_hash, provider, model, prompt_version, proposed_at, delta_sharpe) \
         VALUES (?, 'openai', 'model-b', 'v2', '2026-06-02T00:00:00Z', 0.4)",
    )
    .bind(bundle_hash)
    .execute(&state.pool)
    .await
    .unwrap();

    let resp = server.get("/api/autooptimizer/lineage").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body[0]["bundle_hash"], bundle_hash);
    assert_eq!(body[0]["writer_provider"], "openai");
    assert_eq!(body[0]["writer_model"], "model-b");
    assert_eq!(body[0]["writer_prompt_version"], "v2");
}

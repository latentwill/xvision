//! Integration tests for the Inspector backend routes:
//! `GET /api/strategy/:id`, `PUT /api/strategy/:id/slot/:role`,
//! `PUT /api/strategy/:id/risk`, `POST /api/strategy/:id/validate`.
//!
//! Each test boots a real dashboard router against a tempdir XVN_HOME +
//! in-memory db (via `AppState::new`), creates a draft via the underlying
//! `engine::api::strategy::create_strategy`, and exercises the routes.

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::api::strategy::create_strategy;
use xvision_engine::authoring::CreateStrategyReq;

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

async fn create_draft(state: &AppState) -> String {
    create_strategy(
        &state.api_context(),
        CreateStrategyReq {
            template: "trend_follower".into(),
            name: "btc-mom-test".into(),
            creator: Some("@tester".into()),
        },
    )
    .await
    .expect("create draft")
    .id
}

#[tokio::test]
async fn get_strategy_returns_full_bundle() {
    let (server, _tmp, state) = boot().await;
    let id = create_draft(&state).await;

    let response = server.get(&format!("/api/strategy/{id}")).await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["manifest"]["id"], id);
    assert_eq!(body["manifest"]["template"], "trend_follower");
}

#[tokio::test]
async fn get_strategy_unknown_returns_404() {
    let (server, _tmp, _state) = boot().await;
    let response = server
        .get("/api/strategy/01TOTALLYMISSINGAGENTID000")
        .await;
    response.assert_status_not_found();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn put_slot_updates_prompt() {
    let (server, _tmp, state) = boot().await;
    let id = create_draft(&state).await;

    let response = server
        .put(&format!("/api/strategy/{id}/slot/trader"))
        .json(&serde_json::json!({ "prompt": "Decide carefully." }))
        .await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["id"], id);
    assert!(body["updated"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "prompt"));

    // Round-trip: fetch the bundle and confirm the prompt changed.
    let bundle: serde_json::Value =
        server.get(&format!("/api/strategy/{id}")).await.json();
    assert_eq!(bundle["trader_slot"]["prompt"], "Decide carefully.");
}

#[tokio::test]
async fn put_slot_unknown_role_is_400() {
    let (server, _tmp, state) = boot().await;
    let id = create_draft(&state).await;

    let response = server
        .put(&format!("/api/strategy/{id}/slot/no-such-role"))
        .json(&serde_json::json!({ "prompt": "x" }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn put_risk_preset_round_trips() {
    let (server, _tmp, state) = boot().await;
    let id = create_draft(&state).await;

    let response = server
        .put(&format!("/api/strategy/{id}/risk"))
        .json(&serde_json::json!({ "preset": "conservative" }))
        .await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["applied"], "preset");
    assert_eq!(body["id"], id);
}

#[tokio::test]
async fn put_risk_unknown_preset_is_400() {
    let (server, _tmp, state) = boot().await;
    let id = create_draft(&state).await;

    let response = server
        .put(&format!("/api/strategy/{id}/risk"))
        .json(&serde_json::json!({ "preset": "totally-not-a-preset" }))
        .await;
    response.assert_status_bad_request();
}

#[tokio::test]
async fn post_validate_returns_result_blob() {
    let (server, _tmp, state) = boot().await;
    let id = create_draft(&state).await;

    let response = server
        .post(&format!("/api/strategy/{id}/validate"))
        .await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["id"], id);
    assert!(body["ok"].is_boolean());
    assert!(body["errors"].is_array());
}

#[tokio::test]
async fn post_validate_unknown_draft_is_404() {
    let (server, _tmp, _state) = boot().await;
    let response = server
        .post("/api/strategy/01TOTALLYMISSINGAGENTID000/validate")
        .await;
    response.assert_status_not_found();
}

#[tokio::test]
async fn strategy_agents_add_rename_remove_round_trip() {
    let (server, _tmp, state) = boot().await;
    let strategy_id = create_draft(&state).await;

    // Create an agent first (strategy add-agent validates FK existence).
    let agent_resp = server
        .post("/api/agents")
        .json(&serde_json::json!({
            "name": "qa-agent",
            "description": "for inspector route test",
            "tags": [],
            "slots": [{
                "name": "main",
                "provider": "anthropic",
                "model": "claude-sonnet-4-6",
                "system_prompt": "Test slot prompt body for validator.",
                "skill_ids": [],
                "max_tokens": 512
            }]
        }))
        .await;
    agent_resp.assert_status_ok();
    let agent_body: serde_json::Value = agent_resp.json();
    let agent_id = agent_body["agent_id"].as_str().unwrap().to_string();

    let add = server
        .post(&format!("/api/strategy/{strategy_id}/agents"))
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "role": "trader"
        }))
        .await;
    add.assert_status_ok();
    let add_body: serde_json::Value = add.json();
    assert_eq!(add_body["agents"][0]["role"], "trader");

    let rename = server
        .patch(&format!("/api/strategy/{strategy_id}/agents/trader"))
        .json(&serde_json::json!({
            "new_role": "signal_trader"
        }))
        .await;
    rename.assert_status_ok();
    let rename_body: serde_json::Value = rename.json();
    assert_eq!(rename_body["agents"][0]["role"], "signal_trader");

    let remove = server
        .delete(&format!("/api/strategy/{strategy_id}/agents/signal_trader"))
        .await;
    remove.assert_status_ok();
    let remove_body: serde_json::Value = remove.json();
    assert_eq!(
        remove_body["agents"].as_array().unwrap().len(),
        0,
        "strategy should have no attached agents after remove"
    );
}

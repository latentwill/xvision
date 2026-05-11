//! Integration tests for `GET /api/search` — boots a real router against
//! a tempdir XVN_HOME, seeds the action set, and exercises the JSON
//! contract the React command palette relies on.

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::api::search as api_search;
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

#[tokio::test]
async fn search_returns_seeded_actions_on_empty_query() {
    let (server, _tmp, state) = boot().await;
    api_search::seed_actions(&state.api_context()).await;

    let response = server.get("/api/search?q=").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let hits = body["hits"].as_array().expect("hits is an array");
    assert!(!hits.is_empty(), "empty query returns at least the actions");
    assert!(hits.iter().all(|h| h["kind"] == "action"));
}

#[tokio::test]
async fn search_finds_strategy_after_create() {
    let (server, _tmp, state) = boot().await;
    create_strategy(
        &state.api_context(),
        CreateStrategyReq {
            template: "trend_follower".into(),
            name: "btc-momentum-search-test".into(),
            creator: Some("@tester".into()),
        },
    )
    .await
    .expect("create draft");

    // The create_strategy hook upserts into the index; query for a token
    // that lives in the strategy summary.
    let response = server.get("/api/search?q=trend_follower").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let hits = body["hits"].as_array().expect("hits is an array");
    assert!(hits.iter().any(|h| h["kind"] == "strategy"));
}

#[tokio::test]
async fn search_kind_filter_excludes_other_kinds() {
    let (server, _tmp, state) = boot().await;
    api_search::seed_actions(&state.api_context()).await;
    api_search::upsert_scenarios(&state.api_context()).await;

    let response = server.get("/api/search?q=&kind=scenario").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let hits = body["hits"].as_array().expect("hits is an array");
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| h["kind"] == "scenario"));
}

#[tokio::test]
async fn search_unknown_kind_is_400() {
    let (server, _tmp, _state) = boot().await;
    let response = server.get("/api/search?q=anything&kind=widget").await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

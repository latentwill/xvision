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

fn assert_hit_contract(hit: &serde_json::Value) {
    assert!(hit["artifact_id"].as_str().is_some(), "artifact_id is a string");
    assert!(hit["kind"].as_str().is_some(), "kind is a string");
    assert!(hit["title"].as_str().is_some(), "title is a string");
    assert!(hit["summary"].as_str().is_some(), "summary is a string");
    assert!(hit["tags"].as_array().is_some(), "tags is an array");
    assert!(
        chrono::DateTime::parse_from_rfc3339(hit["updated_at"].as_str().expect("updated_at is a string"))
            .is_ok(),
        "updated_at is RFC3339"
    );
    assert!(hit["href"].as_str().is_some(), "href is a string");
    assert!(hit["bm25_score"].as_f64().is_some(), "bm25_score is a number");
}

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
    for hit in hits {
        assert_hit_contract(hit);
    }

    let new_strategy = hits
        .iter()
        .find(|h| h["artifact_id"] == "new-strategy")
        .expect("new-strategy action is returned");
    assert_eq!(new_strategy["title"], "New strategy");
    assert_eq!(new_strategy["summary"], "Create a blank strategy draft");
    assert_eq!(new_strategy["href"], "/strategies/new");
    assert_eq!(new_strategy["tags"].as_array().unwrap().len(), 0);
    assert_eq!(new_strategy["bm25_score"], 0.0);
}

#[tokio::test]
async fn search_finds_strategy_after_create() {
    let (server, _tmp, state) = boot().await;
    let created = create_strategy(
        &state.api_context(),
        CreateStrategyReq {
            name: "btc-momentum-search-test".into(),
            creator: Some("@tester".into()),
        },
    )
    .await
    .expect("create draft");

    // The create_strategy hook upserts into the index; query for a token
    // that appears in the blank-draft summary. Post-2026-05-21 the
    // strategy template_registry was removed; the `template` label on
    // a blank draft is "custom".
    let response = server.get("/api/search?q=custom").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let hits = body["hits"].as_array().expect("hits is an array");
    let strategy = hits
        .iter()
        .find(|h| h["kind"] == "strategy" && h["artifact_id"] == created.id)
        .expect("created strategy is returned");
    assert_hit_contract(strategy);
    assert_eq!(strategy["title"], "btc-momentum-search-test");
    assert!(strategy["summary"]
        .as_str()
        .expect("summary is a string")
        .contains("custom"));
    assert_eq!(strategy["href"], format!("/strategies/{}", created.id));
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

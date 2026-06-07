//! Integration tests for `/api/tools`.

use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

#[tokio::test]
async fn tools_list_returns_builtin_catalog() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/tools").await;
    response.assert_status_ok();
    let body: Value = response.json();
    let items = body["items"].as_array().expect("items array");
    let names: Vec<&str> = items.iter().filter_map(|item| item["name"].as_str()).collect();

    assert!(names.contains(&"ohlcv"));
    assert!(names.contains(&"indicator_panel"));
    assert!(names.contains(&"submit_decision"));
    assert!(items.iter().all(|item| item["built_in"] == true));
}

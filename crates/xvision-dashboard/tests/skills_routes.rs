//! Integration tests for `/api/skills` routes.

use axum_test::TestServer;
use serde_json::{json, Value};
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

fn sample_body(name: &str) -> Value {
    json!({
        "name": name,
        "description": "a test skill",
        "kind": "tool",
        "config": {}
    })
}

#[tokio::test]
async fn skills_list_empty_on_fresh_db() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/skills").await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert!(body["items"].is_array());
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let (server, _tmp) = boot().await;

    let create_res = server.post("/api/skills").json(&sample_body("rsi-tool")).await;
    create_res.assert_status_ok();
    let created: Value = create_res.json();
    let id = created["skill_id"].as_str().unwrap().to_string();
    assert_eq!(created["name"], "rsi-tool");
    assert_eq!(created["kind"], "tool");

    let get_res = server.get(&format!("/api/skills/{}", id)).await;
    get_res.assert_status_ok();
    let got: Value = get_res.json();
    assert_eq!(got["skill_id"], id);
}

#[tokio::test]
async fn duplicate_name_returns_409() {
    let (server, _tmp) = boot().await;
    let body = sample_body("dup");

    server.post("/api/skills").json(&body).await.assert_status_ok();
    let second = server.post("/api/skills").json(&body).await;
    assert_eq!(second.status_code(), 409);
}

#[tokio::test]
async fn archive_then_list_excludes() {
    let (server, _tmp) = boot().await;

    let id = server
        .post("/api/skills")
        .json(&sample_body("to-archive"))
        .await
        .json::<Value>()["skill_id"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .delete(&format!("/api/skills/{}", id))
        .await
        .assert_status_ok();

    let list_res = server.get("/api/skills").await;
    let len = list_res.json::<Value>()["items"].as_array().unwrap().len();
    assert_eq!(len, 0);

    let with_archived = server.get("/api/skills?include_archived=true").await;
    let len_all = with_archived.json::<Value>()["items"].as_array().unwrap().len();
    assert_eq!(len_all, 1);
}

#[tokio::test]
async fn update_changes_kind_and_config() {
    let (server, _tmp) = boot().await;

    let id = server
        .post("/api/skills")
        .json(&sample_body("changeable"))
        .await
        .json::<Value>()["skill_id"]
        .as_str()
        .unwrap()
        .to_string();

    let patch = json!({
        "kind": "prompt_fragment",
        "config": { "text": "You are a careful trader." }
    });
    let res = server.put(&format!("/api/skills/{}", id)).json(&patch).await;
    res.assert_status_ok();
    let updated: Value = res.json();
    assert_eq!(updated["kind"], "prompt_fragment");
    assert_eq!(updated["config"]["text"], "You are a careful trader.");
}

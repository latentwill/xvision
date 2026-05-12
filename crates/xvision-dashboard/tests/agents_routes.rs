//! Integration tests for `/api/agents` routes.

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

fn sample_create_body(name: &str) -> Value {
    json!({
        "name": name,
        "description": "round-trip test agent",
        "tags": ["test"],
        "slots": [{
            "name": "main",
            "provider": "anthropic",
            "model": "claude-sonnet-4-6",
            "system_prompt": "You are a trader.",
            "max_tokens": 4096,
        }]
    })
}

#[tokio::test]
async fn agents_list_is_empty_on_fresh_db() {
    let (server, _tmp) = boot().await;
    let response = server.get("/api/agents").await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert!(body["items"].is_array(), "items must be array");
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let (server, _tmp) = boot().await;

    let create_res = server
        .post("/api/agents")
        .json(&sample_create_body("rt-agent-1"))
        .await;
    assert_eq!(create_res.status_code(), 200);
    let created: Value = create_res.json();
    let id = created["agent_id"].as_str().unwrap().to_string();
    assert_eq!(created["name"], "rt-agent-1");
    assert_eq!(created["slots"].as_array().unwrap().len(), 1);

    let get_res = server.get(&format!("/api/agents/{}", id)).await;
    get_res.assert_status_ok();
    let got: Value = get_res.json();
    assert_eq!(got["agent_id"], id);
    assert_eq!(got["name"], "rt-agent-1");
    assert_eq!(got["slots"][0]["name"], "main");
}

#[tokio::test]
async fn duplicate_name_returns_409() {
    let (server, _tmp) = boot().await;
    let body = sample_create_body("dup-name");

    let first = server.post("/api/agents").json(&body).await;
    first.assert_status_ok();

    let second = server.post("/api/agents").json(&body).await;
    assert_eq!(second.status_code(), 409);
    let err: Value = second.json();
    assert_eq!(err["code"], "conflict");
}

#[tokio::test]
async fn validate_returns_diagnostics_for_empty_prompt() {
    let (server, _tmp) = boot().await;

    let mut body = sample_create_body("validate-1");
    body["slots"][0]["system_prompt"] = json!(""); // trigger warning

    let create_res = server.post("/api/agents").json(&body).await;
    create_res.assert_status_ok();
    let id = create_res.json::<Value>()["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    let validate_res = server
        .post(&format!("/api/agents/{}/validate", id))
        .await;
    validate_res.assert_status_ok();
    let diags = validate_res.json::<Value>();
    let arr = diags["diagnostics"].as_array().unwrap();
    let codes: Vec<&str> = arr
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(
        codes.contains(&"slot_prompt_empty"),
        "expected slot_prompt_empty in {:?}",
        codes
    );
}

#[tokio::test]
async fn archive_then_list_excludes() {
    let (server, _tmp) = boot().await;

    let create_res = server
        .post("/api/agents")
        .json(&sample_create_body("to-archive"))
        .await;
    let id = create_res.json::<Value>()["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    let archive_res = server.delete(&format!("/api/agents/{}", id)).await;
    archive_res.assert_status_ok();

    let list_res = server.get("/api/agents").await;
    let items = list_res.json::<Value>()["items"].as_array().unwrap().len();
    assert_eq!(items, 0, "archived agent should not appear in default list");

    let list_archived = server
        .get("/api/agents?include_archived=true")
        .await;
    let items_all = list_archived.json::<Value>()["items"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(items_all, 1, "include_archived=true should return it");
}

#[tokio::test]
async fn deployed_in_returns_empty_v1_stub() {
    let (server, _tmp) = boot().await;

    let id = server
        .post("/api/agents")
        .json(&sample_create_body("stub-deployed"))
        .await
        .json::<Value>()["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    let res = server
        .get(&format!("/api/agents/{}/strategies", id))
        .await;
    res.assert_status_ok();
    let items = res.json::<Value>()["items"].as_array().unwrap().len();
    assert_eq!(items, 0, "v1 returns empty until strategies refactor");
}

#[tokio::test]
async fn update_replaces_slots() {
    let (server, _tmp) = boot().await;

    let id = server
        .post("/api/agents")
        .json(&sample_create_body("multi-slot"))
        .await
        .json::<Value>()["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    let patch = json!({
        "slots": [
            {
                "name": "trader",
                "provider": "anthropic",
                "model": "claude-sonnet-4-6",
                "system_prompt": "Trade.",
                "max_tokens": 4096,
            },
            {
                "name": "risk_check",
                "provider": "anthropic",
                "model": "claude-haiku-4-5",
                "system_prompt": "Check risk.",
                "max_tokens": 2048,
            }
        ]
    });
    let res = server.put(&format!("/api/agents/{}", id)).json(&patch).await;
    res.assert_status_ok();
    let updated: Value = res.json();
    let slots = updated["slots"].as_array().unwrap();
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0]["name"], "trader");
    assert_eq!(slots[1]["name"], "risk_check");
}

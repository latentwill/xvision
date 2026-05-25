//! Integration tests for `/api/agents` routes.

mod support;

use serde_json::{json, Value};
use support::test_server;

fn sample_prompt() -> &'static str {
    "You are a disciplined multi-asset trading agent. Review the latest market context, \
     active risk limits, portfolio exposure, and scenario notes before producing a trading \
     decision. Explain the setup, invalidation level, sizing rationale, and why the action \
     is appropriate for the current conditions. Return structured JSON only."
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
            "system_prompt": sample_prompt(),
            "skill_ids": [],
            "max_tokens": 4096,
        }]
    })
}

#[tokio::test]
async fn agents_list_is_empty_on_fresh_db() {
    let (server, _tmp) = test_server().await;
    let response = server.get("/api/agents").await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert!(body["items"].is_array(), "items must be array");
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let (server, _tmp) = test_server().await;

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
    let (server, _tmp) = test_server().await;
    let body = sample_create_body("dup-name");

    let first = server.post("/api/agents").json(&body).await;
    first.assert_status_ok();

    let second = server.post("/api/agents").json(&body).await;
    assert_eq!(second.status_code(), 409);
    let err: Value = second.json();
    assert_eq!(err["code"], "conflict");
}

#[tokio::test]
async fn create_rejects_empty_prompt() {
    let (server, _tmp) = test_server().await;

    let mut body = sample_create_body("validate-1");
    body["slots"][0]["system_prompt"] = json!("");

    let create_res = server.post("/api/agents").json(&body).await;
    assert_eq!(create_res.status_code(), 400);
    let err: Value = create_res.json();
    assert_eq!(err["code"], "validation");
    assert!(
        err["message"].as_str().unwrap().contains("system_prompt"),
        "expected system_prompt in message, got: {}",
        err["message"]
    );
}

#[tokio::test]
async fn create_rejects_whitespace_only_prompt() {
    let (server, _tmp) = test_server().await;

    let mut body = sample_create_body("validate-2");
    body["slots"][0]["system_prompt"] = json!("   \n\t  ");

    let create_res = server.post("/api/agents").json(&body).await;
    assert_eq!(create_res.status_code(), 400);
}

#[tokio::test]
async fn archive_then_list_excludes() {
    let (server, _tmp) = test_server().await;

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

    let list_archived = server.get("/api/agents?include_archived=true").await;
    let items_all = list_archived.json::<Value>()["items"].as_array().unwrap().len();
    assert_eq!(items_all, 1, "include_archived=true should return it");
}

#[tokio::test]
async fn list_scope_query_includes_strategy_scoped_agents() {
    let (server, _tmp) = test_server().await;

    let workspace_res = server
        .post("/api/agents")
        .json(&sample_create_body("workspace-visible"))
        .await;
    workspace_res.assert_status_ok();

    let mut scoped_body = sample_create_body("strategy-scoped");
    scoped_body["scope_strategy_id"] = json!("01STRATEGYSCOPED000000000000");
    let scoped_res = server.post("/api/agents").json(&scoped_body).await;
    scoped_res.assert_status_ok();

    let default_list = server.get("/api/agents").await;
    default_list.assert_status_ok();
    let default_items = default_list.json::<Value>()["items"].as_array().unwrap().len();
    assert_eq!(default_items, 1, "default list should hide scoped agents");

    let scoped_list = server.get("/api/agents?scope=01STRATEGYSCOPED000000000000").await;
    scoped_list.assert_status_ok();
    let scoped_json: Value = scoped_list.json();
    let names: Vec<_> = scoped_json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"workspace-visible"), "names: {names:?}");
    assert!(names.contains(&"strategy-scoped"), "names: {names:?}");
}

#[tokio::test]
async fn deployed_in_returns_empty_v1_stub() {
    let (server, _tmp) = test_server().await;

    let id = server
        .post("/api/agents")
        .json(&sample_create_body("stub-deployed"))
        .await
        .json::<Value>()["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    let res = server.get(&format!("/api/agents/{}/strategies", id)).await;
    res.assert_status_ok();
    let items = res.json::<Value>()["items"].as_array().unwrap().len();
    assert_eq!(items, 0, "v1 returns empty until strategies refactor");
}

#[tokio::test]
async fn update_replaces_slots() {
    let (server, _tmp) = test_server().await;

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
                "system_prompt": sample_prompt(),
                "skill_ids": [],
                "max_tokens": 4096,
            },
            {
                "name": "risk_check",
                "provider": "anthropic",
                "model": "claude-haiku-4-5",
                "system_prompt": "You are a risk-control reviewer. Check proposed trades against account exposure, \
                                  concentration, stop distance, expected liquidity, and scenario constraints. \
                                  Call out any breach before approval, require clear invalidation, and return a \
                                  concise structured assessment that an execution component can consume.",
                "skill_ids": [],
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

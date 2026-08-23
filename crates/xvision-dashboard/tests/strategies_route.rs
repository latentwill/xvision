//! HTTP-level RED coverage for Route Builder authoring routes.
//!
//! These tests defend the dashboard contract for route mutation: router
//! activation forwarding, save returning `{ strategy, readiness }`, dry-run
//! validation without persistence, exact body rejection, and preservation of
//! pre-existing graph metadata not owned by the route save.

use axum::http::StatusCode;
use axum_test::TestServer;
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

async fn create_strategy(server: &TestServer) -> String {
    let response = server
        .post("/api/strategies")
        .json(&serde_json::json!({
            "name": "Route Builder API Test",
            "creator": "@route-tester"
        }))
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json::<serde_json::Value>()["id"]
        .as_str()
        .expect("created strategy returns id")
        .to_string()
}

async fn create_agent(server: &TestServer, name: &str) -> String {
    let response = server
        .post("/api/agents")
        .json(&serde_json::json!({
            "name": name,
            "description": "agent fixture for Route Builder dashboard route tests",
            "tags": ["route-builder-test"],
            "slots": [{
                "name": "main",
                "provider": "anthropic",
                "model": "claude-sonnet-4-6",
                "system_prompt": "You are a deterministic route-builder test agent. Review strategy context, routing decisions, branch targets, risk limits, and market evidence before returning structured output suitable for validating dashboard route mutation behavior without relying on placeholder prose.",
                "skill_ids": [],
                "max_tokens": 512
            }]
        }))
        .await;
    response.assert_status_ok();
    response.json::<serde_json::Value>()["agent_id"]
        .as_str()
        .expect("created agent returns id")
        .to_string()
}

async fn add_agent(server: &TestServer, strategy_id: &str, agent_id: &str, role: &str, activates: &str) {
    let response = server
        .post(&format!("/api/strategy/{strategy_id}/agents"))
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "role": role,
            "activates": activates
        }))
        .await;
    response.assert_status_ok();
}

async fn create_route_builder_strategy(server: &TestServer) -> String {
    let strategy_id = create_strategy(server).await;
    let router = create_agent(server, "router-agent").await;
    let analyst = create_agent(server, "analyst-agent").await;
    let trader = create_agent(server, "trader-agent").await;

    add_agent(server, &strategy_id, &router, "router", "router").await;
    add_agent(server, &strategy_id, &analyst, "analyst", "trader").await;
    add_agent(server, &strategy_id, &trader, "trader", "trader").await;

    let response = server
        .put(&format!("/api/strategy/{strategy_id}/pipeline"))
        .json(&serde_json::json!({
            "kind": "graph",
            "edges": [{
                "from_role": "analyst",
                "to_role": "trader"
            }]
        }))
        .await;
    response.assert_status_ok();

    strategy_id
}

fn route_body() -> serde_json::Value {
    serde_json::json!({
        "router_role": "router",
        "branches": [{
            "target_role": "analyst"
        }],
        "context_fields": ["market_snapshot", "available_targets"],
        "trace_mode": "compact"
    })
}

#[tokio::test]
async fn dashboard_post_add_agent_forwards_router_activation() {
    let (server, _tmp) = boot().await;
    let strategy_id = create_strategy(&server).await;
    let router_agent = create_agent(&server, "router-capable-agent").await;

    let response = server
        .post(&format!("/api/strategy/{strategy_id}/agents"))
        .json(&serde_json::json!({
            "agent_id": router_agent,
            "role": "router",
            "activates": "router"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let router = body["agents"]
        .as_array()
        .expect("agents array")
        .iter()
        .find(|agent| agent["role"] == "router")
        .expect("router agent ref returned");
    assert_eq!(
        router["activates"], "router",
        "dashboard add-agent must forward activates=router instead of silently defaulting to trader"
    );
}

#[tokio::test]
async fn dashboard_post_add_agent_ignores_unknown_fields_while_forwarding_activation() {
    let (server, _tmp) = boot().await;
    let strategy_id = create_strategy(&server).await;
    let router_agent = create_agent(&server, "router-capable-agent-with-additive-field").await;

    let response = server
        .post(&format!("/api/strategy/{strategy_id}/agents"))
        .json(&serde_json::json!({
            "agent_id": router_agent,
            "role": "router",
            "activates": "router",
            "additive_client_field": "future-dashboard-metadata"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let router = body["agents"]
        .as_array()
        .expect("agents array")
        .iter()
        .find(|agent| agent["role"] == "router")
        .expect("router agent ref returned");
    assert_eq!(
        router["activates"], "router",
        "dashboard add-agent must ignore additive unknown fields without dropping supplied activates"
    );
}

#[tokio::test]
async fn put_route_saves_route_returns_strategy_readiness_and_preserves_graph_metadata() {
    let (server, _tmp) = boot().await;
    let strategy_id = create_route_builder_strategy(&server).await;

    let response = server
        .put(&format!("/api/strategy/{strategy_id}/route"))
        .json(&route_body())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["strategy"]["manifest"]["id"], strategy_id);
    assert_eq!(body["readiness"]["routed"], true);
    assert_eq!(
        body["readiness"]["context_fields"],
        serde_json::json!(["market_snapshot", "available_targets"]),
        "route save must return readiness for the exact context fields submitted",
    );
    assert_eq!(
        body["strategy"]["pipeline"]["route"],
        route_body(),
        "route save must persist the authored RouteDefinition on the returned strategy",
    );
    let edges = body["strategy"]["pipeline"]["edges"]
        .as_array()
        .expect("pipeline edges array");
    assert!(
        edges
            .iter()
            .any(|edge| edge["from_role"] == "analyst" && edge["to_role"] == "trader"),
        "route save must not delete existing graph metadata outside its ownership",
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge["from_role"] == "router" && edge["to_role"] == "analyst"),
        "route save must compile the router branch into executable graph edges",
    );

    let fetched: serde_json::Value = server.get(&format!("/api/strategy/{strategy_id}")).await.json();
    assert_eq!(fetched["pipeline"]["route"], route_body());
}

#[tokio::test]
async fn post_route_validate_returns_readiness_without_persisting_route() {
    let (server, _tmp) = boot().await;
    let strategy_id = create_route_builder_strategy(&server).await;

    let response = server
        .post(&format!("/api/strategy/{strategy_id}/route/validate"))
        .json(&route_body())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["strategy"]["manifest"]["id"], strategy_id);
    assert_eq!(body["readiness"]["routed"], true);
    assert_eq!(
        body["readiness"]["context_fields"],
        serde_json::json!(["market_snapshot", "available_targets"]),
        "route validation must calculate readiness from the supplied dry-run body",
    );
    assert_eq!(
        body["strategy"]["pipeline"]["route"],
        route_body(),
        "dry-run response should show the normalized candidate route for UI preview",
    );

    let fetched: serde_json::Value = server.get(&format!("/api/strategy/{strategy_id}")).await.json();
    assert!(
        fetched["pipeline"]["route"].is_null(),
        "route validation is a dry-run and must not persist the candidate route",
    );
}

#[tokio::test]
async fn put_route_rejects_unknown_top_level_fields_without_persisting() {
    let (server, _tmp) = boot().await;
    let strategy_id = create_route_builder_strategy(&server).await;
    let mut body = route_body();
    body["unexpected_contract_drift"] = serde_json::json!(true);

    let response = server
        .put(&format!("/api/strategy/{strategy_id}/route"))
        .json(&body)
        .await;

    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let fetched: serde_json::Value = server.get(&format!("/api/strategy/{strategy_id}")).await.json();
    assert!(
        fetched["pipeline"]["route"].is_null(),
        "unknown route body fields must fail before any partial route persistence",
    );
}

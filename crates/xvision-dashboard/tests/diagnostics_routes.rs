//! Integration tests for tool-based diagnostics routes.

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;

use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::agents::model::InputsPolicy;
use xvision_engine::agents::store::{AgentStore, NewAgent};
use xvision_engine::agents::AgentSlot;
use xvision_engine::strategies::agent_ref::AgentRef;
use xvision_engine::strategies::{
    manifest::PublicManifest, risk::RiskPreset, store::FilesystemStore, store::StrategyStore, Strategy,
};

const TRADER_PROMPT: &str = "You are a careful trader. Analyse the OHLCV data and respond with a \
    JSON object: action (buy/sell/hold), size_pct (0-100), reason.";

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

fn slot(name: &str, prompt: &str, tools: Vec<&str>) -> AgentSlot {
    AgentSlot {
        name: name.to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        system_prompt: prompt.to_string(),
        skill_ids: vec![],
        max_tokens: Some(4096),
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::default(),
        noop_skip: None,
        allowed_tools: tools.into_iter().map(str::to_string).collect(),
        delta_briefing: None,
    }
}

async fn seed_agent(state: &AppState, name: &str, prompt: &str, tools: Vec<&str>) -> String {
    AgentStore::new(state.pool.clone())
        .create(NewAgent {
            name: name.to_string(),
            description: "seeded for diagnostics route test".to_string(),
            tags: vec!["seed".to_string()],
            slots: vec![slot("trader", prompt, tools)],
            scope_strategy_id: None,
        })
        .await
        .unwrap()
}

async fn seed_strategy(tmp: &TempDir, strategy_id: &str, agent_id: &str, required_tools: Vec<String>) {
    let store = FilesystemStore::new(tmp.path().join("strategies"));
    store
        .save(&Strategy {
            manifest: PublicManifest {
                id: strategy_id.into(),
                display_name: "Diagnostics Fixture".into(),
                plain_summary: "seeded for diagnostics route test".into(),
                creator: "@diag-test".into(),
                template: "custom".into(),
                regime_fit: vec![],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 60,
                attested_with: vec![],
                required_tools,
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
                execution_mode: Default::default(),
                capital_mode: Default::default(),
                timeframe_requirements: Default::default(),
            },
            hypothesis: None,
            agents: vec![AgentRef {
                agent_id: agent_id.into(),
                role: "trader".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
            }],
            pipeline: Default::default(),
            regime_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn agent_diagnostics_reports_registered_tools() {
    let (server, _tmp, state) = boot().await;
    let agent_id = seed_agent(
        &state,
        "Ready Trader",
        TRADER_PROMPT,
        vec!["ohlcv", "submit_decision"],
    )
    .await;

    let resp = server.get(&format!("/api/agents/{agent_id}/diagnostics")).await;
    resp.assert_status_ok();
    let body: Value = resp.json();

    assert_eq!(body["agent_id"], agent_id);
    assert_eq!(body["agent_name"], "Ready Trader");
    assert_eq!(body["agent_ready"], true);
    assert_eq!(body["tool_names"].as_array().unwrap().len(), 2);

    let tools = body["slots"][0]["tools"].as_array().expect("tools");
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "submit_decision" && tool["registered"] == true));
}

#[tokio::test]
async fn agent_diagnostics_missing_prompt_flips_ready() {
    let (server, _tmp, state) = boot().await;
    let agent_id = seed_agent(&state, "Promptless", "   ", vec!["submit_decision"]).await;

    let resp = server.get(&format!("/api/agents/{agent_id}/diagnostics")).await;
    resp.assert_status_ok();
    let body: Value = resp.json();

    assert_eq!(body["agent_ready"], false);
    assert_eq!(body["slots"][0]["prompt_present"], false);
}

#[tokio::test]
async fn agent_diagnostics_unknown_id_404s() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .get("/api/agents/01JNONEXISTENT0000000000000/diagnostics")
        .await;
    assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn strategy_diagnostics_launchable_when_decision_tool_granted() {
    let (server, tmp, state) = boot().await;
    let agent_id = seed_agent(
        &state,
        "Strat Trader",
        TRADER_PROMPT,
        vec!["ohlcv", "submit_decision"],
    )
    .await;
    let strategy_id = "01J0DIAGTEST0000000000001A";
    seed_strategy(&tmp, strategy_id, &agent_id, vec![]).await;

    let resp = server
        .get(&format!("/api/strategy/{strategy_id}/diagnostics"))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();

    assert_eq!(body["strategy_id"], strategy_id);
    assert_eq!(body["launchable"], true);
    assert_eq!(body["has_decision_path"], true);
    assert_eq!(body["unregistered_tools"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn strategy_diagnostics_unregistered_tool_blocks_launch() {
    let (server, tmp, state) = boot().await;
    let agent_id = seed_agent(
        &state,
        "Strat Trader",
        TRADER_PROMPT,
        vec!["not_registered", "submit_decision"],
    )
    .await;
    let strategy_id = "01J0DIAGTEST0000000000002B";
    seed_strategy(&tmp, strategy_id, &agent_id, vec![]).await;

    let resp = server
        .get(&format!("/api/strategy/{strategy_id}/diagnostics"))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();

    assert_eq!(body["launchable"], false);
    assert_eq!(body["unregistered_tools"][0]["tool"], "not_registered");
}

#[tokio::test]
async fn strategy_diagnostics_unknown_id_404s() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .get("/api/strategy/01JNONEXISTENT0000000000000/diagnostics")
        .await;
    assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
}

//! Integration tests for the Phase 4.5 read-only diagnostics surface:
//!   GET /api/strategy/:id/diagnostics
//!   GET /api/agents/:id/diagnostics
//!
//! These exercise the HTTP shell over the engine `capability_diagnostics`
//! (strategy-scoped) and the dashboard's agent-level composition over the
//! engine's public diagnostics helpers. The agent library + strategy
//! filesystem store are seeded directly; the route layer is under test.
//!
//! Coverage:
//!   * agent diagnostics: a complete trader slot reports `Optimizable`
//!     (ready + has a dspy optimizer signature) and `agent_ready = true`;
//!     a prompt-less slot reports `MissingPrompt` and flips `agent_ready`.
//!   * strategy diagnostics: a strategy whose manifest grants the trader's
//!     `ohlcv` tool is `launchable`; built-in required tools are also
//!     launchable without explicit manifest grants.
//!   * unknown strategy / agent ids 404.

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;

use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::agents::model::InputsPolicy;
use xvision_engine::agents::store::{AgentStore, NewAgent};
use xvision_engine::agents::{default_capabilities, AgentSlot};
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

fn slot(name: &str, prompt: &str) -> AgentSlot {
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
        capabilities: default_capabilities(),
        delta_briefing: None,
    }
}

/// Seed a single-slot trader agent and return its id.
async fn seed_agent(state: &AppState, name: &str, prompt: &str) -> String {
    AgentStore::new(state.pool.clone())
        .create(NewAgent {
            name: name.to_string(),
            description: "seeded for diagnostics route test".to_string(),
            tags: vec!["seed".to_string()],
            slots: vec![slot("trader", prompt)],
            scope_strategy_id: None,
        })
        .await
        .unwrap()
}

/// Seed a strategy referencing `agent_id` as a trader.
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
            },
            hypothesis: None,
            agents: vec![AgentRef {
                agent_id: agent_id.into(),
                role: "trader".into(),
                activates: None,
            }],
            pipeline: Default::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
        })
        .await
        .unwrap();
}

// ── agent-level diagnostics ──────────────────────────────────────────────

#[tokio::test]
async fn agent_diagnostics_ready_trader_is_optimizable() {
    let (server, _tmp, state) = boot().await;
    let agent_id = seed_agent(&state, "Ready Trader", TRADER_PROMPT).await;

    let resp = server.get(&format!("/api/agents/{agent_id}/diagnostics")).await;
    resp.assert_status_ok();
    let body: Value = resp.json();

    assert_eq!(body["agent_id"], agent_id);
    assert_eq!(body["agent_name"], "Ready Trader");
    assert_eq!(body["agent_ready"], true);

    let slots = body["slots"].as_array().expect("slots array");
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0]["slot_name"], "trader");
    assert_eq!(slots[0]["model_bound"], true);
    assert_eq!(slots[0]["prompt_present"], true);

    let caps = slots[0]["capabilities"].as_array().expect("capabilities");
    let trader = caps
        .iter()
        .find(|c| c["capability"] == "trader")
        .expect("trader capability line present");
    // A complete trader slot is Ready AND optimizable → `optimizable` status.
    assert_eq!(trader["status"]["kind"], "optimizable");
    assert_eq!(trader["optimizable"], true);
    assert_eq!(trader["required_tools"][0], "ohlcv");

    // The trader capability is surfaced as optimizable at the agent level.
    let opt = body["optimizable_capabilities"]
        .as_array()
        .expect("optimizable_capabilities");
    assert!(opt.iter().any(|c| c == "trader"));
}

#[tokio::test]
async fn agent_diagnostics_missing_prompt_flips_ready() {
    let (server, _tmp, state) = boot().await;
    // Whitespace-only prompt → MissingPrompt blocker.
    let agent_id = seed_agent(&state, "Promptless", "   ").await;

    let resp = server.get(&format!("/api/agents/{agent_id}/diagnostics")).await;
    resp.assert_status_ok();
    let body: Value = resp.json();

    assert_eq!(body["agent_ready"], false);
    let caps = body["slots"][0]["capabilities"].as_array().unwrap();
    let trader = caps.iter().find(|c| c["capability"] == "trader").unwrap();
    assert_eq!(trader["status"]["kind"], "missing_prompt");
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

// ── strategy-level diagnostics ───────────────────────────────────────────

#[tokio::test]
async fn strategy_diagnostics_launchable_when_tool_granted() {
    let (server, tmp, state) = boot().await;
    let agent_id = seed_agent(&state, "Strat Trader", TRADER_PROMPT).await;
    let strategy_id = "01J0DIAGTEST0000000000001A";
    // Manifest grants the trader's required `ohlcv` tool → launchable.
    seed_strategy(&tmp, strategy_id, &agent_id, vec!["ohlcv".into()]).await;

    let resp = server
        .get(&format!("/api/strategy/{strategy_id}/diagnostics"))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();

    assert_eq!(body["strategy_id"], strategy_id);
    assert_eq!(body["launchable"], true);
    assert_eq!(
        body["required_unmet"].as_array().unwrap().len(),
        0,
        "no unmet requirements when launchable"
    );
    // Trader is required and optimizable.
    let req_caps = body["required_capabilities"].as_array().unwrap();
    assert!(req_caps.iter().any(|c| c == "trader"));
    assert!(body["optimizable"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "trader"));
}

#[tokio::test]
async fn strategy_diagnostics_allows_builtin_required_tool_without_manifest_grant() {
    let (server, tmp, state) = boot().await;
    let agent_id = seed_agent(&state, "Strat Trader", TRADER_PROMPT).await;
    let strategy_id = "01J0DIAGTEST0000000000002B";
    // Manifest grants NO tools, but `ohlcv` is a built-in tool and should
    // not block launchability.
    seed_strategy(&tmp, strategy_id, &agent_id, vec![]).await;

    let resp = server
        .get(&format!("/api/strategy/{strategy_id}/diagnostics"))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();

    assert_eq!(body["launchable"], true);
    let unmet = body["required_unmet"].as_array().unwrap();
    assert_eq!(
        unmet.len(),
        0,
        "built-in tools must not create unmet requirements"
    );
}

#[tokio::test]
async fn strategy_diagnostics_unknown_id_404s() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .get("/api/strategy/01JNONEXISTENT0000000000000/diagnostics")
        .await;
    assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
}

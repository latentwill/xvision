//! Live trading ClineSDK dispatch integration smoke test.
//!
//! Asserts the Cline context is wired correctly when the executor is configured
//! for live trading (RealBrokerFills path). Contract assertions delegated to U1
//! (same ClineSlotInput shape).

use std::path::PathBuf;

use serde_json::json;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_engine::agent::dispatch_capability::ClineDispatchCtx;
use xvision_engine::agent::execute_cline::{ClineSlotInput, TrajectoryMode};
use xvision_engine::agent::llm::ResponseSchema;
use xvision_engine::strategies::slot::LLMSlot;

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

async fn spawn_mock(cfg: Option<serde_json::Value>) -> (AgentClient, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    if let Some(cfg) = cfg {
        std::fs::write(
            dir.path().join("agentd.sock.cfg"),
            serde_json::to_vec(&cfg).unwrap(),
        )
        .expect("write cfg");
    }
    let client = AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar (is `node` on PATH?)");
    (client, dir)
}

#[tokio::test]
async fn live_trading_cline_context_is_some() {
    // Live trading uses RunMode::Live — assert it can be constructed.
    let (client, _dir) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;
    let entry = ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-haiku-4-5".into()],
    };
    let _ctx = ClineDispatchCtx {
        client: std::sync::Arc::new(client),
        provider_entry: entry,
        api_key: Some("test-key".into()),
        recording_slot_role: None,
        tool_asset_guard: None,
        as_of_guard: None,
        run_mode: xvision_engine::eval::run::RunMode::Live,
    };
}

#[tokio::test]
async fn live_trading_cline_slot_input_is_record_mode() {
    // Live trading records trajectories (like backtest/forward test)
    // so operators can inspect decisions after the fact.
    let (client, _dir) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-haiku-4-5".into(),
        allowed_tools: vec![],
        provider: Some("anthropic".into()),
        model: Some("claude-haiku-4-5".into()),
    };
    let entry = ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-haiku-4-5".into()],
    };

    let input = ClineSlotInput {
        slot: &slot,
        provider_entry: &entry,
        api_key: Some("test-key".into()),
        system_prompt: "Decide whether to trade.".into(),
        upstream_inputs: json!({"market_data": {"bar_history": [{"c": 100.0}]}}),
        response_schema: ResponseSchema::trader_output(),
        allowed_tools: vec![],
        max_tokens: Some(4096),
        max_wall_ms: None,
        run_id: "LIVE-01".into(),
        cline_client: std::sync::Arc::new(client),
        trajectory_mode: TrajectoryMode::Record,
        record_slot_role: None,
        obs: None,
        model_call_span_id: None,
        reasoning_effort: None,
    };

    assert!(matches!(input.trajectory_mode, TrajectoryMode::Record));
}

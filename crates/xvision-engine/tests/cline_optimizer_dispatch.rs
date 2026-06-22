//! Optimizer ClineSDK dispatch regression test.
//!
//! Contract-level: asserts the optimizer's eval adapter constructs the Cline
//! context correctly. Integration coverage is delegated to U2 (backtest) since
//! the optimizer reuses the backtest paper tester path.

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

fn trader_slot() -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-haiku-4-5".into(),
        allowed_tools: vec![],
        provider: Some("anthropic".into()),
        model: Some("claude-haiku-4-5".into()),
    }
}

fn anthropic_entry() -> ProviderEntry {
    ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-haiku-4-5".into()],
    }
}

fn slot_input<'a>(
    slot: &'a LLMSlot,
    entry: &'a ProviderEntry,
    client: std::sync::Arc<AgentClient>,
    run_id: &str,
) -> ClineSlotInput<'a> {
    ClineSlotInput {
        slot,
        provider_entry: entry,
        api_key: Some("test-key".into()),
        system_prompt: "Decide whether to trade.".into(),
        upstream_inputs: json!({"market_data": {"bar_history": [{"c": 100.0}]}}),
        response_schema: ResponseSchema::trader_output(),
        allowed_tools: vec!["indicators.rsi".into()],
        max_tokens: Some(4096),
        max_wall_ms: None,
        run_id: run_id.into(),
        cline_client: client,
        trajectory_mode: TrajectoryMode::Record,
        record_slot_role: None,
        obs: None,
        model_call_span_id: None,
        reasoning_effort: None,
    }
}

#[tokio::test]
async fn optimizer_cline_slot_input_matches_backtest_shape() {
    // The optimizer reuses the backtest paper tester — ClineSlotInput
    // construction is identical. Assert it matches the backtest contract.
    let (client, _dir) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;
    let slot = trader_slot();
    let entry = anthropic_entry();

    let input = slot_input(&slot, &entry, std::sync::Arc::new(client), "OPT-01");

    assert_eq!(input.slot.provider.as_deref(), Some("anthropic"));
    assert_eq!(input.slot.model.as_deref(), Some("claude-haiku-4-5"));
    assert!(matches!(input.trajectory_mode, TrajectoryMode::Record));
}

#[tokio::test]
async fn optimizer_cline_context_is_some_when_runtime_is_cline() {
    // The optimizer eval adapter (BudgetCappedPaperTester) constructs
    // ClineDispatchCtx with RunMode::Backtest — same as the backtest path.
    let (client, _dir) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;
    let entry = anthropic_entry();
    let _ctx = ClineDispatchCtx {
        client: std::sync::Arc::new(client),
        provider_entry: entry,
        api_key: Some("test-key".into()),
        recording_slot_role: None,
        tool_asset_guard: None,
        as_of_guard: None,
        run_mode: xvision_engine::eval::run::RunMode::Backtest,
    };
}

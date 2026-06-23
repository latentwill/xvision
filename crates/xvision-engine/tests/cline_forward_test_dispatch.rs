//! Forward-test ClineSDK dispatch regression tests.
//!
//! Contract-level: asserts `ClineSlotInput` shape, provider-mapping abort,
//! and error boundaries for the forward test path.
//!
//! Follows `cline_execute_slot.rs` patterns for mock sidecar spawning and
//! `ClineSlotInput` construction.

use std::path::PathBuf;

use serde_json::json;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_engine::agent::execute_cline::{execute_slot_cline, ClineSlotInput, TrajectoryMode};
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
        allowed_tools: vec!["indicators.rsi".into()],
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

// ── Contract-level tests ───────────────────────────────────────────────────

#[tokio::test]
async fn forward_test_cline_slot_input_has_correct_shape() {
    let (_client, _dir) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;
    let slot = trader_slot();
    let entry = anthropic_entry();
    let (client, _dir2) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;

    let input = slot_input(&slot, &entry, std::sync::Arc::new(client), "01FWD-01");

    assert_eq!(input.slot.provider.as_deref(), Some("anthropic"));
    assert_eq!(input.slot.model.as_deref(), Some("claude-haiku-4-5"));
    assert!(input.max_tokens.is_some());
    assert!(!input.system_prompt.is_empty());
    assert!(input.allowed_tools.iter().any(|t| t == "indicators.rsi"));
    assert!(matches!(input.trajectory_mode, TrajectoryMode::Record));
}

#[tokio::test]
async fn forward_test_provider_unmapped_is_error() {
    // local-candle has no Cline mapping (matches cline_execute_slot.rs)
    let (client, _dir) = spawn_mock(None).await;
    let client = std::sync::Arc::new(client);
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "local".into(),
        allowed_tools: Vec::new(),
        provider: Some("local".into()),
        model: Some("mock".into()),
    };
    let entry = ProviderEntry {
        name: "local".into(),
        kind: ProviderKind::LocalCandle,
        base_url: String::new(),
        api_key_env: String::new(),
        enabled_models: vec!["mock".into()],
    };
    let err = execute_slot_cline(slot_input(&slot, &entry, client, "FWD-ERR"))
        .await
        .expect_err("unmapped provider must abort");
    let msg = format!("{err:#}");
    assert!(msg.contains("no Cline mapping"), "got: {msg}");
}

#[tokio::test]
async fn forward_test_cline_uses_trajectory_mode_record() {
    let (_client, _dir) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;
    let slot = trader_slot();
    let entry = anthropic_entry();
    let (client, _dir2) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;

    let input = slot_input(&slot, &entry, std::sync::Arc::new(client), "01FWD-REC");
    assert!(matches!(input.trajectory_mode, TrajectoryMode::Record));
}

#[tokio::test]
async fn forward_test_slot_input_has_response_schema() {
    let (_client, _dir) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;
    let slot = trader_slot();
    let entry = anthropic_entry();
    let (client, _dir2) = spawn_mock(Some(json!({"decisionJson": r#"{"action":"hold"}"#}))).await;

    let input = slot_input(&slot, &entry, std::sync::Arc::new(client), "01FWD-SCHEMA");
    let schema_str = serde_json::to_string(&input.response_schema).unwrap();
    assert!(!schema_str.is_empty());
    assert!(schema_str.contains("trader_output"), "response schema should be trader_output");
}

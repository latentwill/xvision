//! Parity guard for the Cline-only trader capability dispatch.

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use tempfile::TempDir;
use xvision_agent_client::AgentClient;
use xvision_core::config::{AgentRuntime, ProviderEntry, ProviderKind};
use xvision_engine::agent::dispatch_capability::{
    dispatch_capability, AgentOutput, ClineDispatchCtx, DispatchInput,
};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::pipeline::ResolvedAgentSlot;
use xvision_engine::agents::Capability;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

fn anthropic_entry() -> ProviderEntry {
    ProviderEntry {
        name: "anthropic".into(),
        kind: ProviderKind::Anthropic,
        base_url: String::new(),
        api_key_env: "K".into(),
        enabled_models: vec!["claude-sonnet-4-6".into()],
    }
}

async fn spawn_mock(decision_json: &str) -> (Arc<AgentClient>, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    std::fs::write(
        dir.path().join("agentd.sock.cfg"),
        serde_json::json!({ "decisionJson": decision_json }).to_string(),
    )
    .expect("write mock agentd config");
    let client = AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar");
    (Arc::new(client), dir)
}

struct RecordingDispatch {
    seen: Mutex<Vec<LlmRequest>>,
    text: String,
}

impl RecordingDispatch {
    fn new(text: impl Into<String>) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            text: text.into(),
        }
    }
}

#[async_trait]
impl LlmDispatch for RecordingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.seen.lock().unwrap().push(req);
        Ok(LlmResponse {
            content: vec![ContentBlock::Text {
                text: self.text.clone(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 7,
            output_tokens: 11,
        })
    }
}

fn resolved_trader() -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: "trader".into(),
        slot: LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4-6".into(),
            allowed_tools: vec!["ohlcv".into(), "submit_decision".into()],
            provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-6".into()),
        },
        system_prompt: "Decide.".into(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: "parity-trader".into(),
        noop_skip: false,
        nano: None,
    }
}

const CANNED_JSON: &str = r#"{"action":"hold","conviction":0.3,"justification":"parity"}"#;

#[tokio::test]
async fn raw_response_text_is_byte_identical_to_trader_decision_response_text() {
    let resolved_slot = resolved_trader();
    let slot = resolved_slot.slot.clone();
    let (client, _sidecar_dir) = spawn_mock(CANNED_JSON).await;
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(RecordingDispatch::new(
        r#"{"action":"flat","conviction":0.0,"justification":"unused"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let outcome = dispatch_capability(DispatchInput {
        resolved: &resolved_slot,
        slot: &slot,
        system_prompt: "Decide.".into(),
        upstream_inputs: serde_json::json!({}),
        dispatch,
        tools,
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: "parity-trader".into(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-parity".into(),
        scenario_id: "sc-parity".into(),
        cycle_idx: 0,
        invocation_suffix: None,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
        current_index: 0,
        total_agents: 1,
        agent_roles: &["trader".to_string()],
        activates: Capability::Trader,
        recorder: None,
        runtime: AgentRuntime::Cline,
        cline: Some(ClineDispatchCtx {
            client,
            provider_entry: anthropic_entry(),
            api_key: Some("test-key".into()),
            recording_slot_role: None,
            tool_asset_guard: None,
            as_of_guard: None,
            run_mode: xvision_engine::eval::run::RunMode::Backtest,
        }),
        model_call_span_id: None,
    })
    .await
    .expect("dispatch_capability must succeed");

    let raw = outcome.raw_response.expect("trader raw response");
    let raw_text = raw.text();
    let trader_text = match outcome.output {
        AgentOutput::Trader(t) => t.response.text(),
        other => panic!("expected AgentOutput::Trader, got {other:?}"),
    };
    assert_eq!(raw_text, trader_text);
    assert_eq!(raw_text, CANNED_JSON);
    // The mock sidecar usage is (input=11, output=7).
    assert_eq!(outcome.input_tokens, 11);
    assert_eq!(outcome.output_tokens, 7);
}

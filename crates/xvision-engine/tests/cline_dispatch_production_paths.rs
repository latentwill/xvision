//! Production-path Cline dispatch regression tests across eval surfaces.
//!
//! These drive `dispatch_capability` with a real `ClineDispatchCtx` and the
//! mock agentd sidecar. They intentionally do not hand-build `ClineSlotInput`:
//! the production dispatcher must construct it and reach `session.step`.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_engine::agent::dispatch_capability::{
    dispatch_capability, AgentOutput, ClineDispatchCtx, DispatchInput,
};
use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::agent::pipeline::ResolvedAgentSlot;
use xvision_engine::agents::{Capability, InputsPolicy};
use xvision_engine::eval::run::RunMode;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

async fn spawn_mock(record_steps_path: &std::path::Path) -> (AgentClient, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    let cfg = json!({
        "decisionJson": r#"{"action":"hold","conviction":0.5,"justification":"mock"}"#,
        "recordStepsPath": record_steps_path,
    });
    std::fs::write(sock.with_extension("sock.cfg"), serde_json::to_vec(&cfg).unwrap()).expect("write cfg");
    let client = AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar (is `node` on PATH?)");
    (client, dir)
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

fn resolved_slot() -> ResolvedAgentSlot {
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-haiku-4-5".into(),
        allowed_tools: vec!["indicators.rsi".into()],
        provider: Some("anthropic".into()),
        model: Some("claude-haiku-4-5".into()),
    };
    ResolvedAgentSlot {
        role: slot.role.clone(),
        slot,
        system_prompt: "Decide whether to trade.".into(),
        max_tokens: Some(4096),
        max_wall_ms: None,
        temperature: None,
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: "agent-cline-production".into(),
        noop_skip: false,
        nano: None,
    }
}

async fn assert_surface_reaches_sidecar(surface: &str, run_mode: RunMode) {
    let dir = TempDir::new().expect("steps tempdir");
    let steps = dir.path().join(format!("{surface}.jsonl"));
    let (client, _agentd_dir) = spawn_mock(&steps).await;
    let resolved = resolved_slot();
    let slot = resolved.slot.clone();
    let run_id = format!("run-{surface}");

    let outcome = dispatch_capability(DispatchInput {
        resolved: &resolved,
        slot: &slot,
        system_prompt: resolved.system_prompt.clone(),
        upstream_inputs: json!({"surface": surface, "market_data": {"bar_history": [{"c": 100.0}]}}),
        dispatch: Arc::new(MockDispatch::echo("{}")),
        tools: Arc::new(ToolRegistry::default_with_builtins()),
        max_tokens: resolved.max_tokens,
        max_wall_ms: resolved.max_wall_ms,
        temperature: resolved.temperature,
        obs: None,
        memory: None,
        memory_mode: resolved.memory_mode,
        agent_id: resolved.agent_id.clone(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: run_id.clone(),
        scenario_id: format!("scenario-{surface}"),
        cycle_idx: 0,
        invocation_suffix: None,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
        current_index: 0,
        total_agents: 1,
        activates: Capability::Trader,
        recorder: None,
        runtime: Default::default(),
        cline: Some(ClineDispatchCtx {
            client: Arc::new(client),
            provider_entry: anthropic_entry(),
            api_key: Some("test-key".into()),
            recording_slot_role: None,
            tool_asset_guard: None,
            as_of_guard: None,
            run_mode,
        }),
        model_call_span_id: None,
    })
    .await
    .expect("dispatch_capability should route trader through Cline");

    match outcome.output {
        AgentOutput::Trader(t) => assert!(t.response.text().contains("hold")),
        other => panic!("expected trader output, got {other:?}"),
    }
    assert_eq!(outcome.input_tokens, 11);
    assert_eq!(outcome.output_tokens, 7);

    let recorded = std::fs::read_to_string(&steps).expect("mock sidecar should record a step");
    assert!(
        recorded.contains(&format!("{run_id}::trader::cycle0")),
        "production Cline run id should reach sidecar step log: {recorded}"
    );
    assert!(
        recorded.contains(surface),
        "surface-specific upstream inputs should reach sidecar prompt: {recorded}"
    );
}

#[tokio::test]
async fn forward_test_dispatch_reaches_cline_sidecar() {
    assert_surface_reaches_sidecar("forward", RunMode::Backtest).await;
}

#[tokio::test]
async fn backtest_dispatch_reaches_cline_sidecar() {
    assert_surface_reaches_sidecar("backtest", RunMode::Backtest).await;
}

#[tokio::test]
async fn optimizer_dispatch_reaches_cline_sidecar() {
    assert_surface_reaches_sidecar("optimizer", RunMode::Backtest).await;
}

#[tokio::test]
async fn live_trading_dispatch_reaches_cline_sidecar() {
    assert_surface_reaches_sidecar("live", RunMode::Live).await;
}

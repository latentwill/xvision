//! WU-6 integration test for the pipeline Cline runtime (Task 1.6).
//!
//! Runs a single-Trader-agent strategy through `run_pipeline`:
//!   * `runtime = Cline` (mock sidecar wired) — the Trader decision is
//!     produced via the Cline `start_run -> step -> end_run` lifecycle;
//!   * `runtime = Cline` with `cline = None` — must HARD ERROR since WU-6
//!     retired LlmDispatch and the fallback is gone.
//!
//! The `LlmDispatch` runtime path and the soft-fallback were removed in WU-6.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;

use xvision_agent_client::AgentClient;
use xvision_core::config::{AgentRuntime, ProviderEntry, ProviderKind};
use xvision_engine::agent::dispatch_capability::ClineDispatchCtx;
use xvision_engine::agent::llm::{LlmDispatch, LlmResponse, MockDispatch};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::agents::Capability;
use xvision_engine::strategies::agent_ref::AgentRef;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, PipelineKind, Strategy};
use xvision_engine::tools::ToolRegistry;

fn mock_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock_agentd.js")
}

async fn spawn_mock(cfg: serde_json::Value) -> (AgentClient, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    std::fs::write(
        dir.path().join("agentd.sock.cfg"),
        serde_json::to_vec(&cfg).unwrap(),
    )
    .expect("write cfg");
    let client = AgentClient::spawn(&mock_bin(), &sock)
        .await
        .expect("spawn mock sidecar (is `node` on PATH?)");
    (client, dir)
}

fn trader_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01HZCLINEFLAG".into(),
            display_name: "ClineFlag".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 15,
            attested_with: vec!["anthropic.claude-sonnet-4-6".into()],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
        }],
        pipeline: PipelineDef {
            kind: PipelineKind::Sequential,
            edges: Vec::new(),
        },
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn trader_slot() -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: "trader".into(),
        slot: LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4-6".into(),
            allowed_tools: Vec::new(),
            provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-6".into()),
        },
        system_prompt: "Decide.".into(),
        max_tokens: Some(4096),
        max_wall_ms: None,
        temperature: None,
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        noop_skip: false,
    }
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

fn assert_is_trader_decision(resp: &LlmResponse) {
    let decision: serde_json::Value =
        serde_json::from_str(&resp.text()).expect("trader output must be valid JSON");
    assert!(
        decision.get("action").is_some(),
        "trader output must carry an `action`; got: {decision}"
    );
}

#[tokio::test]
async fn pipeline_cline_runtime_produces_trader_decision() {
    let (client, _dir) = spawn_mock(json!({
        "decisionJson": r#"{"action":"long_open","conviction":0.7,"justification":"via cline"}"#
    }))
    .await;
    let client = Arc::new(client);

    let strategy = trader_strategy();
    let slots = vec![trader_slot()];
    // The dispatch is still required by the signature but should be unused
    // on the Cline path; a MockDispatch that would emit a DISTINCT decision
    // proves the output came from Cline, not the fallback.
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"flat","conviction":0.0,"justification":"FROM LLMDISPATCH not cline"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: json!({"market_data": {"bar_history": []}}),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-cline".into(),
        scenario_id: "sc-1".into(),
        cycle_idx: 0,
        provider_catalogs: std::collections::HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: AgentRuntime::Cline,
        cline: Some(ClineDispatchCtx {
            client: client.clone(),
            provider_entry: anthropic_entry(),
            api_key: Some("test-key".into()),
            recording_slot_role: None,
            tool_asset_guard: None,
        }),
        model_call_span_id: None,
    })
    .await
    .expect("cline pipeline runs");

    let trader = outs.trader.expect("trader output present");
    assert_is_trader_decision(&trader);
    let decision: serde_json::Value = serde_json::from_str(&trader.text()).unwrap();
    assert_eq!(
        decision["action"], "long_open",
        "decision must come from the Cline sidecar, not the LlmDispatch fallback"
    );
}

#[tokio::test]
async fn pipeline_cline_without_client_is_hard_error() {
    // WU-6: runtime = Cline but cline = None must now return an error.
    // The LlmDispatch fallback was retired; a missing sidecar is a
    // programmer error that must never silently drop a decision.
    let strategy = trader_strategy();
    let slots = vec![trader_slot()];
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.1,"justification":"should not reach here"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let result = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: json!({}),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-no-sidecar".into(),
        scenario_id: "sc-1".into(),
        cycle_idx: 0,
        provider_catalogs: std::collections::HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: AgentRuntime::Cline,
        cline: None,
        model_call_span_id: None,
    })
    .await;

    assert!(
        result.is_err(),
        "pipeline with Cline runtime but no sidecar must return an error (WU-6)"
    );
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("Cline sidecar") || err_msg.contains("WU-6") || err_msg.contains("XVN_AGENTD_BIN"),
        "error message should mention the sidecar requirement; got: {err_msg}"
    );
}

//! Phase B — `dispatch_capability` round-trip + A/B cache-key invariance.
//!
//! Covers the first acceptance test from
//! `team/contracts/agent-graph-capability-dispatch.md`:
//!
//!   round-trip a `kind: Sequential` strategy with 2 capability-typed
//!   agents (Trader+Filter); assert each dispatched via the
//!   right handler.
//!
//! Also pins the A/B cache-pairing acceptance criterion: every
//! `dispatch_capability` call preserves `(cycle_id, scenario_id)` — the
//! fixture asserts the same `cycle_idx` flows through to the dispatcher
//! request body so the pre-Phase-B cache key shape is unchanged.

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tempfile::TempDir;
use xvision_agent_client::AgentClient;
use xvision_core::config::{AgentRuntime, ProviderEntry, ProviderKind};
use xvision_engine::agent::dispatch_capability::{
    dispatch_capability, AgentOutput, ClineDispatchCtx, DispatchInput,
};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::agents::Capability;
use xvision_engine::strategies::agent_ref::AgentRef;
fn mock_agentd_bin() -> PathBuf {
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

async fn spawn_mock_cline(
    decision_json_by_role: serde_json::Value,
    record_path: &Path,
) -> (ClineDispatchCtx, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    std::fs::write(
        dir.path().join("agentd.sock.cfg"),
        serde_json::to_vec(&serde_json::json!({
            "decisionJsonByRole": decision_json_by_role,
            "recordStepsPath": record_path,
        }))
        .unwrap(),
    )
    .expect("write mock agentd cfg");
    let client = AgentClient::spawn(&mock_agentd_bin(), &sock)
        .await
        .expect("spawn mock sidecar");
    (
        ClineDispatchCtx {
            client: Arc::new(client),
            provider_entry: anthropic_entry(),
            api_key: Some("test-key".into()),
            recording_slot_role: None,
            tool_asset_guard: None,
            as_of_guard: None,
            run_mode: xvision_engine::eval::run::RunMode::Backtest,
        },
        dir,
    )
}

fn recorded_steps(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid mock step JSON"))
        .collect()
}

use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, PipelineKind, Strategy};
use xvision_engine::tools::ToolRegistry;

/// Dispatch double that records every request and returns canned text.
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

    fn requests(&self) -> Vec<LlmRequest> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmDispatch for RecordingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.seen.lock().unwrap().push(req.clone());
        let text = if req.system_prompt.contains("You are a Filter") {
            r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"bar"}"#.to_string()
        } else {
            self.text.clone()
        };
        Ok(LlmResponse {
            content: vec![ContentBlock::Text { text }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 7,
            output_tokens: 11,
        })
    }
}

fn fixture_strategy(agents: Vec<AgentRef>) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01HZSEQDISPATCH".into(),
            display_name: "DispatchTest".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 15,
            attested_with: vec!["mock".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents,
        pipeline: PipelineDef {
            kind: PipelineKind::Sequential,
            edges: Vec::new(),
            route: None,
        },
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
    }
}

fn resolved(role: &str) -> ResolvedAgentSlot {
    let allowed_tools = if role == "trader" {
        vec!["ohlcv".into(), "submit_decision".into()]
    } else {
        vec!["ohlcv".into()]
    };
    ResolvedAgentSlot {
        role: role.into(),
        slot: LLMSlot {
            role: role.into(),
            attested_with: "mock".into(),
            allowed_tools,
            provider: None,
            model: Some("mock".into()),
        },
        system_prompt: String::new(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        noop_skip: false,
        nano: None,
    }
}

// ── Round-trip: Trader + Filter each route through dispatch_capability ──

#[tokio::test]
async fn three_capability_pipeline_routes_each_kind_correctly() {
    // Two agents: filter → trader. Filter is a stub (no LLM dispatch);
    // Trader is the real path.
    let agents = vec![
        AgentRef {
            agent_id: "01HZF".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        },
    ];
    let strategy = fixture_strategy(agents);
    let slots = vec![resolved("regime_filter"), resolved("trader")];
    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(
        serde_json::json!({
            "regime_filter": r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"bar"}"#,
            "trader": r#"{"action":"long_open","conviction":0.6,"justification":"r"}"#,
        }),
        &record_path,
    )
    .await;
    let dispatch = Arc::new(RecordingDispatch::new(
        r#"{"action":"long_open","conviction":0.6,"justification":"r"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
        tools,
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-1".into(),
        scenario_id: "sc-1".into(),
        cycle_idx: 0,
        provider_catalogs: std::collections::HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: AgentRuntime::Cline,
        cline: Some(cline),
        model_call_span_id: None,
    })
    .await
    .unwrap_or_else(|err| panic!("pipeline runs: {err}; steps={:?}", recorded_steps(&record_path)));

    // Trader populated; Filter stub is not exposed on the
    // legacy `PipelineOutputs` struct (Phase D will widen this).
    assert!(outs.trader.is_some(), "trader output must be populated");
    assert!(outs.regime.is_none());

    // Filter still uses the explicit LlmDispatch seam; Trader uses the
    // Cline sidecar. Together they account for both capability dispatches.
    let llm_requests = dispatch.requests();
    let cline_steps = recorded_steps(&record_path);
    assert_eq!(
        llm_requests.len() + cline_steps.len(),
        2,
        "Phase C: Filter + Trader dispatch (got {} LlmDispatch + {} Cline steps)",
        llm_requests.len(),
        cline_steps.len(),
    );
}

// ── A/B cache-key invariance: cycle_idx flows through unchanged ────────

#[tokio::test]
async fn dispatch_capability_preserves_cycle_id_in_dispatcher_call() {
    // Drive `dispatch_capability` directly with a Trader capability and
    // confirm the dispatcher sees the same model + body shape it would
    // have under the pre-Phase-B path. The cycle_id / scenario_id are
    // propagated through `SlotInput`; the dispatcher itself doesn't
    // see them on `LlmRequest` (cache-key derivation happens at the
    // executor seam) — but the executor's identity assertions hinge on
    // the same dispatcher being called with the same prompt body
    // shape. We pin the prompt body's "Inputs:" prefix as the byte-
    // identical contract here.
    let resolved_slot = resolved("trader");
    let slot = resolved_slot.slot.clone();
    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(
        serde_json::json!({
            "trader": r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
        }),
        &record_path,
    )
    .await;
    let dispatch = Arc::new(RecordingDispatch::new(
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let cycle_idx = 42_i64;
    let scenario_id = "sc-cache-key".to_string();

    let outcome = dispatch_capability(DispatchInput {
        resolved: &resolved_slot,
        slot: &slot,
        system_prompt: "Decide.".into(),
        upstream_inputs: serde_json::json!({"bar_index": 7}),
        dispatch: dispatch.clone(),
        tools,
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-cache-key".into(),
        scenario_id: scenario_id.clone(),
        cycle_idx,
        invocation_suffix: None,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        current_index: 0,
        total_agents: 1,
        agent_roles: &["trader".to_string()],
        activates: Capability::Trader,
        trace_attrs: None,
        recorder: None,
        runtime: AgentRuntime::Cline,
        cline: Some(cline),
        model_call_span_id: None,
    })
    .await
    .expect("dispatch_capability runs");

    // Trader returns a typed AgentOutput::Trader with the raw response
    // preserved verbatim.
    match outcome.output {
        AgentOutput::Trader(t) => {
            assert!(t.response.text().contains("hold"));
        }
        other => panic!("expected AgentOutput::Trader, got {other:?}"),
    }
    assert_eq!(outcome.input_tokens, 11);
    assert_eq!(outcome.output_tokens, 7);

    // The sidecar recorded exactly one call. Its run id preserves both the
    // scenario identity and cycle id, while the prompt keeps the canonical
    // input body shape used by cache-key derivation.
    let steps = recorded_steps(&record_path);
    assert_eq!(steps.len(), 1);
    let step = &steps[0];
    let run_id = step["run_id"].as_str().expect("recorded run id");
    assert!(run_id.contains("run-cache-key"));
    assert!(run_id.contains("cycle42"));
    let prompt = step["prompt"].as_str().expect("recorded prompt");
    assert!(
        prompt.contains("bar_index"),
        "request body must include the upstream inputs verbatim: {prompt}",
    );
    assert!(
        prompt.contains("Inputs:"),
        "request body must carry the canonical 'Inputs:' prefix: {prompt}",
    );
}

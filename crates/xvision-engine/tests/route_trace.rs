use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tempfile::TempDir;
use xvision_agent_client::AgentClient;
use xvision_core::config::{AgentRuntime, ProviderEntry, ProviderKind};
use xvision_engine::agent::dispatch_capability::ClineDispatchCtx;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::observability::{normalize_route_lane_label, ObsEmitter};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::agents::{Capability, InputsPolicy};
use xvision_engine::strategies::agent_ref::{
    AgentRef, RouteBranch, RouteContextField, RouteDefinition, RouteGraphEdge, RouteTraceMode,
};
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_observability::{AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus};

fn text_response(text: &str) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

struct RecordingDispatch {
    canned: Mutex<Vec<LlmResponse>>,
}

impl RecordingDispatch {
    fn sequence(responses: Vec<LlmResponse>) -> Self {
        Self {
            canned: Mutex::new(responses),
        }
    }
}

#[async_trait::async_trait]
impl LlmDispatch for RecordingDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let mut q = self.canned.lock().unwrap();
        if q.len() > 1 {
            Ok(q.remove(0))
        } else {
            Ok(q.first().cloned().unwrap_or_else(|| {
                text_response(r#"{"action":"hold","conviction":0.1,"justification":"fallback"}"#)
            }))
        }
    }
}

fn fixture_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01H8N7ZROUTE".into(),
            display_name: "Route Trace Test".into(),
            plain_summary: "trace route decisions".into(),
            creator: "@test".into(),
            template: "custom".into(),
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
        agents: Vec::new(),
        pipeline: PipelineDef::sequential(),
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

fn agent_ref(role: &str, activates: Capability) -> AgentRef {
    AgentRef {
        agent_id: format!("{role}-agent"),
        role: role.into(),
        activates: Some(activates),
        prompt: String::new(),
        model_override: None,
        checkpoint: None,
        veto: None,
    }
}

fn resolved_agent(role: &str) -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: role.into(),
        slot: LLMSlot {
            role: role.into(),
            attested_with: "anthropic.claude-sonnet-4-6".into(),
            allowed_tools: vec!["ohlcv".into()],
            provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-6".into()),
        },
        system_prompt: format!("role={role}"),
        max_tokens: Some(4096),
        max_wall_ms: None,
        temperature: None,
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: format!("{role}-agent"),
        noop_skip: false,
        nano: None,
    }
}

fn route_definition() -> RouteDefinition {
    RouteDefinition {
        router_role: "router".into(),
        branches: vec![
            RouteBranch {
                target_role: "trend_trader".into(),
            },
            RouteBranch {
                target_role: "range_trader".into(),
            },
        ],
        graph_edges: vec![RouteGraphEdge {
            from_role: "trend_trader".into(),
            to_role: "risk_reviewer".into(),
            condition: None,
        }],
        context_fields: vec![RouteContextField::AvailableTargets],
        trace_mode: RouteTraceMode::Compact,
    }
}

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

async fn spawn_mock_cline(decision_json: &str, record_steps_path: &Path) -> (ClineDispatchCtx, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    std::fs::write(
        dir.path().join("agentd.sock.cfg"),
        serde_json::to_vec(&serde_json::json!({
            "decisionJson": decision_json,
            "recordStepsPath": record_steps_path,
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

async fn collect_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let events = recorder.snapshot().await;
        if !events.is_empty() {
            return events;
        }
    }
    recorder.snapshot().await
}

#[test]
fn route_lane_labels_are_product_facing() {
    assert_eq!(normalize_route_lane_label("backward test"), "backtest");
    assert_eq!(normalize_route_lane_label("backtest"), "backtest");
    assert_eq!(normalize_route_lane_label("forward"), "forward test");
    assert_eq!(normalize_route_lane_label("optimizer"), "optimizer");
    assert_eq!(normalize_route_lane_label("live"), "live trading");
}

#[tokio::test]
async fn routed_pipeline_emits_compact_route_decision_and_skip_events() {
    let mut strategy = fixture_strategy();
    strategy.agents = vec![
        agent_ref("router", Capability::Router),
        agent_ref("trend_trader", Capability::Trader),
        agent_ref("range_trader", Capability::Trader),
        agent_ref("risk_reviewer", Capability::Trader),
    ];
    strategy.pipeline.route = Some(route_definition());

    let agent_slots = ["router", "trend_trader", "range_trader", "risk_reviewer"]
        .into_iter()
        .map(resolved_agent)
        .collect::<Vec<_>>();
    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(
        r#"{"target_role":"trend_trader","reason":"trend regime"}"#,
        &record_path,
    )
    .await;
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let obs = ObsEmitter::new(bus.clone(), "run-route-trace");
    let dispatch = Arc::new(RecordingDispatch::sequence(vec![
        text_response(r#"{"action":"buy","conviction":0.8,"justification":"trend"}"#),
        text_response(r#"{"action":"approve","conviction":0.7,"justification":"risk ok"}"#),
    ]));

    run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools: Arc::new(ToolRegistry::default_with_builtins()),
        obs: Some(obs),
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-route-trace".into(),
        scenario_id: "scenario-route-trace".into(),
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
    .expect("route pipeline should execute selected branch and downstream review");

    let events = collect_events(&bus, &recorder).await;
    let route_events = events
        .into_iter()
        .filter_map(|event| match event {
            RunEvent::EngineEvent(event) if event.kind.starts_with("route.") => Some(event),
            _ => None,
        })
        .collect::<Vec<_>>();

    let decision = route_events
        .iter()
        .find(|event| event.kind == "route.decision")
        .expect("route runtime must emit a compact route.decision event");
    let payload: serde_json::Value = serde_json::from_str(decision.payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(payload["event"], "route.decision");
    assert_eq!(payload["lane"], "backtest");
    assert_eq!(payload["lane_label"], "backtest");
    assert_eq!(payload["router_role"], "router");
    assert_eq!(payload["selected_target_role"], "trend_trader");
    assert_eq!(
        payload["selected_path"],
        serde_json::json!(["router", "trend_trader", "risk_reviewer"])
    );
    assert_eq!(
        payload["skipped_target_roles"],
        serde_json::json!(["range_trader"])
    );
    assert_eq!(payload["summary"], "trend regime");
    assert_eq!(payload["final_trader_role"], "trend_trader");
    assert_eq!(payload["final_action"], "unknown");
    assert_eq!(payload["actual_vs_intended"], "matched");
    assert_eq!(
        payload["intended_route"]["branch_targets"],
        serde_json::json!(["trend_trader", "range_trader"])
    );

    let skip = route_events
        .iter()
        .find(|event| event.kind == "route.skip")
        .expect("route runtime must emit a skip event for the unselected sibling branch");
    let skip_payload: serde_json::Value =
        serde_json::from_str(skip.payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(skip_payload["target_role"], "range_trader");
    assert_eq!(skip_payload["reason"], "unselected_branch_target");
}

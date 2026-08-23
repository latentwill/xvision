use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use xvision_agent_client::AgentClient;
use xvision_core::config::{AgentRuntime, ProviderEntry, ProviderKind};
use xvision_engine::agent::dispatch_capability::ClineDispatchCtx;
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch, StopReason,
};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, PipelineOutputs, ResolvedAgentSlot};
use xvision_engine::agents::Capability;
use xvision_engine::strategies::agent_ref::{
    AgentRef, EdgePredicate, RouteBranch, RouteContextField, RouteDefinition, RouteGraphEdge, RouteTraceMode,
};
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;

fn fixture_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01H8N7ZPIPE".into(),
            display_name: "Pipe Test".into(),
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
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: Some(LLMSlot {
            role: "regime".into(),
            attested_with: "mock".into(),
            allowed_tools: vec!["ohlcv".into()],
            provider: None,
            model: None,
        }),
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "mock".into(),
            allowed_tools: vec!["ohlcv".into()],
            provider: None,
            model: None,
        }),
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
    requests: Mutex<Vec<LlmRequest>>,
}

impl RecordingDispatch {
    fn sequence(responses: Vec<LlmResponse>) -> Self {
        Self {
            canned: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<LlmRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LlmDispatch for RecordingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.requests.lock().unwrap().push(req);

        let mut q = self.canned.lock().unwrap();
        if q.len() > 1 {
            Ok(q.remove(0))
        } else {
            Ok(q.first().cloned().unwrap_or_else(|| text_response("ok")))
        }
    }
}

fn request_text(req: &LlmRequest) -> String {
    req.messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn request_role(req: &LlmRequest) -> String {
    req.system_prompt
        .lines()
        .find_map(|line| line.strip_prefix("role="))
        .expect("test requests carry a role marker in the system prompt")
        .to_string()
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
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: format!("{role}-agent"),
        noop_skip: false,
        nano: None,
    }
}

fn fixture_strategy_with_agents(agents: Vec<AgentRef>) -> Strategy {
    let mut strategy = fixture_strategy();
    strategy.agents = agents;
    strategy.regime_slot = None;
    strategy.trader_slot = None;
    strategy.pipeline = PipelineDef::sequential();
    strategy
}

fn route_definition(
    branch_targets: &[&str],
    graph_edges: Vec<RouteGraphEdge>,
    context_fields: Vec<RouteContextField>,
) -> RouteDefinition {
    RouteDefinition {
        router_role: "router".into(),
        branches: branch_targets
            .iter()
            .map(|target_role| RouteBranch {
                target_role: (*target_role).into(),
            })
            .collect(),
        graph_edges,
        context_fields,
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
        .expect("spawn mock sidecar (node must be on PATH)");
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

fn recorded_cline_steps(path: &Path) -> Vec<serde_json::Value> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("mock agentd step record is valid JSON"))
        .collect()
}

fn recorded_cline_roles(path: &Path) -> Vec<String> {
    recorded_cline_steps(path)
        .into_iter()
        .map(|step| {
            step.get("run_id")
                .and_then(|v| v.as_str())
                .and_then(|run_id| run_id.split("::").nth(1))
                .expect("mock agentd run_id includes slot role")
                .to_string()
        })
        .collect()
}

fn first_recorded_cline_prompt(path: &Path) -> String {
    recorded_cline_steps(path)
        .into_iter()
        .next()
        .and_then(|step| step.get("prompt").and_then(|v| v.as_str()).map(str::to_string))
        .expect("mock agentd recorded at least one prompt")
}

fn pipeline_inputs<'a>(
    strategy: &'a Strategy,
    agent_slots: &'a [ResolvedAgentSlot],
    seed_inputs: serde_json::Value,
    dispatch: Arc<dyn LlmDispatch>,
    cline: Option<ClineDispatchCtx>,
) -> PipelineInputs<'a> {
    PipelineInputs {
        strategy,
        agent_slots,
        seed_inputs,
        dispatch,
        tools: Arc::new(ToolRegistry::default_with_builtins()),
        obs: None,
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: format!("route-builder-task-5-{}", uuid::Uuid::new_v4()),
        scenario_id: "route-builder-task-5-scenario".into(),
        cycle_idx: 0,
        provider_catalogs: std::collections::HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: if cline.is_some() {
            AgentRuntime::Cline
        } else {
            Default::default()
        },
        cline,
        model_call_span_id: None,
    }
}

#[tokio::test]
async fn routed_pipeline_does_not_fall_through_to_unselected_branch_targets() {
    let agents = vec![
        agent_ref("router", Capability::Router),
        agent_ref("trend_trader", Capability::Trader),
        agent_ref("range_trader", Capability::Trader),
        agent_ref("trader", Capability::Trader),
    ];
    let mut strategy = fixture_strategy_with_agents(agents);
    strategy.pipeline.route = Some(route_definition(
        &["trend_trader", "range_trader"],
        vec![
            RouteGraphEdge {
                from_role: "trend_trader".into(),
                to_role: "trader".into(),
                condition: None,
            },
            RouteGraphEdge {
                from_role: "range_trader".into(),
                to_role: "trader".into(),
                condition: None,
            },
        ],
        vec![
            RouteContextField::MarketSnapshot,
            RouteContextField::ToolState,
            RouteContextField::AvailableTargets,
        ],
    ));
    let agent_slots = ["router", "trend_trader", "range_trader", "trader"]
        .into_iter()
        .map(resolved_agent)
        .collect::<Vec<_>>();

    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(r#"{"target_agent_ref_index":1}"#, &record_path).await;
    let dispatch = Arc::new(RecordingDispatch::sequence(Vec::new()));
    let pipeline_dispatch: Arc<dyn LlmDispatch> = dispatch.clone();

    let outs = run_pipeline(pipeline_inputs(
        &strategy,
        &agent_slots,
        serde_json::json!({"market_snapshot": {"symbol": "BTC/USD"}}),
        pipeline_dispatch,
        Some(cline),
    ))
    .await
    .expect("routed pipeline should execute selected path");

    let roles = recorded_cline_roles(&record_path);
    assert_eq!(
        roles,
        vec!["router", "trend_trader", "trader"],
        "Route Builder runtime must execute the router-selected branch target, continue to the merge/decision target, and skip unselected sibling branch targets"
    );
    assert!(
        !roles.iter().any(|role| role == "range_trader"),
        "unselected sibling branch target must not receive a model call"
    );
    assert!(
        outs.trader.is_some(),
        "selected route path must reach the decision trader"
    );
}

#[tokio::test]
async fn routed_pipeline_executes_supported_conditioned_graph_edge() {
    let agents = vec![
        agent_ref("router", Capability::Router),
        agent_ref("regime_filter", Capability::Filter),
        agent_ref("trend_trader", Capability::Trader),
        agent_ref("range_trader", Capability::Trader),
        agent_ref("trader", Capability::Trader),
    ];
    let mut strategy = fixture_strategy_with_agents(agents);
    strategy.pipeline.route = Some(route_definition(
        &["regime_filter"],
        vec![
            RouteGraphEdge {
                from_role: "regime_filter".into(),
                to_role: "trend_trader".into(),
                condition: Some(EdgePredicate::Eq {
                    signal_field: "regime".into(),
                    value: serde_json::json!("trend"),
                }),
            },
            RouteGraphEdge {
                from_role: "regime_filter".into(),
                to_role: "range_trader".into(),
                condition: Some(EdgePredicate::Eq {
                    signal_field: "regime".into(),
                    value: serde_json::json!("range"),
                }),
            },
            RouteGraphEdge {
                from_role: "trend_trader".into(),
                to_role: "trader".into(),
                condition: None,
            },
            RouteGraphEdge {
                from_role: "range_trader".into(),
                to_role: "trader".into(),
                condition: None,
            },
        ],
        vec![RouteContextField::AvailableTargets],
    ));
    let agent_slots = [
        "router",
        "regime_filter",
        "trend_trader",
        "range_trader",
        "trader",
    ]
    .into_iter()
    .map(resolved_agent)
    .collect::<Vec<_>>();

    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(r#"{"target_agent_ref_index":1}"#, &record_path).await;
    let dispatch = Arc::new(RecordingDispatch::sequence(vec![text_response(
        r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"bar"}"#,
    )]));
    let pipeline_dispatch: Arc<dyn LlmDispatch> = dispatch.clone();

    let outs = run_pipeline(pipeline_inputs(
        &strategy,
        &agent_slots,
        serde_json::json!({}),
        pipeline_dispatch,
        Some(cline),
    ))
    .await
    .expect("routed graph-edge pipeline should execute selected route path");

    let filter_roles = dispatch.requests().iter().map(request_role).collect::<Vec<_>>();
    assert_eq!(filter_roles, vec!["regime_filter"]);
    let roles = recorded_cline_roles(&record_path);
    assert_eq!(
        roles,
        vec!["router", "trend_trader", "trader"],
        "Route graph-edge predicates must dispatch only the matching branch target and still continue to the downstream decision target"
    );
    assert!(
        !roles.iter().any(|role| role == "range_trader"),
        "non-matching route graph-edge branch target must be gated out"
    );
    assert!(
        outs.trader.is_some(),
        "conditioned route path must reach the decision trader"
    );
}

#[tokio::test]
async fn no_route_present_preserves_sequential_agent_execution() {
    let agents = vec![
        agent_ref("scout", Capability::Trader),
        agent_ref("risk_reviewer", Capability::Trader),
        agent_ref("trader", Capability::Trader),
    ];
    let strategy = fixture_strategy_with_agents(agents);
    let agent_slots = ["scout", "risk_reviewer", "trader"]
        .into_iter()
        .map(resolved_agent)
        .collect::<Vec<_>>();
    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(
        r#"{"action":"hold","conviction":0.2,"justification":"legacy"}"#,
        &record_path,
    )
    .await;
    let dispatch = Arc::new(RecordingDispatch::sequence(Vec::new()));
    let pipeline_dispatch: Arc<dyn LlmDispatch> = dispatch.clone();

    let outs = run_pipeline(pipeline_inputs(
        &strategy,
        &agent_slots,
        serde_json::json!({}),
        pipeline_dispatch,
        Some(cline),
    ))
    .await
    .expect("legacy sequential pipeline should still run");

    let roles = recorded_cline_roles(&record_path);
    assert_eq!(
        roles,
        vec!["scout", "risk_reviewer", "trader"],
        "absence of a RouteDefinition must preserve legacy sequential dispatch order"
    );
    assert!(
        outs.trader.is_some(),
        "legacy sequential trader output remains surfaced"
    );
}

#[tokio::test]
async fn router_input_includes_available_targets_and_configured_context_fields() {
    let agents = vec![
        agent_ref("router", Capability::Router),
        agent_ref("trend_trader", Capability::Trader),
        agent_ref("range_trader", Capability::Trader),
    ];
    let mut strategy = fixture_strategy_with_agents(agents);
    strategy.pipeline.route = Some(route_definition(
        &["trend_trader", "range_trader"],
        Vec::new(),
        vec![
            RouteContextField::AvailableTargets,
            RouteContextField::ToolState,
            RouteContextField::RegimeSummary,
        ],
    ));
    let agent_slots = ["router", "trend_trader", "range_trader"]
        .into_iter()
        .map(resolved_agent)
        .collect::<Vec<_>>();
    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(r#"{"target_agent_ref_index":1}"#, &record_path).await;
    let dispatch = Arc::new(RecordingDispatch::sequence(Vec::new()));
    let pipeline_dispatch: Arc<dyn LlmDispatch> = dispatch.clone();

    let _outs = run_pipeline(pipeline_inputs(
        &strategy,
        &agent_slots,
        serde_json::json!({
            "market_snapshot": {"secret_price": 42},
            "tool_state": {"ohlcv": "ready"},
            "regime_summary": {"label": "trend"},
        }),
        pipeline_dispatch,
        Some(cline),
    ))
    .await
    .expect("router should run with configured route context");

    let router_prompt = first_recorded_cline_prompt(&record_path);
    assert!(
        router_prompt.contains("\"route_targets\""),
        "router input must advertise available Route Builder branch targets: {router_prompt}"
    );
    assert!(
        router_prompt.contains("\"trend_trader\"") && router_prompt.contains("\"range_trader\""),
        "router input route_targets must include configured branch target roles: {router_prompt}"
    );
    assert!(
        router_prompt.contains("\"route_context\""),
        "router input must isolate configured route_context fields: {router_prompt}"
    );
    assert!(
        router_prompt.contains("\"tool_state\"") && router_prompt.contains("\"regime_summary\""),
        "router input route_context must include explicitly configured context fields: {router_prompt}"
    );
    assert!(
        !router_prompt.contains("secret_price"),
        "router input must not leak omitted context fields into route_context: {router_prompt}"
    );
}

#[tokio::test]
async fn routed_pipeline_rejects_legacy_index_that_is_not_a_configured_branch_target() {
    let agents = vec![
        agent_ref("router", Capability::Router),
        agent_ref("trend_trader", Capability::Trader),
        agent_ref("range_trader", Capability::Trader),
        agent_ref("rogue_trader", Capability::Trader),
        agent_ref("trader", Capability::Trader),
    ];
    let mut strategy = fixture_strategy_with_agents(agents);
    strategy.pipeline.route = Some(route_definition(
        &["trend_trader", "range_trader"],
        Vec::new(),
        vec![RouteContextField::AvailableTargets],
    ));
    let agent_slots = ["router", "trend_trader", "range_trader", "rogue_trader", "trader"]
        .into_iter()
        .map(resolved_agent)
        .collect::<Vec<_>>();
    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(r#"{"target_agent_ref_index":3}"#, &record_path).await;
    let dispatch = Arc::new(RecordingDispatch::sequence(Vec::new()));
    let pipeline_dispatch: Arc<dyn LlmDispatch> = dispatch.clone();

    let err = run_pipeline(pipeline_inputs(
        &strategy,
        &agent_slots,
        serde_json::json!({}),
        pipeline_dispatch,
        Some(cline),
    ))
    .await
    .expect_err("route runtime must reject an unreachable/unconfigured branch target");

    assert!(
        err.to_string().contains("configured branch target")
            || err.to_string().contains("route")
            || err.to_string().contains("unreachable"),
        "route error should explain that the router selected an unconfigured/unreachable branch target, got: {err}"
    );
    let roles = recorded_cline_roles(&record_path);
    assert_eq!(
        roles,
        vec!["router"],
        "invalid route output must stop on a route diagnostic instead of silently executing the wrong branch"
    );
}

#[tokio::test]
async fn two_slot_pipeline_chains_outputs() {
    let strategy = fixture_strategy();
    let dispatch = Arc::new(RecordingDispatch::sequence(vec![
        text_response(r#"{"stage":"regime","regime_id":"range-bound-42"}"#),
        text_response(r#"{"action":"hold","conviction":0.12,"justification":"uses range-bound-42"}"#),
    ]));
    let pipeline_dispatch: Arc<dyn LlmDispatch> = dispatch.clone();
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs: PipelineOutputs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &[],
        seed_inputs: serde_json::json!({"ohlcv_history": [], "indicator_panel": {}}),
        dispatch: pipeline_dispatch,
        tools,
        obs: None,
        memory_recorder: None,

        scenario_start: None,

        source_window_start: None,

        source_window_end: None,

        run_id: String::new(),

        scenario_id: String::new(),

        cycle_idx: 0,
        provider_catalogs: std::collections::HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .unwrap();
    assert_eq!(
        outs.regime.as_ref().map(LlmResponse::text).as_deref(),
        Some(r#"{"stage":"regime","regime_id":"range-bound-42"}"#)
    );
    assert_eq!(
        outs.trader.as_ref().map(LlmResponse::text).as_deref(),
        Some(r#"{"action":"hold","conviction":0.12,"justification":"uses range-bound-42"}"#)
    );
    assert!(outs.total_input_tokens > 0);
    assert!(outs.total_output_tokens > 0);

    let requests = dispatch.requests();
    assert_eq!(requests.len(), 2);

    let trader_request = request_text(&requests[1]);
    assert!(trader_request.contains("regime_output"));
    assert!(trader_request.contains("range-bound-42"));
}

#[tokio::test]
async fn skips_missing_optional_slots() {
    let mut strategy = fixture_strategy();
    strategy.regime_slot = None; // skip
    let dispatch = Arc::new(MockDispatch::echo(r#"{"ok":true}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &[],
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,

        scenario_start: None,

        source_window_start: None,

        source_window_end: None,

        run_id: String::new(),

        scenario_id: String::new(),

        cycle_idx: 0,
        provider_catalogs: std::collections::HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .unwrap();
    assert!(outs.regime.is_none());
    assert!(outs.trader.is_some());
}

#[tokio::test]
async fn resolved_agent_pipeline_uses_trader_role_as_decision_output() {
    let mut strategy = fixture_strategy();
    strategy.regime_slot = None;
    strategy.trader_slot = None;
    strategy.pipeline = PipelineDef::sequential();
    let agent_slots = vec![resolved_agent("scout"), resolved_agent("trader")];

    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(
        r#"{"action":"hold","conviction":0.1,"justification":"mock"}"#,
        &record_path,
    )
    .await;
    let dispatch = Arc::new(MockDispatch::echo(r#"{"action":"hold"}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,

        scenario_start: None,

        source_window_start: None,

        source_window_end: None,

        run_id: String::new(),

        scenario_id: String::new(),

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
    .unwrap();
    assert!(outs.regime.is_none());
    assert!(outs.trader.is_some());
    assert!(outs.total_input_tokens > 0);
}

#[tokio::test]
async fn resolved_agent_pipeline_does_not_treat_non_trader_as_decision_output() {
    let mut strategy = fixture_strategy();
    strategy.regime_slot = None;
    strategy.trader_slot = None;
    strategy.pipeline = PipelineDef::sequential();
    let agent_slots = vec![resolved_agent("scout"), resolved_agent("final_decider")];

    let record_dir = TempDir::new().expect("record dir");
    let record_path = record_dir.path().join("steps.jsonl");
    let (cline, _agentd_dir) = spawn_mock_cline(
        r#"{"action":"hold","conviction":0.1,"justification":"mock"}"#,
        &record_path,
    )
    .await;
    let dispatch = Arc::new(MockDispatch::echo(r#"{"action":"hold"}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools,
        obs: None,
        memory_recorder: None,

        scenario_start: None,

        source_window_start: None,

        source_window_end: None,

        run_id: String::new(),

        scenario_id: String::new(),

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
    .unwrap();

    assert!(outs.trader.is_none());
    assert!(outs.total_input_tokens > 0);
    assert!(outs.total_output_tokens > 0);
}

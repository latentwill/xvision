use std::sync::{Arc, Mutex};
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch, StopReason,
};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, PipelineOutputs, ResolvedAgentSlot};
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
    let agent_slots = vec![
        ResolvedAgentSlot {
            role: "scout".into(),
            slot: LLMSlot {
                role: "scout".into(),
                attested_with: "mock".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            },
            system_prompt: String::new(),
            max_tokens: Some(4096),
            max_wall_ms: None,
            temperature: None,
            inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
            nano: None,
        },
        ResolvedAgentSlot {
            role: "trader".into(),
            slot: LLMSlot {
                role: "trader".into(),
                attested_with: "mock".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            },
            system_prompt: String::new(),
            max_tokens: Some(4096),
            max_wall_ms: None,
            temperature: None,
            inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
            nano: None,
        },
    ];

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
        runtime: Default::default(),
        cline: None,
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
    let agent_slots = vec![
        ResolvedAgentSlot {
            role: "scout".into(),
            slot: LLMSlot {
                role: "scout".into(),
                attested_with: "mock".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            },
            system_prompt: String::new(),
            max_tokens: Some(4096),
            max_wall_ms: None,
            temperature: None,
            inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
            nano: None,
        },
        ResolvedAgentSlot {
            role: "final_decider".into(),
            slot: LLMSlot {
                role: "final_decider".into(),
                attested_with: "mock".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            },
            system_prompt: String::new(),
            max_tokens: Some(4096),
            max_wall_ms: None,
            temperature: None,
            inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
            nano: None,
        },
    ];

    let dispatch = Arc::new(MockDispatch::sequence(vec![
        text_response(r#"{"stage":"first"}"#),
        text_response(r#"{"stage":"second"}"#),
    ]));
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
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .unwrap();

    assert!(outs.trader.is_none());
    assert_eq!(outs.total_input_tokens, 2);
    assert_eq!(outs.total_output_tokens, 2);
}

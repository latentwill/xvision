//! Phase C — LLM Filter dispatch round-trip.
//!
//! Covers acceptance test #1: a strategy with one LLM Filter slot +
//! one Trader; assert the Filter produces a `FilterSignal`, the Trader
//! sees it under `filter_signals[<role>]`, and an edge predicate `Eq`
//! on the payload gates Trader invocation correctly.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::agents::Capability;
use xvision_engine::strategies::agent_ref::{AgentRef, EdgePredicate};
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, PipelineEdge, PipelineKind, Strategy};
use xvision_engine::tools::ToolRegistry;

/// Routes the canned-response selection by the slot's role so a single
/// pipeline can dispatch a Filter + Trader with different bodies on
/// the same `LlmDispatch`. The `last_request_role` mutex records the
/// most recent dispatch's role so the test can verify which path was
/// taken.
struct RoleAwareDispatch {
    seen: Mutex<Vec<LlmRequest>>,
    filter_text: String,
    trader_text: String,
}

impl RoleAwareDispatch {
    fn new(filter_text: impl Into<String>, trader_text: impl Into<String>) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            filter_text: filter_text.into(),
            trader_text: trader_text.into(),
        }
    }
    fn requests(&self) -> Vec<LlmRequest> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmDispatch for RoleAwareDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        // The Filter dispatcher wraps the operator's system_prompt
        // with "You are a Filter..." — we detect that to decide which
        // canned response to emit.
        let is_filter = req.system_prompt.contains("You are a Filter");
        let text = if is_filter {
            self.filter_text.clone()
        } else {
            self.trader_text.clone()
        };
        self.seen.lock().unwrap().push(req);
        Ok(LlmResponse {
            content: vec![ContentBlock::Text { text }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 3,
            output_tokens: 5,
        })
    }
}

fn fixture_strategy(agents: Vec<AgentRef>) -> Strategy {
    fixture_strategy_with_pipeline(agents, PipelineKind::Sequential, Vec::new())
}

fn fixture_strategy_with_pipeline(
    agents: Vec<AgentRef>,
    kind: PipelineKind,
    edges: Vec<PipelineEdge>,
) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01HZFLT".into(),
            display_name: "FilterDispatchTest".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 15,
            attested_with: vec!["mock".into()],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents,
        pipeline: PipelineDef { kind, edges },
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
    ResolvedAgentSlot {
        role: role.into(),
        slot: LLMSlot {
            role: role.into(),
            attested_with: "mock".into(),
            allowed_tools: Vec::new(),
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
    }
}

#[tokio::test]
async fn filter_signal_flows_into_trader_briefing() {
    let agents = vec![
        AgentRef {
            agent_id: "01HZF".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt_override: None,
            model_override: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
        },
    ];
    let strategy = fixture_strategy(agents);
    let slots = vec![resolved("regime_filter"), resolved("trader")];

    let dispatch = Arc::new(RoleAwareDispatch::new(
        r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"bar"}"#,
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
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .expect("pipeline runs");

    assert!(outs.trader.is_some(), "trader output populated");

    // Two LLM dispatches: one for the Filter, one for the Trader.
    let requests = dispatch.requests();
    assert_eq!(requests.len(), 2, "expected Filter + Trader dispatches");

    // The Trader's request body must carry `filter_signals.regime_filter`
    // populated from the Filter's parsed payload.
    let trader_req = requests
        .iter()
        .find(|r| !r.system_prompt.contains("You are a Filter"))
        .expect("trader request present");
    let body = serde_json::to_string(&trader_req.messages).unwrap();
    assert!(
        body.contains("filter_signals"),
        "Trader briefing must include `filter_signals` map: {body}"
    );
    assert!(
        body.contains("regime_filter"),
        "Trader briefing must include the Filter's role key: {body}"
    );
    assert!(
        body.contains("trend"),
        "Trader briefing must include the Filter payload value: {body}"
    );
    assert_eq!(outs.total_input_tokens, 6, "Filter + Trader token accounting");
    assert_eq!(outs.total_output_tokens, 10, "Filter + Trader token accounting");
}

#[tokio::test]
async fn malformed_filter_output_does_not_panic_and_emits_null_signal() {
    // Filter LLM returns malformed JSON; the dispatcher records the
    // parse error and the pipeline continues. The Trader still runs
    // and sees a `filter_signals` map with a null-payload entry —
    // edge predicates against the missing field return false (the
    // existing "unknown field → false" rule from edge_predicate).
    let agents = vec![
        AgentRef {
            agent_id: "01HZF".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt_override: None,
            model_override: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
        },
    ];
    let strategy = fixture_strategy(agents);
    let slots = vec![resolved("regime_filter"), resolved("trader")];

    let dispatch = Arc::new(RoleAwareDispatch::new(
        "this is not json",
        r#"{"action":"hold","conviction":0.1,"justification":"r"}"#,
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
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .expect("pipeline runs even with malformed Filter");

    assert!(
        outs.trader.is_some(),
        "Trader still runs with a null-payload Filter signal"
    );
}

#[tokio::test]
async fn graph_predicate_true_invokes_trader() {
    let agents = vec![
        AgentRef {
            agent_id: "01HZF".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt_override: None,
            model_override: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
        },
    ];
    let strategy = fixture_strategy_with_pipeline(
        agents,
        PipelineKind::Graph,
        vec![PipelineEdge {
            from_role: "regime_filter".into(),
            to_role: "trader".into(),
            condition: Some(EdgePredicate::Eq {
                signal_field: "regime".into(),
                value: serde_json::json!("trend"),
            }),
        }],
    );
    let slots = vec![resolved("regime_filter"), resolved("trader")];
    let dispatch = Arc::new(RoleAwareDispatch::new(
        r#"{"name":"regime_filter","payload":{"regime":"trend"},"granularity":"bar"}"#,
        r#"{"action":"long_open","conviction":0.6,"justification":"r"}"#,
    ));

    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
        tools: Arc::new(ToolRegistry::default_with_builtins()),
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
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .expect("graph pipeline runs");

    assert!(outs.trader.is_some(), "matching predicate should invoke Trader");
    assert_eq!(dispatch.requests().len(), 2, "Filter + Trader dispatches");
}

#[tokio::test]
async fn graph_predicate_false_skips_trader() {
    let agents = vec![
        AgentRef {
            agent_id: "01HZF".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt_override: None,
            model_override: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
        },
    ];
    let strategy = fixture_strategy_with_pipeline(
        agents,
        PipelineKind::Graph,
        vec![PipelineEdge {
            from_role: "regime_filter".into(),
            to_role: "trader".into(),
            condition: Some(EdgePredicate::Eq {
                signal_field: "regime".into(),
                value: serde_json::json!("trend"),
            }),
        }],
    );
    let slots = vec![resolved("regime_filter"), resolved("trader")];
    let dispatch = Arc::new(RoleAwareDispatch::new(
        r#"{"name":"regime_filter","payload":{"regime":"range"},"granularity":"bar"}"#,
        r#"{"action":"long_open","conviction":0.6,"justification":"r"}"#,
    ));

    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: serde_json::json!({}),
        dispatch: dispatch.clone(),
        tools: Arc::new(ToolRegistry::default_with_builtins()),
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
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .expect("graph pipeline runs");

    let trader = outs.trader.expect("graph skip synthesizes a hold response");
    assert!(
        trader.text().contains("trader_skipped_by_graph"),
        "skip response should explain graph gating: {}",
        trader.text(),
    );
    assert_eq!(dispatch.requests().len(), 1, "only Filter should dispatch");
}

#[tokio::test]
async fn filter_provider_error_aborts_pipeline() {
    struct FilterFails;

    #[async_trait]
    impl LlmDispatch for FilterFails {
        async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
            if req.system_prompt.contains("You are a Filter") {
                anyhow::bail!("provider unavailable");
            }
            Ok(LlmResponse {
                content: vec![ContentBlock::Text {
                    text: r#"{"action":"hold","conviction":0.1,"justification":"r"}"#.into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 3,
                output_tokens: 5,
            })
        }
    }

    let agents = vec![
        AgentRef {
            agent_id: "01HZF".into(),
            role: "regime_filter".into(),
            activates: Some(Capability::Filter),
            prompt_override: None,
            model_override: None,
        },
        AgentRef {
            agent_id: "01HZT".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
        },
    ];
    let strategy = fixture_strategy(agents);
    let slots = vec![resolved("regime_filter"), resolved("trader")];

    let err = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &slots,
        seed_inputs: serde_json::json!({}),
        dispatch: Arc::new(FilterFails),
        tools: Arc::new(ToolRegistry::default_with_builtins()),
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
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .expect_err("provider failures must not become null Filter signals");

    assert!(err.to_string().contains("provider unavailable"), "got: {err}");
}

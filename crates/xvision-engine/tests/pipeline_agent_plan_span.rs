//! WS-12 (`trace-obs-ws12`): the `agent.plan` topology span.
//!
//! `SpanKind::AgentPlan` (`agent.plan`) is defined in
//! `xvision_observability::types` but had ZERO producers — the trace
//! could not say which slots/roles/models ran for a decision. This test
//! pins the wiring: `run_pipeline` opens exactly ONE `agent.plan` span
//! at pipeline entry carrying the resolved topology (ordered stages with
//! `role` / `model?` / `capability?`), and finishes it after the stages
//! complete.
//!
//! Coverage:
//! 1. Legacy `regime_slot` + `trader_slot` path emits an `agent.plan`
//!    span whose topology lists both roles in order, opened + finished.
//! 2. The agent-slots (modern multi-stage) path emits an `agent.plan`
//!    span whose topology lists the slot roles in order.
//! 3. `obs: None` emits NO `agent.plan` span (and does not panic).

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;
use xvision_observability::{NoopRecorder, RunEvent, RunEventBus, SpanKind, SpanStartedEvent};

fn fixture_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01H8N7ZPLAN".into(),
            display_name: "Plan Test".into(),
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
            model: Some("mock-regime-model".into()),
        }),
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "mock".into(),
            allowed_tools: vec!["ohlcv".into()],
            provider: None,
            model: Some("mock-trader-model".into()),
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

async fn collect_events(bus: &RunEventBus, recorder: &NoopRecorder) -> Vec<RunEvent> {
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    recorder.snapshot().await
}

fn agent_plan_starts(events: &[RunEvent]) -> Vec<&SpanStartedEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if s.kind == SpanKind::AgentPlan => Some(s),
            _ => None,
        })
        .collect()
}

fn span_finished_ids(events: &[RunEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanFinished(f) => Some(f.span_id.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn legacy_pipeline_emits_agent_plan_span_with_topology() {
    let strategy = fixture_strategy();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![
        text_response(r#"{"stage":"regime"}"#),
        text_response(r#"{"action":"hold","conviction":0.1,"justification":"x"}"#),
    ]));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let emitter = ObsEmitter::new(bus.clone(), "run-legacy");

    run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &[],
        seed_inputs: serde_json::json!({"ohlcv_history": [], "indicator_panel": {}}),
        dispatch,
        tools,
        obs: Some(emitter),
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-legacy".into(),
        scenario_id: String::new(),
        cycle_idx: 0,
        provider_catalogs: HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .unwrap();

    let events = collect_events(&bus, &recorder).await;
    let plans = agent_plan_starts(&events);
    assert_eq!(
        plans.len(),
        1,
        "expected exactly one agent.plan span, got {}",
        plans.len()
    );
    let plan = plans[0];

    // The span is opened AND finished.
    let finished = span_finished_ids(&events);
    assert!(
        finished.contains(&plan.span_id),
        "agent.plan span {} was opened but never finished",
        plan.span_id
    );

    // Topology carries the resolved stages in pipeline order with roles
    // and models.
    let attrs: Value = serde_json::from_str(
        plan.attributes_json
            .as_deref()
            .expect("agent.plan has attributes"),
    )
    .expect("attributes_json is valid JSON");
    let topology = attrs
        .get("topology")
        .and_then(|t| t.as_array())
        .expect("topology array present on agent.plan attributes");
    let roles: Vec<&str> = topology
        .iter()
        .filter_map(|s| s.get("role").and_then(Value::as_str))
        .collect();
    assert_eq!(roles, vec!["regime", "trader"], "topology roles in order");

    let regime_model = topology[0].get("model").and_then(Value::as_str);
    assert_eq!(regime_model, Some("mock-regime-model"));
    let trader_model = topology[1].get("model").and_then(Value::as_str);
    assert_eq!(trader_model, Some("mock-trader-model"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_slots_pipeline_emits_agent_plan_span_with_topology() {
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
                model: Some("scout-model".into()),
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
                model: Some("trader-model".into()),
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

    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(r#"{"action":"hold"}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));
    let emitter = ObsEmitter::new(bus.clone(), "run-agent");

    // NOTE: the modern agent path (WU-6) requires the Cline sidecar to
    // dispatch a Trader slot, so this `run_pipeline` returns an `Err`
    // under the unit-test `MockDispatch`. That is fine for this test:
    // the `agent.plan` span is opened BEFORE the stages run, so it must
    // still be emitted (with the resolved topology) AND finished even
    // though the dispatch later errors. We assert on the topology + the
    // close on the error path; the dispatch error itself is expected.
    let result = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools,
        obs: Some(emitter),
        memory_recorder: None,
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: "run-agent".into(),
        scenario_id: String::new(),
        cycle_idx: 0,
        provider_catalogs: HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await;
    // Either path is acceptable for the span assertions below; in the
    // current sidecar-required build this is an Err, but the span is
    // emitted regardless.
    let _ = result;

    let events = collect_events(&bus, &recorder).await;
    let plans = agent_plan_starts(&events);
    assert_eq!(
        plans.len(),
        1,
        "expected exactly one agent.plan span on the agent path, got {}",
        plans.len()
    );
    let plan = plans[0];

    let finished = span_finished_ids(&events);
    assert!(
        finished.contains(&plan.span_id),
        "agent.plan span {} was opened but never finished",
        plan.span_id
    );

    let attrs: Value = serde_json::from_str(
        plan.attributes_json
            .as_deref()
            .expect("agent.plan has attributes"),
    )
    .expect("attributes_json is valid JSON");
    let topology = attrs
        .get("topology")
        .and_then(|t| t.as_array())
        .expect("topology array present on agent.plan attributes");
    let roles: Vec<&str> = topology
        .iter()
        .filter_map(|s| s.get("role").and_then(Value::as_str))
        .collect();
    assert_eq!(roles, vec!["scout", "trader"], "agent topology roles in order");
    assert_eq!(
        topology[0].get("model").and_then(Value::as_str),
        Some("scout-model")
    );
    assert_eq!(
        topology[1].get("model").and_then(Value::as_str),
        Some("trader-model")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_emitter_emits_no_agent_plan_span() {
    let strategy = fixture_strategy();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(vec![
        text_response(r#"{"stage":"regime"}"#),
        text_response(r#"{"action":"hold","conviction":0.1,"justification":"x"}"#),
    ]));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    // obs: None — the no-op path must not panic and must emit nothing.
    let recorder = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![recorder.clone()]));

    run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &[],
        seed_inputs: serde_json::json!({"ohlcv_history": [], "indicator_panel": {}}),
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
        provider_catalogs: HashMap::new(),
        filter_ctx: None,
        trace_attrs: None,
        recorder: None,
        runtime: Default::default(),
        cline: None,
        model_call_span_id: None,
    })
    .await
    .unwrap();

    let events = collect_events(&bus, &recorder).await;
    let plans = agent_plan_starts(&events);
    assert!(
        plans.is_empty(),
        "expected NO agent.plan span when obs is None, got {}",
        plans.len()
    );
}

//! Wiring coverage for the `indicator-tool-wiring` track (2026-05-22).
//!
//! Strategy templates declare `required_tools: ["ohlcv", "indicator_panel"]`
//! (see `crates/xvision-engine/src/strategies/templates.rs`) and the
//! `ToolName::new("indicator_panel")` registration exists at
//! `crates/xvision-engine/src/tools/indicators.rs`. But the agent-loop
//! dispatch path historically lost the strategy's tool surface because
//! `agent_slot_to_llm_slot` hard-coded `allowed_tools: Vec::new()` — the
//! LLM call therefore shipped `"tools": []` and the trader had no way to
//! request indicators on demand.
//!
//! This test file pins the three acceptance points from
//! `team/contracts/indicator-tool-wiring.md`:
//!
//! (a) The dispatched `tools` array carries an `indicator_panel` schema
//!     entry when the strategy manifest's `required_tools` includes it
//!     (templates already declare this; we just rely on the
//!     `run_agent_pipeline` bridge populating the slot's allowed_tools
//!     from the strategy).
//! (b) A fixture trader response invoking `indicator_panel` actually
//!     routes through the tool registry; the result feeds back into the
//!     next dispatch turn as a `ToolResult` block.
//! (c) The tool execution emits a dedicated `tool.call` span plus a
//!     `tool.validate_input` + `tool.validate_output` span pair carrying
//!     `tool_name = "indicator_panel"`.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::agent::observability::ObsEmitter;
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::agents::InputsPolicy;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{AgentRef, PipelineDef, Strategy};
use xvision_engine::tools::{Tool, ToolName, ToolRegistry};
use xvision_filters::ActivationMode;
use xvision_observability::{AgentRunRecorder, NoopRecorder, RunEvent, RunEventBus, SpanKind};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// A self-contained mock that satisfies the `indicator_panel` contract
/// without dragging in the parquet fixture loader. Returns a static
/// `IndicatorPanel`-shaped JSON object; calls increment a shared
/// counter so tests can assert "actually invoked".
struct MockIndicatorPanel {
    invocations: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
}

#[async_trait]
impl Tool for MockIndicatorPanel {
    fn name(&self) -> ToolName {
        ToolName::new("indicator_panel")
    }
    fn description(&self) -> &'static str {
        "mock indicator panel (test fixture)"
    }
    fn descriptor(&self) -> xvision_agent_client::protocol::ToolDescriptor {
        xvision_agent_client::protocol::ToolDescriptor {
            name: self.name().as_str().to_string(),
            version: "1".to_string(),
            description: self.description().to_string(),
            input_schema: json!({ "type": "object" }),
            output_schema: json!({ "type": "object" }),
            timeout_ms: 1_000,
            side_effect_level: xvision_agent_client::protocol::SideEffectLevel::ReadOnly,
            requires_approval: false,
        }
    }
    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        self.invocations.lock().unwrap().push(input);
        Ok(json!({
            "rsi_14": 55.0,
            "sma_20": 100.0,
            "ema_12": 99.5,
            "atr_14": 1.2
        }))
    }
}

/// Dispatch that captures every outbound `LlmRequest`. The first call
/// returns a `tool_use` for `indicator_panel`; the second call returns a
/// text `EndTurn`. Lets us drive `execute_slot` through one tool-call
/// iteration AND inspect the `tools` array that was advertised.
struct ToolUseThenEndTurn {
    seen: std::sync::Mutex<Vec<LlmRequest>>,
}

impl ToolUseThenEndTurn {
    fn new() -> Self {
        Self {
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn requests(&self) -> Vec<LlmRequest> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmDispatch for ToolUseThenEndTurn {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let saw_tool_result = req.messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
        });
        self.seen.lock().unwrap().push(req);
        if saw_tool_result {
            Ok(LlmResponse {
                content: vec![ContentBlock::Text {
                    text: r#"{"action":"hold","conviction":0.4,"justification":"rsi neutral"}"#.into(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            })
        } else {
            Ok(LlmResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "tu-indicator-1".into(),
                    name: "indicator_panel".into(),
                    input: json!({"asset": "BTC/USD", "fixture": "test", "lookback_bars": 50}),
                }],
                stop_reason: StopReason::ToolUse,
                input_tokens: 1,
                output_tokens: 1,
            })
        }
    }
}

fn registry_with_mock() -> (
    Arc<ToolRegistry>,
    std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
) {
    let invocations = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut registry = ToolRegistry::empty();
    registry.register(Arc::new(MockIndicatorPanel {
        invocations: invocations.clone(),
    }));
    (Arc::new(registry), invocations)
}

fn trader_slot_with_tools(tools: Vec<String>) -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4-6".into(),
        allowed_tools: tools,
        provider: Some("anthropic".into()),
        model: Some("claude-sonnet-4-6".into()),
    }
}

/// Build a minimal `Strategy` that drives the agent-loop path. The
/// manifest's `required_tools` is what the indicator-tool-wiring bridge
/// consumes; the slot itself starts with empty `allowed_tools` (matching
/// what `agent_slot_to_llm_slot` produces today) so the bridge has to
/// pick it up from the manifest.
fn strategy_with_required_tools(required: Vec<String>) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "test-strategy".into(),
            display_name: "Test Strategy".into(),
            plain_summary: "indicator-tool-wiring test fixture".into(),
            creator: "@test".into(),
            template: "test".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec!["anthropic.claude-sonnet-4-6".into()],
            required_tools: required,
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: "test-agent".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
        }],
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: json!({}),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// (a) dispatched tools array contains an `indicator_panel` schema entry
//     when the strategy's required_tools declares it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn agent_loop_dispatch_advertises_indicator_panel_tool_when_strategy_requires_it() {
    let strategy = strategy_with_required_tools(vec!["ohlcv".into(), "indicator_panel".into()]);

    // Resolved slot starts with empty allowed_tools — that's what
    // `agent_slot_to_llm_slot` produces today; the wiring fix must
    // pull the surface from the strategy manifest, not from the slot.
    let resolved = ResolvedAgentSlot {
        role: "trader".into(),
        slot: trader_slot_with_tools(Vec::new()),
        system_prompt: "decide".into(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: "test-agent".into(),
        noop_skip: true,
    };

    let dispatch = Arc::new(ToolUseThenEndTurn::new());
    let (registry, _invocations) = registry_with_mock();

    let inputs = PipelineInputs {
        strategy: &strategy,
        agent_slots: &[resolved],
        seed_inputs: json!({}),
        dispatch: dispatch.clone(),
        tools: registry,
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
    };

    let _ = run_pipeline(inputs).await.expect("pipeline runs");

    let requests = dispatch.requests();
    assert!(!requests.is_empty(), "dispatcher must have been called");

    let first = &requests[0];
    assert!(
        !first.tools.is_empty(),
        "indicator-tool-wiring: first dispatch must advertise the strategy's tools, \
         not `tools: []`. Got: {:?}",
        first.tools
    );
    assert!(
        first.tools.iter().any(|t| t.name == "indicator_panel"),
        "indicator-tool-wiring: tools array must include `indicator_panel`. Got: {:?}",
        first.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// (b) a fixture trader response invoking `indicator_panel` actually
//     executes the tool; the result is fed back as a tool_result block
//     on the next dispatch turn
// ---------------------------------------------------------------------------

#[tokio::test]
async fn agent_loop_routes_tool_use_to_indicator_panel_and_feeds_result_back() {
    let strategy = strategy_with_required_tools(vec!["indicator_panel".into()]);
    let resolved = ResolvedAgentSlot {
        role: "trader".into(),
        slot: trader_slot_with_tools(Vec::new()),
        system_prompt: "decide".into(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: "test-agent".into(),
        noop_skip: true,
    };
    let dispatch = Arc::new(ToolUseThenEndTurn::new());
    let (registry, invocations) = registry_with_mock();

    let inputs = PipelineInputs {
        strategy: &strategy,
        agent_slots: &[resolved],
        seed_inputs: json!({}),
        dispatch: dispatch.clone(),
        tools: registry,
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
    };

    let outputs = run_pipeline(inputs).await.expect("pipeline runs");

    // The tool was actually invoked exactly once with the model's args.
    let calls = invocations.lock().unwrap().clone();
    assert_eq!(
        calls.len(),
        1,
        "indicator_panel tool must execute exactly once when the model emits one tool_use; got {} calls",
        calls.len()
    );
    assert_eq!(
        calls[0].get("asset").and_then(|v| v.as_str()),
        Some("BTC/USD"),
        "tool input must carry the model's args verbatim",
    );

    // The second dispatch saw a ToolResult block carrying the mock's
    // payload — the loop wired the result back into the conversation.
    let requests = dispatch.requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 dispatch turns (tool_use, then end_turn); got {}",
        requests.len()
    );
    let second = &requests[1];
    let has_tool_result = second.messages.iter().any(|m| {
        m.content.iter().any(|c| match c {
            ContentBlock::ToolResult {
                tool_use_id, content, ..
            } => tool_use_id == "tu-indicator-1" && content.contains("rsi_14"),
            _ => false,
        })
    });
    assert!(
        has_tool_result,
        "second dispatch turn must carry the indicator_panel ToolResult; messages={:?}",
        second.messages
    );

    // The pipeline returned the trader's final EndTurn text.
    let trader = outputs.trader.expect("trader output present");
    assert!(
        trader.text().contains("hold"),
        "final trader response must be the EndTurn text; got {:?}",
        trader.text()
    );
}

#[tokio::test]
async fn agent_loop_rejects_unadvertised_indicator_panel_tool_use() {
    let strategy = strategy_with_required_tools(vec![]);
    let resolved = ResolvedAgentSlot {
        role: "trader".into(),
        slot: trader_slot_with_tools(Vec::new()),
        system_prompt: "decide".into(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: "test-agent".into(),
        noop_skip: true,
    };
    let dispatch = Arc::new(ToolUseThenEndTurn::new());
    let (registry, invocations) = registry_with_mock();

    let inputs = PipelineInputs {
        strategy: &strategy,
        agent_slots: &[resolved],
        seed_inputs: json!({}),
        dispatch: dispatch.clone(),
        tools: registry,
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
    };

    let outputs = run_pipeline(inputs).await.expect("pipeline runs");
    assert!(
        outputs.trader.is_some(),
        "model should get an error tool_result and recover to a final trader response"
    );

    let calls = invocations.lock().unwrap().clone();
    assert!(
        calls.is_empty(),
        "unadvertised indicator_panel tool_use must not invoke the registry; got {calls:?}"
    );

    let requests = dispatch.requests();
    assert!(
        requests.len() >= 2,
        "expected a recovery dispatch after the denied tool_use; got {}",
        requests.len()
    );
    assert!(
        requests[0].tools.is_empty(),
        "strategy with no required_tools must not advertise indicator_panel; got {:?}",
        requests[0].tools
    );
    let denied_tool_result = requests[1].messages.iter().any(|m| {
        m.content.iter().any(|c| match c {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                tool_use_id == "tu-indicator-1" && *is_error == Some(true) && content.contains("not allowed")
            }
            _ => false,
        })
    });
    assert!(
        denied_tool_result,
        "second dispatch must carry an error ToolResult for the denied tool_use; messages={:?}",
        requests[1].messages
    );
}

// ---------------------------------------------------------------------------
// (c) tool execution emits observability spans carrying the tool name.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn indicator_panel_invocation_emits_validate_spans_for_trace_dock() {
    let strategy = strategy_with_required_tools(vec!["indicator_panel".into()]);
    let resolved = ResolvedAgentSlot {
        role: "trader".into(),
        slot: trader_slot_with_tools(Vec::new()),
        system_prompt: "decide".into(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: "test-agent".into(),
        noop_skip: true,
    };

    let recorder: Arc<NoopRecorder> = Arc::new(NoopRecorder::new());
    let bus = Arc::new(RunEventBus::new(vec![
        recorder.clone() as Arc<dyn AgentRunRecorder>
    ]));
    let emitter = ObsEmitter::new(bus.clone(), "run-indicator-tool-wiring");

    let dispatch = Arc::new(ToolUseThenEndTurn::new());
    let (registry, _invocations) = registry_with_mock();

    let inputs = PipelineInputs {
        strategy: &strategy,
        agent_slots: &[resolved],
        seed_inputs: json!({}),
        dispatch: dispatch.clone(),
        tools: registry,
        obs: Some(emitter),
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
    };

    let _ = run_pipeline(inputs).await.expect("pipeline runs");

    // Drain the bus into the recorder.
    for _ in 0..50 {
        bus.quiesce().await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    let events = recorder.snapshot().await;

    let tool_call_spans: Vec<&xvision_observability::SpanStartedEvent> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s) if matches!(s.kind, SpanKind::ToolCall) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        tool_call_spans.len(),
        1,
        "expected one ToolCall span per indicator_panel invocation; got {}",
        tool_call_spans.len()
    );
    assert_eq!(
        tool_call_spans[0].name, "indicator_panel",
        "ToolCall span must carry tool name `indicator_panel`; got {:?}",
        tool_call_spans[0].name
    );

    let validate_spans: Vec<&xvision_observability::SpanStartedEvent> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::SpanStarted(s)
                if matches!(s.kind, SpanKind::ToolValidateInput | SpanKind::ToolValidateOutput) =>
            {
                Some(s)
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        validate_spans.len(),
        2,
        "expected one ToolValidateInput + one ToolValidateOutput per indicator_panel \
         invocation; got {} validate spans",
        validate_spans.len()
    );
    for span in &validate_spans {
        assert_eq!(
            span.name, "indicator_panel",
            "validate span must carry tool name `indicator_panel`; got {:?}",
            span.name
        );
    }
}

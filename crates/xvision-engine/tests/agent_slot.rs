use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch, StopReason,
};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::{Tool, ToolName, ToolRegistry};

struct EchoOhlcvTool;

#[async_trait]
impl Tool for EchoOhlcvTool {
    fn name(&self) -> ToolName {
        ToolName::new("ohlcv")
    }

    fn description(&self) -> &'static str {
        "test OHLCV echo"
    }

    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "asset": input.get("asset").cloned().unwrap_or(serde_json::Value::Null),
            "bars": [{"close": 50_000.0}]
        }))
    }
}

struct MismatchedAssetToolDispatch {
    calls: Mutex<u32>,
}

#[async_trait]
impl LlmDispatch for MismatchedAssetToolDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            return Ok(MockDispatch::tool_use(
                "tu_wrong_asset",
                "ohlcv",
                serde_json::json!({"asset": "BTC/USD", "fixture": "any"}),
            ));
        }

        let body = req
            .messages
            .iter()
            .map(|m| serde_json::to_string(m).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("|");
        if !body.contains("asset mismatch") || !body.contains("ETH/USD") || !body.contains("BTC/USD") {
            anyhow::bail!("expected wrong-asset ohlcv call to be returned as an asset mismatch tool error; got {body}");
        }

        Ok(LlmResponse {
            content: vec![ContentBlock::Text {
                text: r#"{"action":"hold","conviction":0.0,"justification":"wrong asset market-data tool call was blocked"}"#.into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 50,
            output_tokens: 30,
        })
    }
}

#[tokio::test]
async fn execute_slot_returns_parsed_output() {
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: vec!["ohlcv".into()],
        provider: None,
        model: None,
    };
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.7,"justification":"oversold"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let out = execute_slot(SlotInput {
        slot: &slot,
        system_prompt: String::new(),
        upstream_inputs: serde_json::json!({"ohlcv_history": [], "indicator_panel": {}}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: Some(4096),
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .unwrap();
    assert!(out.text().contains("long_open"));
    assert!(out.input_tokens > 0);
}

#[tokio::test]
async fn execute_slot_loops_through_tool_use_to_final_text() {
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: vec!["ohlcv".into()],
        provider: None,
        model: None,
    };

    // Sequence: turn 1 emits tool_use(ohlcv); turn 2 emits final text.
    let dispatch = Arc::new(MockDispatch::sequence(vec![
        MockDispatch::tool_use(
            "tu_001",
            "ohlcv",
            serde_json::json!({"asset": "BTC/USD", "fixture": "test-fixture-btc-2024-01"}),
        ),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: r#"{"action":"long_open","conviction":0.7,"justification":"oversold"}"#.into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 50,
            output_tokens: 30,
        },
    ]));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let out = execute_slot(SlotInput {
        slot: &slot,
        system_prompt: String::new(),
        upstream_inputs: serde_json::json!({
            "asset": "BTC/USD",
            "fixture": "test-fixture-btc-2024-01"
        }),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: Some(4096),
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .unwrap();
    assert!(out.text().contains("long_open"));
    // Two LLM calls: tool_use then final text. Tokens accumulate.
    assert!(out.input_tokens >= 50);
}

#[tokio::test]
async fn execute_slot_blocks_market_data_tool_calls_for_the_wrong_decision_asset() {
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: vec!["ohlcv".into()],
        provider: None,
        model: None,
    };
    let dispatch = Arc::new(MismatchedAssetToolDispatch {
        calls: Mutex::new(0),
    });
    let mut registry = ToolRegistry::empty();
    registry.register(Arc::new(EchoOhlcvTool));

    let out = execute_slot(SlotInput {
        slot: &slot,
        system_prompt: String::new(),
        upstream_inputs: serde_json::json!({
            "asset": "ETH/USD",
            "market_data": {
                "asset": "ETH/USD",
                "reference_price_usd": 3_000.0,
                "current_bar": {"close": 3_000.0}
            }
        }),
        dispatch,
        tools: Arc::new(registry),
        response_schema: None,
        max_tokens: Some(4096),
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .expect("wrong-asset tool call should be recoverable as a tool error");

    assert!(out.text().contains("wrong asset market-data tool call was blocked"));
}

#[tokio::test]
async fn execute_slot_allows_more_than_eight_productive_tool_calls() {
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: vec!["ohlcv".into()],
        provider: None,
        model: None,
    };

    let mut responses = (0..9)
        .map(|idx| {
            MockDispatch::tool_use(
                &format!("tu_{idx:03}"),
                "ohlcv",
                serde_json::json!({
                    "asset": "BTC/USD",
                    "fixture": "test-fixture-btc-2024-01"
                }),
            )
        })
        .collect::<Vec<_>>();
    responses.push(LlmResponse {
        content: vec![ContentBlock::Text {
            text:
                r#"{"action":"long_open","conviction":0.7,"justification":"complete after deeper research"}"#
                    .into(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 50,
        output_tokens: 30,
    });

    let dispatch = Arc::new(MockDispatch::sequence(responses));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let out = execute_slot(SlotInput {
        slot: &slot,
        system_prompt: String::new(),
        upstream_inputs: serde_json::json!({
            "asset": "BTC/USD",
            "fixture": "test-fixture-btc-2024-01"
        }),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: Some(4096),
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await
    .unwrap();

    assert!(out.text().contains("long_open"));
    assert!(out.input_tokens >= 140);
}

#[tokio::test]
async fn execute_slot_succeeds_even_when_caller_passes_extra_inputs() {
    let slot = LLMSlot {
        role: "trader".into(),
        attested_with: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: vec!["ohlcv".into()],
        provider: None,
        model: None,
    };
    let dispatch = Arc::new(MockDispatch::echo("ok"));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    // Tool-allowlist enforcement is a Plan #2 concern; MVP execute_slot
    // does not invoke tools so undeclared inputs pass through.
    let result = execute_slot(SlotInput {
        slot: &slot,
        system_prompt: String::new(),
        upstream_inputs: serde_json::json!({"requested_tool": "indicator_panel"}),
        dispatch,
        tools,
        response_schema: None,
        max_tokens: Some(4096),
        temperature: None,
        obs: None,
        memory: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        scenario_start: None,
        source_window_start: None,
        source_window_end: None,
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
        trace_name: None,
        trace_attrs: None,
    })
    .await;
    assert!(result.is_ok());
}

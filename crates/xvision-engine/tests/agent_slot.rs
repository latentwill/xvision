use std::sync::Arc;
use xvision_engine::agent::execute::{execute_slot, SlotInput};
use xvision_engine::agent::llm::{ContentBlock, LlmResponse, MockDispatch, StopReason};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::tools::ToolRegistry;

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
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
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
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
    })
    .await
    .unwrap();
    assert!(out.text().contains("long_open"));
    // Two LLM calls: tool_use then final text. Tokens accumulate.
    assert!(out.input_tokens >= 50);
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
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
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
        run_id: String::new(),
        scenario_id: String::new(),
        cycle_idx: 0,
        catalog: None,
        delta_briefing: false,
        prev_briefing: None,
    })
    .await;
    assert!(result.is_ok());
}

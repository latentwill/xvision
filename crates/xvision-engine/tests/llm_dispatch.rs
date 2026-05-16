use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, Message, MockDispatch, StopReason};

#[tokio::test]
async fn mock_dispatch_returns_text_block() {
    let mock = MockDispatch::echo(r#"{"action":"hold","conviction":0.5,"justification":"mock"}"#);
    let resp = mock
        .complete(LlmRequest {
            model: "anthropic.claude-sonnet-4.6".into(),
            system_prompt: "you are a trader".into(),
            messages: vec![Message::user_text("decide")],
            max_tokens: Some(200),
            tools: vec![],
            temperature: None,
            response_schema: None,
        })
        .await
        .unwrap();
    assert!(resp.text().contains("hold"));
    assert!(matches!(resp.stop_reason, StopReason::EndTurn));
    assert!(resp.input_tokens > 0);
    assert!(resp.output_tokens > 0);
}

#[tokio::test]
async fn mock_dispatch_sequence_drives_tool_use_loop() {
    // First call: model wants to call `xvn_create_strategy`.
    // Second call: model emits final text response.
    let mock = MockDispatch::sequence(vec![
        MockDispatch::tool_use(
            "tu_1",
            "xvn_create_strategy",
            serde_json::json!({"template":"trend_follower","name":"x"}),
        ),
        xvision_engine::agent::llm::LlmResponse {
            content: vec![ContentBlock::Text {
                text: "Created. Want me to validate?".into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 5,
            output_tokens: 5,
        },
    ]);
    let req = LlmRequest {
        model: "claude-sonnet-4-6".into(),
        system_prompt: "you are the wizard".into(),
        messages: vec![Message::user_text("make me a trend follower")],
        max_tokens: Some(200),
        tools: vec![],
        temperature: None,
        response_schema: None,
    };

    let r1 = mock.complete(req.clone()).await.unwrap();
    assert!(matches!(r1.stop_reason, StopReason::ToolUse));
    let uses = r1.tool_uses();
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].1, "xvn_create_strategy");

    let r2 = mock.complete(req).await.unwrap();
    assert!(matches!(r2.stop_reason, StopReason::EndTurn));
    assert!(r2.text().contains("Created"));
}

#[tokio::test]
#[ignore = "needs ANTHROPIC_API_KEY"]
async fn anthropic_dispatch_returns_real_text() {
    use xvision_engine::agent::llm::AnthropicDispatch;
    let key = std::env::var("ANTHROPIC_API_KEY").unwrap();
    let d = AnthropicDispatch::new(key);
    let resp = d
        .complete(LlmRequest {
            model: "claude-sonnet-4-6".into(),
            system_prompt: "you are concise".into(),
            messages: vec![Message::user_text("say 'hello' and nothing else")],
            max_tokens: Some(50),
            tools: vec![],
            temperature: None,
            response_schema: None,
        })
        .await
        .unwrap();
    assert!(resp.text().to_lowercase().contains("hello"));
}

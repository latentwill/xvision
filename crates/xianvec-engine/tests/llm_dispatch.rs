use xianvec_engine::agent::llm::{LlmDispatch, LlmRequest, MockDispatch};

#[tokio::test]
async fn mock_dispatch_returns_expected_output() {
    let mock = MockDispatch::echo(r#"{"action":"hold","conviction":0.5,"justification":"mock"}"#);
    let resp = mock.complete(LlmRequest {
        model: "anthropic.claude-sonnet-4.6".into(),
        system_prompt: "you are a trader".into(),
        user_prompt: "decide".into(),
        max_tokens: 200,
    }).await.unwrap();
    assert!(resp.text.contains("hold"));
    assert!(resp.input_tokens > 0);
    assert!(resp.output_tokens > 0);
}

#[tokio::test]
#[ignore = "needs ANTHROPIC_API_KEY"]
async fn anthropic_dispatch_returns_real_text() {
    use xianvec_engine::agent::llm::AnthropicDispatch;
    let key = std::env::var("ANTHROPIC_API_KEY").unwrap();
    let d = AnthropicDispatch::new(key);
    let resp = d.complete(LlmRequest {
        model: "claude-sonnet-4-6".into(),
        system_prompt: "you are concise".into(),
        user_prompt: "say 'hello' and nothing else".into(),
        max_tokens: 50,
    }).await.unwrap();
    assert!(resp.text.to_lowercase().contains("hello"));
}

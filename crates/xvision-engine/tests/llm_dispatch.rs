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
            cache_control: None,
            force_json: false,
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
        MockDispatch::tool_use("tu_1", "xvn_create_strategy", serde_json::json!({"name":"x"})),
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
        cache_control: None,
        force_json: false,
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

// ---- OpenAI-compat retry policy --------------------------------------
//
// Track `eval-provider-error-classify-retry` (intake #344).
//
// `OpenaiCompatDispatch::complete` now retries:
//  - 429 honouring `X-RateLimit-Reset` (max 3 attempts total).
//  - 200-OK responses missing `choices` with exponential backoff base
//    500ms (max 3 attempts total).
//
// Verified end-to-end via wiremock with response cycles: the first
// attempt produces the failure shape; the second produces a valid
// completion. We can't easily verify the wall-clock delay (CI variance)
// so we assert only that the retry recovers and that the typed error
// surfaces *after* the budget is exhausted.

mod openai_retry {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use xvision_engine::agent::llm::{OpenAiCompatError, OpenaiCompatDispatch};

    fn ok_completion_body() -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": "{\"action\":\"hold\",\"conviction\":0.5,\"justification\":\"ok\"}"
                }
            }],
            "usage": { "prompt_tokens": 12, "completion_tokens": 3 }
        })
    }

    fn req_for(model: &str) -> LlmRequest {
        LlmRequest {
            model: model.into(),
            system_prompt: "you are a test".into(),
            messages: vec![Message::user_text("decide")],
            max_tokens: Some(200),
            tools: vec![],
            temperature: None,
            response_schema: None,
            cache_control: None,
            force_json: false,
        }
    }

    /// 429 with an `X-RateLimit-Reset` header retries after the reset
    /// and succeeds on the second attempt. We set the reset to "now" so
    /// the test doesn't sleep — `parse_rate_limit_reset`'s saturating
    /// math clamps to a small jitter.
    #[tokio::test]
    async fn rate_limited_with_reset_header_retries_and_succeeds() {
        let server = MockServer::start().await;

        // First attempt: 429 with X-RateLimit-Reset set to now() (so
        // the wait is essentially just jitter, ≤256ms).
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let reset_str = now_ms.to_string();
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("x-ratelimit-reset", reset_str.as_str())
                    .insert_header("x-ratelimit-remaining", "0")
                    .set_body_string("429 Too Many Requests: limit_rpm/test"),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second attempt: successful completion.
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_completion_body()))
            .mount(&server)
            .await;

        let dispatch = OpenaiCompatDispatch::new(server.uri(), "test-key".into());
        let resp = dispatch.complete(req_for("test-model")).await.unwrap();
        assert!(resp.text().contains("hold"));
    }

    /// 200 OK body missing `choices` is classified as
    /// `MissingChoicesArray` and retried, succeeding on retry.
    #[tokio::test]
    async fn missing_choices_array_retries_and_succeeds() {
        let server = MockServer::start().await;

        // First attempt: 200 with a body missing `choices`. Matches the
        // audit-log shape that previously fell through to
        // `[unclassified]` and failed the run.
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-bad",
                "object": "chat.completion",
                "usage": { "prompt_tokens": 1, "completion_tokens": 0 }
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second attempt: well-formed completion.
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_completion_body()))
            .mount(&server)
            .await;

        let dispatch = OpenaiCompatDispatch::new(server.uri(), "test-key".into());
        let resp = dispatch.complete(req_for("test-model")).await.unwrap();
        assert!(resp.text().contains("hold"));
    }

    /// Undecodable 200 OK JSON retries by issuing a fresh HTTP request, not
    /// by reparsing the same invalid body.
    #[tokio::test]
    async fn invalid_json_retries_with_fresh_request_and_succeeds() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{not json"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_completion_body()))
            .mount(&server)
            .await;

        let dispatch = OpenaiCompatDispatch::new(server.uri(), "test-key".into());
        let resp = dispatch.complete(req_for("test-model")).await.unwrap();
        assert!(resp.text().contains("hold"));
    }

    /// Gemini's OpenAI-compatible endpoint includes a non-`/v1` versioned root:
    /// `/v1beta/openai`. Normalization must preserve that root instead of
    /// appending `/v1`, or dispatch goes to `/v1beta/openai/v1/chat/completions`.
    #[tokio::test]
    async fn non_v1_openai_compat_root_is_preserved() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/openai/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_completion_body()))
            .mount(&server)
            .await;

        let dispatch =
            OpenaiCompatDispatch::new(format!("{}/v1beta/openai", server.uri()), "test-key".into());
        let resp = dispatch.complete(req_for("gemini-2.5-flash")).await.unwrap();
        assert!(resp.text().contains("hold"));
    }

    /// Once the `MissingChoicesArray` retry budget is exhausted (3
    /// attempts total = 1 initial + 2 retries), the typed
    /// `OpenAiCompatError::MissingChoicesArray` surfaces with the
    /// retry count populated.
    #[tokio::test]
    async fn missing_choices_array_bubbles_typed_error_after_retries() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-bad",
                "object": "chat.completion"
            })))
            .mount(&server)
            .await;

        let dispatch = OpenaiCompatDispatch::new(server.uri(), "test-key".into());
        let err = dispatch.complete(req_for("test-model")).await.unwrap_err();
        let typed = err
            .chain()
            .find_map(|c| c.downcast_ref::<OpenAiCompatError>())
            .expect("typed OpenAiCompatError on the error chain");
        match typed {
            OpenAiCompatError::MissingChoicesArray { retry_count, .. } => {
                assert!(
                    *retry_count >= 2,
                    "should record exhausted retries, got {retry_count}"
                );
            }
            other => panic!("expected MissingChoicesArray, got {other:?}"),
        }
    }

    /// Typed `OpenAiCompatError` rolls up to the eval classifier's
    /// stable class-tag. Pins the wire-format contract review/UI
    /// consumers parse out of `eval_runs.error`.
    #[test]
    fn typed_error_class_tags_are_stable() {
        let rate = OpenAiCompatError::RateLimited {
            status: 429,
            url: "https://example.invalid/v1/chat/completions".into(),
            body: "429 Too Many Requests: limit_rpm/test".into(),
            reset_at_ms: None,
            retry_after: None,
            retry_count: 2,
        };
        assert_eq!(rate.class_tag(), "provider_rate_limited");
        let err: anyhow::Error = anyhow::Error::new(rate).context("review dispatch failed after retries");
        let class = xvision_engine::eval::executor::classify_run_failure(&err);
        assert_eq!(class, "provider_rate_limited");

        let missing = OpenAiCompatError::MissingChoicesArray {
            url: "https://example.invalid/v1/chat/completions".into(),
            body_excerpt: "{}".into(),
            retry_count: 2,
        };
        assert_eq!(missing.class_tag(), "provider_missing_choices");

        // The classifier walks the chain — pin that downcast works
        // through an anyhow::Error::new wrap.
        let err: anyhow::Error = anyhow::Error::new(missing);
        let class = xvision_engine::eval::executor::classify_run_failure(&err);
        assert_eq!(class, "provider_missing_choices");
    }
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
            cache_control: None,
            force_json: false,
        })
        .await
        .unwrap();
    assert!(resp.text().to_lowercase().contains("hello"));
}

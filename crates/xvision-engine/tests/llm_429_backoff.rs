//! Unit tests for the 429 rate-limit retry handler in `OpenaiCompatDispatch`.
//!
//! Uses `wiremock` (already in dev-dependencies) to stub the HTTP layer:
//! - test 1: 429 with `X-RateLimit-Reset` header, success on 2nd attempt.
//! - test 2: persistent 429 (3 attempts all fail) → error propagated after
//!   `MAX_RATE_LIMIT_RETRIES` (3) are exhausted.

use std::time::{Duration, Instant};

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use xvision_engine::agent::llm::{LlmDispatch, LlmRequest, Message, OpenaiCompatDispatch};

fn hold_response_body() -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": r#"{"action":"hold","conviction":0.0,"justification":"test"}"#,
                "refusal": null
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 20,
            "total_tokens": 30
        }
    })
}

fn simple_request() -> LlmRequest {
    LlmRequest {
        model: "google/gemini-3.1-flash-lite".into(),
        system_prompt: "you are a trader".into(),
        messages: vec![Message::user_text("decide")],
        max_tokens: None,
        tools: vec![],
        temperature: None,
        response_schema: None,
        cache_control: None,
    }
}

// ── test 1 ──────────────────────────────────────────────────────────────────

/// A 429 with a near-future `X-RateLimit-Reset` header causes the dispatcher
/// to sleep and retry. The 2nd request succeeds; the total elapsed time is
/// roughly >= the requested wait.
#[tokio::test]
async fn retries_once_on_429_and_succeeds_on_second_attempt() {
    let server = MockServer::start().await;

    // First request: 429 with reset in ~100ms from now.
    let reset_ms = (chrono::Utc::now().timestamp_millis() + 100).to_string();
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .append_header("x-ratelimit-reset", reset_ms.as_str())
                .append_header("x-ratelimit-limit", "450")
                .append_header("x-ratelimit-remaining", "0"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    // Second request: 200 OK with a valid completion.
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(hold_response_body()))
        .mount(&server)
        .await;

    let dispatch = OpenaiCompatDispatch::new(server.uri(), "test-key".into());
    let t0 = Instant::now();
    let result = dispatch.complete(simple_request()).await;
    let elapsed = t0.elapsed();

    assert!(result.is_ok(), "expected success on 2nd attempt; got: {result:?}");
    let resp = result.unwrap();
    assert!(resp.text().contains("hold"), "response should contain 'hold'");
    // The dispatcher slept at least ~100ms before retrying (plus jitter, capped).
    assert!(
        elapsed.as_millis() >= 80,
        "expected at least 80ms elapsed for the 429 sleep; got {}ms",
        elapsed.as_millis()
    );
}

// ── test 2 ──────────────────────────────────────────────────────────────────

/// When the provider returns 429 on every attempt, the error propagates after
/// MAX_RATE_LIMIT_RETRIES (3) retries — the dispatcher does NOT loop forever.
#[tokio::test]
async fn persistent_429_fails_after_max_retries() {
    let server = MockServer::start().await;

    // Every request returns 429 with a minimal (0ms) reset so the test
    // completes quickly. Jitter adds 0–200ms per attempt, so 3 retries
    // could take up to ~600ms in the worst case.
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .append_header("x-ratelimit-reset", "0") // reset = epoch 0 → wait = 0 + jitter
                .append_header("x-ratelimit-remaining", "0"),
        )
        .mount(&server)
        .await;

    let dispatch = OpenaiCompatDispatch::new(server.uri(), "test-key".into());
    let result = tokio::time::timeout(Duration::from_secs(2), dispatch.complete(simple_request()))
        .await
        .expect("persistent 429 retry loop must terminate within the bounded retry budget");

    assert!(result.is_err(), "expected error after exhausting retries; got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("429"),
        "error message should mention 429; got: {err_msg}"
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        3,
        "persistent 429 should make one initial request plus two retries before surfacing the error"
    );
}

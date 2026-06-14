//! Behavioral test for the LLM HTTP client timeout (W3 / xvision-t4u8.3,
//! Finding #3, DoD §2).
//!
//! Stands up a TCP listener that accepts connections but never responds,
//! then drives the real `AnthropicDispatch` / `OpenaiCompatDispatch` pointed
//! at that stub URL with a short test timeout. Asserts that `complete()`
//! returns an error quickly rather than hanging indefinitely.
//!
//! We do NOT read back the reqwest::Client builder's timeout field
//! (it is opaque) — we assert observable behavior: the call must resolve
//! within a generous wall-clock budget (5s) while the stub never replies.

use std::time::{Duration, Instant};

use tokio::net::TcpListener;
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, LlmRequest, Message, OpenaiCompatDispatch};

/// Spawns a TCP server that accepts connections and immediately parks them —
/// it reads nothing, writes nothing, never closes. Returns the local address.
async fn silent_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        // Accept connections forever; hold them open without responding.
        loop {
            if let Ok((_stream, _peer)) = listener.accept().await {
                // Intentionally hold `_stream` in a spawned task so the
                // connection stays open (no TCP RST) — the client sees an
                // idle open socket, not a refused connection.
                tokio::spawn(async move {
                    // Park this task until the test process exits.
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    drop(_stream);
                });
            }
        }
    });
    addr
}

fn minimal_request() -> LlmRequest {
    LlmRequest {
        model: "claude-sonnet-4-6".into(),
        system_prompt: "you are a trader".into(),
        messages: vec![Message::user_text("decide")],
        max_tokens: Some(100),
        tools: vec![],
        temperature: None,
        response_schema: None,
        cache_control: None,
        force_json: false,
    }
}

/// `AnthropicDispatch` pointed at a silent server with a SHORT test timeout
/// must return an error within 5 seconds rather than hanging until a
/// proxy/OS cutoff (~120 s observed in production).
#[tokio::test]
async fn anthropic_dispatch_times_out_against_silent_server() {
    let addr = silent_server().await;
    // Use the test-overridable constructor with a 500 ms timeout.
    let dispatch = AnthropicDispatch::with_timeout("test-key".into(), Duration::from_millis(500));

    // Override the endpoint to point at our silent stub.
    // The method under test must time out; we give it a 5 s wall-clock budget.
    let t0 = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        dispatch.complete_with_url(minimal_request(), &format!("http://{addr}/v1/messages")),
    )
    .await;

    // Must NOT have hit the outer 5 s wall-clock budget.
    assert!(
        result.is_ok(),
        "complete() hung for > 5 s against a silent server — timeout is not applied"
    );
    // The inner result must be an error (timeout).
    assert!(
        result.unwrap().is_err(),
        "complete() should return Err on timeout, not Ok"
    );
    // Must have returned well within the outer budget.
    assert!(
        t0.elapsed() < Duration::from_secs(5),
        "elapsed {:?} exceeded 5 s budget",
        t0.elapsed()
    );
}

/// `OpenaiCompatDispatch` with a short test timeout must also return an error
/// quickly against a silent server.
#[tokio::test]
async fn openai_compat_dispatch_times_out_against_silent_server() {
    let addr = silent_server().await;
    let dispatch = OpenaiCompatDispatch::with_timeout(
        format!("http://{addr}"),
        "test-key".into(),
        Duration::from_millis(500),
    );

    let t0 = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(5), dispatch.complete(minimal_request())).await;

    assert!(
        result.is_ok(),
        "complete() hung for > 5 s against a silent server — timeout is not applied"
    );
    assert!(
        result.unwrap().is_err(),
        "complete() should return Err on timeout, not Ok"
    );
    assert!(
        t0.elapsed() < Duration::from_secs(5),
        "elapsed {:?} exceeded 5 s budget",
        t0.elapsed()
    );
}

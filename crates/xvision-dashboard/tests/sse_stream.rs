//! Integration test for `GET /api/eval/runs/:id/stream`.
//!
//! Strategy: spin up a real `axum::serve` on a random port, emit events via
//! the bus from a spawned task, and read the SSE stream body with `reqwest`
//! (which supports streaming). `axum_test::TestServer` materialises the
//! response body all at once, which would block forever on an SSE stream
//! that hasn't closed — even though the stream DOES terminate here (via
//! `drop_channel`), the response body can only be consumed after the stream
//! ends, so we use `reqwest` + streaming to avoid buffering the entire body.

use std::time::Duration;

use tempfile::TempDir;
use tokio::time::timeout;
use xvision_dashboard::{server::build_router, AppState};
use xvision_engine::api::chart::{ChartEquityPoint, RunChartEvent};
use xvision_engine::eval::run::{DeploymentSource, Run, RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;

/// Boot a real TCP server on an ephemeral port and return the base URL.
async fn boot_server() -> (String, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let router = build_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("axum serve failed");
    });

    (base_url, tmp, state)
}

async fn wait_for_run_subscription(bus: &xvision_engine::api::chart::RunEventBus, run_id: &str) {
    timeout(Duration::from_secs(2), async {
        loop {
            if bus.sender(run_id).await.receiver_count() > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("SSE handler did not subscribe to run bus");
}

#[tokio::test]
async fn sse_stream_emits_equity_and_status_then_closes() {
    let (base_url, _tmp, state) = boot_server().await;
    let bus = state.event_bus.clone();

    let url = format!("{base_url}/api/eval/runs/test-run/stream");
    let client = reqwest::Client::new();

    let body_task = tokio::spawn(async move {
        let resp = client
            .get(&url)
            .send()
            .await
            .expect("GET /api/eval/runs/test-run/stream");

        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.contains("text/event-stream"),
            "expected text/event-stream content-type, got: {content_type}"
        );

        // Consume the full body (stream terminates once drop_channel fires).
        resp.bytes().await.expect("read body")
    });

    wait_for_run_subscription(&bus, "test-run").await;
    bus.emit(
        "test-run",
        RunChartEvent::Equity(ChartEquityPoint {
            time: 1,
            equity_usd: 100.0,
        }),
    )
    .await;
    bus.emit(
        "test-run",
        RunChartEvent::Status {
            phase: "completed".into(),
            message: None,
        },
    )
    .await;
    // Drop the channel so the SSE handler sees RecvError::Closed and
    // terminates the stream. Without this the handler waits for the next
    // tick to flush and the test would stall.
    bus.drop_channel("test-run").await;

    // Give the server up to 5 s to deliver the full SSE body and close.
    let body = timeout(Duration::from_secs(5), body_task)
        .await
        .expect("SSE stream did not close within 5 s")
        .expect("SSE request task panicked");

    let text = std::str::from_utf8(&body).expect("body is utf-8");

    assert!(
        text.contains("event: equity"),
        "expected 'event: equity' in SSE body; got:\n{text}"
    );
    assert!(
        text.contains("event: status"),
        "expected 'event: status' in SSE body; got:\n{text}"
    );
    assert!(
        text.contains("100"),
        "expected equity value '100' in SSE body; got:\n{text}"
    );
}

#[tokio::test]
async fn sse_stream_late_subscriber_gets_immediate_close_for_completed_run() {
    let (base_url, _tmp, state) = boot_server().await;

    // Insert a completed run into the store so the handler's pre-check finds
    // it in a terminal state. Uses "crypto-bull-q1-2025" — a canonical
    // scenario seeded by AppState::new so the trigger-based FK passes.
    let store = RunStore::new(state.pool.clone());
    let run = Run {
        id: "test-run-completed".into(),
        agent_id: "abc123".into(),
        agents_agent_id: None,
        scenario_id: "crypto-bull-q1-2025".into(),
        params_override: None,
        mode: RunMode::Backtest,
        status: RunStatus::Queued,
        started_at: chrono::Utc::now(),
        completed_at: None,
        metrics: None,
        error: None,
        estimated_total_tokens: None,
        actual_input_tokens: None,
        actual_output_tokens: None,
        bars_content_hash: None,
        manifest_canonical: None,
        bars_manifest: None,
        auto_fire_review: false,
        review_model: None,
        max_annotations_per_review: Some(8),
        live_config: None,
        paused: false,
        paused_at: None,
        flatten_requested: false,
        source: DeploymentSource::Human,
        unrealized_pnl_usd: None,
    };
    store.create(&run).await.expect("insert test run");
    store
        .update_status("test-run-completed", RunStatus::Completed, None)
        .await
        .expect("mark run completed");

    // Simulate the executor having already cleaned up the bus channel.
    state.event_bus.drop_channel("test-run-completed").await;

    let url = format!("{base_url}/api/eval/runs/test-run-completed/stream");
    let client = reqwest::Client::new();

    // The handler should detect the terminal state and close immediately —
    // give it a generous 2 s timeout (should finish in milliseconds).
    let body = timeout(Duration::from_secs(2), async {
        let resp = client
            .get(&url)
            .send()
            .await
            .expect("GET /api/eval/runs/test-run-completed/stream");

        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.contains("text/event-stream"),
            "expected text/event-stream content-type, got: {content_type}"
        );

        resp.bytes().await.expect("read body")
    })
    .await
    .expect("SSE stream did not close within 2 s for already-completed run");

    let text = std::str::from_utf8(&body).expect("body is utf-8");

    assert!(
        text.contains("event: status"),
        "expected 'event: status' in SSE body; got:\n{text}"
    );
    assert!(
        text.contains("completed"),
        "expected 'completed' phase in SSE body; got:\n{text}"
    );
}

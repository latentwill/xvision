mod support;

#[tokio::test]
async fn get_deployments_returns_array() {
    // test_server() returns (TestServer, TempDir) — bind _tmp so the DB dir
    // is not dropped mid-test.
    let (server, _tmp) = support::test_server().await;
    let res = server.get("/api/live/deployments").await;
    res.assert_status_ok();
    assert!(res.json::<serde_json::Value>().is_array());
}

// ---------------------------------------------------------------------------
// Task 8: /api/live/deployments/:id/stream SSE endpoint
// ---------------------------------------------------------------------------

/// Boot a live TCP server and wait for the subscriber to connect.
async fn wait_for_subscription(bus: &xvision_engine::api::chart::RunEventBus, run_id: &str) {
    use tokio::time::{timeout, Duration};
    timeout(Duration::from_secs(2), async {
        loop {
            if bus.sender(run_id).await.receiver_count() > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("SSE handler did not subscribe within 2 s");
}

#[tokio::test]
async fn live_deployments_stream_emits_live_run_state_and_closes() {
    use std::time::Duration;
    use tokio::time::timeout;
    use xvision_engine::api::chart::{LiveRunStatePayload, RunChartEvent};

    let (base_url, _tmp, state) = support::live_server().await;
    let bus = state.event_bus.clone();

    let url = format!("{base_url}/api/live/deployments/deploy-test-run/stream");
    let client = reqwest::Client::new();

    let body_task = tokio::spawn(async move {
        let resp = client
            .get(&url)
            .send()
            .await
            .expect("GET /api/live/deployments/deploy-test-run/stream");

        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.contains("text/event-stream"),
            "expected text/event-stream, got: {content_type}"
        );

        resp.bytes().await.expect("read body")
    });

    wait_for_subscription(&bus, "deploy-test-run").await;

    bus.emit(
        "deploy-test-run",
        RunChartEvent::LiveRunState(LiveRunStatePayload {
            equity_usd: Some(11_000.0),
            unrealized_pnl_usd: Some(100.0),
            realized_today_usd: Some(0.0),
            daily_loss_remaining_usd: Some(500.0),
            drawdown_pct: Some(0.0),
            risk_veto_count: 0,
            last_decision_at: Some("2026-06-13T12:00:00Z".into()),
        }),
    )
    .await;

    bus.emit(
        "deploy-test-run",
        RunChartEvent::Status {
            phase: "completed".into(),
            message: None,
        },
    )
    .await;
    bus.drop_channel("deploy-test-run").await;

    let body = timeout(Duration::from_secs(5), body_task)
        .await
        .expect("SSE stream did not close within 5 s")
        .expect("body task panicked");

    let text = std::str::from_utf8(&body).expect("body is utf-8");

    assert!(
        text.contains("event: live_run_state"),
        "expected 'event: live_run_state' in SSE body; got:\n{text}"
    );
    assert!(
        text.contains("event: status"),
        "expected 'event: status' in SSE body; got:\n{text}"
    );
    assert!(
        text.contains("11000"),
        "expected equity value in SSE body; got:\n{text}"
    );
}

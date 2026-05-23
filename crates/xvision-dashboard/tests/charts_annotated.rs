//! Integration tests for chart-rework Track B B3 annotated chart routes.

mod support;

use serde_json::Value;
use support::test_server;

#[tokio::test]
async fn annotated_run_route_returns_demo_annotations() {
    let (server, _tmp) = test_server().await;

    let response = server.get("/api/v2/charts/annotated/demo-run").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["kind"], "annotated");
    assert_eq!(body["source"], "run");
    assert_eq!(body["runId"], "demo-run");
    assert_eq!(body["asset"], "BTC/USDT");
    assert_eq!(body["annotations"].as_array().unwrap().len(), 5);
    assert_eq!(body["candles"]["time"].as_array().unwrap().len(), 170);
}

#[tokio::test]
async fn annotated_live_route_accepts_encoded_slash_symbol() {
    let (server, _tmp) = test_server().await;

    let response = server
        .get("/api/v2/charts/annotated/live/BTC%2FUSDT")
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["kind"], "annotated");
    assert_eq!(body["source"], "live");
    assert_eq!(body["symbol"], "BTC/USDT");
    assert_eq!(body["asset"], "BTC/USDT");
    assert_eq!(body["annotations"].as_array().unwrap().len(), 0);
    assert_eq!(body["candles"]["time"].as_array().unwrap().len(), 170);
}

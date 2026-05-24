//! Integration tests for chart-rework Track B B3 annotated chart routes.

mod support;

use serde_json::Value;
use support::{state_with_tempdir, test_server};
use xvision_dashboard::server::build_router;
use xvision_engine::eval::review::{EvalReview, ReviewAnnotation, ReviewStatus, ReviewVerdict};
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;

#[tokio::test]
async fn annotated_run_route_returns_demo_annotations() {
    let (server, _tmp) = test_server().await;

    let response = server.get("/api/v2/charts/annotated/demo").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["kind"], "annotated");
    assert_eq!(body["source"], "run");
    assert_eq!(body["runId"], "demo");
    assert_eq!(body["asset"], "BTC/USDT");
    assert_eq!(body["annotations"].as_array().unwrap().len(), 5);
    assert_eq!(body["candles"]["time"].as_array().unwrap().len(), 170);
}

#[tokio::test]
async fn annotated_live_route_accepts_encoded_slash_symbol() {
    let (server, _tmp) = test_server().await;

    let response = server.get("/api/v2/charts/annotated/live/BTC%2FUSDT").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["kind"], "annotated");
    assert_eq!(body["source"], "live");
    assert_eq!(body["symbol"], "BTC/USDT");
    assert_eq!(body["asset"], "BTC/USDT");
    assert_eq!(body["annotations"].as_array().unwrap().len(), 0);
    assert_eq!(body["note"], "no completed live review annotations for symbol");
    assert_eq!(body["candles"]["time"].as_array().unwrap().len(), 170);
}

#[tokio::test]
async fn annotated_run_route_reads_persisted_review_annotations() {
    let (state, _tmp) = state_with_tempdir().await;
    let store = RunStore::new(state.pool.clone());

    let mut run = Run::new_queued(
        "agent-fixture".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    run.status = RunStatus::Queued;
    store.create(&run).await.expect("seed run");
    store.begin_running(&run.id).await.expect("begin run");
    store
        .update_status(&run.id, RunStatus::Completed, None)
        .await
        .expect("complete run");

    let mut review = EvalReview::new_queued(run.id.clone(), "reasoning-agent".into());
    review.status = ReviewStatus::Completed;
    review.verdict = Some(ReviewVerdict::Weak);
    review.confidence = Some(0.77);
    review.score = Some(44);
    review.summary = Some("annotation review".into());
    review.annotations = vec![ReviewAnnotation {
        idx: 52,
        side: "bottom".into(),
        kind: "FLOW".into(),
        title: "Stored annotation".into(),
        body: "Persisted on eval_reviews.annotations_json.".into(),
        conf: 0.77,
        action: "WATCH".into(),
        danger: false,
        ts: None,
    }];
    store.create_review(&review).await.expect("seed review");

    let server = axum_test::TestServer::new(build_router(state)).unwrap();
    let response = server.get(&format!("/api/v2/charts/annotated/{}", run.id)).await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["runId"], run.id);
    assert_eq!(body["annotations"].as_array().unwrap().len(), 1);
    assert_eq!(body["annotations"][0]["title"], "Stored annotation");
}

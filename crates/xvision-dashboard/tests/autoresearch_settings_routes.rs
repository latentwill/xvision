//! Integration tests for autoresearch config settings routes:
//!   GET  /api/settings/autoresearch — read all 8 keys (defaults for unset)
//!   POST /api/settings/autoresearch — write keys (validated; auth required)
//!
//! Auth note: `require_auth_middleware` exempts loopback. axum-test's TestServer
//! connects as loopback, so the mutating POST reaches the handler without a token
//! (which the persist/validation tests want). The 401 path is exercised via
//! `tower::ServiceExt::oneshot` with an injected non-loopback `ConnectInfo`.

mod support;

use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{Request, StatusCode},
};
use axum_test::TestServer;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init state");
    state.run_dashboard_migrations().await.unwrap();
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

// ── GET /api/settings/autoresearch ────────────────────────────────────────────

#[tokio::test]
async fn get_returns_defaults_on_fresh_store() {
    let (server, _tmp, _state) = boot().await;
    let res = server.get("/api/settings/autoresearch").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: Value = res.json();
    // All 8 keys must be present with their documented defaults.
    assert_eq!(body["promotion_epsilon"], 0.01);
    assert_eq!(body["promotion_acc_floor"], 0.52);
    assert_eq!(body["promotion_min_holdout"], 200);
    assert_eq!(body["min_cycle_count"], 500);
    assert_eq!(body["train_wall_clock_sec"], 300);
    assert_eq!(body["min_precision_lift_pp"], 3.0);
    assert_eq!(body["max_pnl_regression"], 0.0);
    // price_forward_threshold has a default too.
    assert!(body.get("price_forward_threshold").is_some());
}

#[tokio::test]
async fn get_requires_no_auth() {
    // GET is on readonly_router — no auth required.
    let (server, _tmp, _state) = boot().await;
    let res = server.get("/api/settings/autoresearch").await;
    assert_ne!(res.status_code(), StatusCode::UNAUTHORIZED);
}

// ── POST /api/settings/autoresearch ──────────────────────────────────────────

#[tokio::test]
async fn post_persists_and_get_reflects() {
    // TestServer is loopback → auth-exempt → reaches the handler.
    let (server, _tmp, _state) = boot().await;

    let res = server
        .post("/api/settings/autoresearch")
        .json(&json!({ "promotion_epsilon": 0.05 }))
        .await;
    assert_eq!(
        res.status_code(),
        StatusCode::OK,
        "POST must succeed: {:?}",
        res.text()
    );

    // GET must reflect the new value.
    let get_res = server.get("/api/settings/autoresearch").await;
    let body: Value = get_res.json();
    assert!(
        (body["promotion_epsilon"].as_f64().unwrap() - 0.05).abs() < 1e-9,
        "POST value must be reflected in GET"
    );
}

#[tokio::test]
async fn post_requires_auth() {
    // A non-loopback client with no token must be rejected by the auth
    // middleware before the handler runs.
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init state");
    state.run_dashboard_migrations().await.unwrap();
    let app = build_router(state);

    let mut request = Request::builder()
        .method("POST")
        .uri("/api/settings/autoresearch")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "promotion_epsilon": 0.02 }).to_string()))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo::<SocketAddr>("203.0.113.5:12345".parse().unwrap()));

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn post_rejects_out_of_range_promotion_acc_floor() {
    let (server, _tmp, _state) = boot().await;

    let res = server
        .post("/api/settings/autoresearch")
        .json(&json!({ "promotion_acc_floor": -0.1 }))
        .await;
    assert_eq!(
        res.status_code(),
        StatusCode::BAD_REQUEST,
        "out-of-range value must be 400"
    );
}

#[tokio::test]
async fn post_rejects_out_of_range_promotion_epsilon() {
    let (server, _tmp, _state) = boot().await;

    let res = server
        .post("/api/settings/autoresearch")
        .json(&json!({ "promotion_epsilon": 0.0 }))
        .await;
    assert_eq!(
        res.status_code(),
        StatusCode::BAD_REQUEST,
        "epsilon=0 must be 400 (must be > 0)"
    );
}

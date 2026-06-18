//! Integration tests for nanochat checkpoint routes:
//!   GET  /api/nanochat/checkpoints
//!   GET  /api/nanochat/checkpoints/:model_id
//!   POST /api/nanochat/checkpoints/:model_id/approve

mod support;

use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{Request, StatusCode},
};
use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf()).await.expect("init state");
    // Run dashboard migrations (session/auth tables).
    state
        .run_dashboard_migrations()
        .await
        .expect("dashboard migrations");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

/// Insert a `trained_models` row directly via the shared pool.
async fn insert_checkpoint(state: &AppState, model_id: &str, promoted: bool, live_approved: bool) {
    sqlx::query(
        "INSERT INTO trained_models
            (model_id, display_name, run_tag, checkpoint_path, weights_sha256,
             input_spec, label_strategy, label_config, promoted, live_approved,
             created_at)
         VALUES (?, 'Test checkpoint', 'jun12a', '/tmp/ckpt', 'sha256abc',
                 '{\"window_bars\":64,\"indicators\":[],\"normalization\":\"zscore\"}',
                 'price_forward', '{}', ?, ?, '2026-06-14T00:00:00Z')",
    )
    .bind(model_id)
    .bind(if promoted { 1i64 } else { 0i64 })
    .bind(if live_approved { 1i64 } else { 0i64 })
    .execute(&state.pool)
    .await
    .unwrap();
}

/// Insert a dashboard session row and return the raw token.
async fn create_session_token(state: &AppState) -> String {
    use chrono::Utc;
    use xvision_dashboard::auth::session::{hash_token, insert_session};
    let token = "test-token-nanochat-unique-1234567890";
    let token_hash = hash_token(token);
    let expires_at = Utc::now() + chrono::Duration::hours(24);
    insert_session(&state.pool, &token_hash, &expires_at, None, None)
        .await
        .unwrap();
    token.to_string()
}

// ── GET /api/nanochat/checkpoints ────────────────────────────────────────────

#[tokio::test]
async fn list_returns_only_promoted_checkpoints() {
    let (server, _tmp, state) = boot().await;
    insert_checkpoint(&state, "model-promoted-01", true, false).await;
    insert_checkpoint(&state, "model-unpromoted-01", false, false).await;

    let res = server.get("/api/nanochat/checkpoints").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: Value = res.json();
    let items = body.as_array().expect("checkpoints array");
    assert_eq!(items.len(), 1, "only promoted checkpoints returned");
    assert_eq!(items[0]["model_id"], "model-promoted-01");
}

#[tokio::test]
async fn list_empty_when_no_promoted_checkpoints() {
    let (server, _tmp, state) = boot().await;
    insert_checkpoint(&state, "model-unpromoted-02", false, false).await;

    let res = server.get("/api/nanochat/checkpoints").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: Value = res.json();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

// ── GET /api/nanochat/checkpoints/:model_id ──────────────────────────────────

#[tokio::test]
async fn get_checkpoint_detail_returns_input_spec() {
    let (server, _tmp, state) = boot().await;
    insert_checkpoint(&state, "model-detail-01", true, false).await;

    let res = server.get("/api/nanochat/checkpoints/model-detail-01").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: Value = res.json();
    assert_eq!(body["model_id"], "model-detail-01");
    // input_spec must be present and parseable.
    let spec = body["input_spec"].as_str().expect("input_spec string");
    let _: Value = serde_json::from_str(spec).expect("input_spec is valid JSON");
}

#[tokio::test]
async fn get_checkpoint_detail_404_for_unknown() {
    let (server, _tmp, _state) = boot().await;
    let res = server.get("/api/nanochat/checkpoints/does-not-exist").await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

// ── POST /api/nanochat/checkpoints/:model_id/approve ─────────────────────────

#[tokio::test]
async fn approve_flips_live_approved_to_1() {
    let (server, _tmp, state) = boot().await;
    insert_checkpoint(&state, "model-approve-01", true, false).await;

    let res = server
        .post("/api/nanochat/checkpoints/model-approve-01/approve")
        .await;
    // TestServer defaults to loopback — passes through require_auth.
    assert_eq!(res.status_code(), StatusCode::OK);

    let live_approved: i64 =
        sqlx::query_scalar("SELECT live_approved FROM trained_models WHERE model_id = 'model-approve-01'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(live_approved, 1);
}

#[tokio::test]
async fn approve_double_approve_is_200_noop() {
    let (server, _tmp, state) = boot().await;
    insert_checkpoint(&state, "model-double-01", true, true).await; // already live_approved

    let res = server
        .post("/api/nanochat/checkpoints/model-double-01/approve")
        .await;
    // Idempotent: 200, not 409.
    assert_eq!(res.status_code(), StatusCode::OK);
}

/// Verify that approve returns 401 from a non-loopback client without a token.
/// Uses `tower::ServiceExt::oneshot` with a manually injected public-IP
/// ConnectInfo (the same pattern as auth_session.rs).
#[tokio::test]
async fn approve_requires_auth_from_non_loopback() {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf()).await.expect("init state");
    state.run_dashboard_migrations().await.unwrap();
    insert_checkpoint(&state, "model-noauth-01", true, false).await;

    let app = build_router(state);

    let mut request = Request::builder()
        .method("POST")
        .uri("/api/nanochat/checkpoints/model-noauth-01/approve")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();
    // Inject a public IP so require_auth_middleware sees a non-loopback client.
    request
        .extensions_mut()
        .insert(ConnectInfo::<SocketAddr>("203.0.113.5:12345".parse().unwrap()));

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Verify that approve with a valid session token from a non-loopback client succeeds.
#[tokio::test]
async fn approve_with_valid_token_from_non_loopback_succeeds() {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf()).await.expect("init state");
    state.run_dashboard_migrations().await.unwrap();
    insert_checkpoint(&state, "model-token-01", true, false).await;
    let token = create_session_token(&state).await;

    let app = build_router(state.clone());

    let mut request = Request::builder()
        .method("POST")
        .uri("/api/nanochat/checkpoints/model-token-01/approve")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo::<SocketAddr>("203.0.113.5:12345".parse().unwrap()));

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let live_approved: i64 =
        sqlx::query_scalar("SELECT live_approved FROM trained_models WHERE model_id = 'model-token-01'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(live_approved, 1);
}

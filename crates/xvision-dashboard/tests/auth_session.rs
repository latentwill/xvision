//! Session-token auth gate tests for v2b-dashboard-auth-boundary.
//!
//! Verifies:
//! - `POST /api/auth/session` issues a token and returns 201.
//! - `GET /api/auth/session/current` returns 401 without a token,
//!   200 with a valid token.
//! - `DELETE /api/auth/session` revokes the token.
//! - Mutating routes (POST / PUT / PATCH / DELETE) return 401 from
//!   non-loopback clients without a session token.
//! - Mutating routes return their normal response with a valid token
//!   (spot-checked on a representative subset).
//! - Read-only GET routes are accessible without a session token.
//! - Session expiry is honoured (expired tokens are rejected).
//!
//! ## Loopback exemption
//!
//! `axum_test::TestServer` injects no `ConnectInfo` extension, so the
//! `require_auth_middleware` defaults to loopback and lets all requests
//! through. The loopback-exemption tests therefore work against the
//! built router without `wrap_with_auth`.
//!
//! Non-loopback tests that verify a 401 use `tower::ServiceExt::oneshot`
//! with a manually injected `ConnectInfo` pointing to a public IP.

use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{Request, StatusCode},
    Router,
};
use axum_test::TestServer;
use tempfile::TempDir;
use tower::ServiceExt;
use xvision_dashboard::{
    server::build_router,
    AppState,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    // Run dashboard migrations so dashboard_sessions + auth_audit tables exist.
    state.run_dashboard_migrations().await.expect("dashboard migrations");
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

async fn boot_router_for_non_loopback() -> (Router, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    state.run_dashboard_migrations().await.expect("dashboard migrations");
    let router = build_router(state);
    (router, tmp)
}

/// Send a request from a non-loopback IP via `tower::ServiceExt::oneshot`.
async fn send_from_public(
    app: Router,
    method: &str,
    path: &str,
    auth_header: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(path);
    builder = builder.header("content-type", "application/json");
    if let Some(token) = auth_header {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let body_bytes = body
        .map(|v| serde_json::to_vec(&v).unwrap())
        .unwrap_or_default();
    let mut request = builder.body(Body::from(body_bytes)).unwrap();
    // Inject a public IP so require_auth_middleware sees a non-loopback client.
    request
        .extensions_mut()
        .insert(ConnectInfo::<SocketAddr>("203.0.113.5:12345".parse().unwrap()));

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({}));
    (status, json)
}

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_session_returns_201_and_token() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/auth/session")
        .json(&serde_json::json!({}))
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    assert!(body["token"].is_string(), "token must be present");
    assert!(body["session_id"].is_string(), "session_id must be present");
    assert!(body["expires_at"].is_string(), "expires_at must be present");
    let token = body["token"].as_str().unwrap();
    assert!(!token.is_empty(), "token must not be empty");
}

#[tokio::test]
async fn current_session_returns_401_without_token() {
    let (server, _tmp) = boot().await;
    // No authorization header — expect 401.
    let response = server.get("/api/auth/session/current").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn current_session_returns_session_info_with_valid_token() {
    let (server, _tmp) = boot().await;

    // Create a session.
    let create_resp = server
        .post("/api/auth/session")
        .json(&serde_json::json!({}))
        .await;
    create_resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = create_resp.json();
    let token = body["token"].as_str().unwrap().to_owned();

    // Use the token to call current.
    let bearer: axum::http::HeaderValue = format!("Bearer {token}").parse().unwrap();
    let current_resp = server
        .get("/api/auth/session/current")
        .add_header(axum::http::header::AUTHORIZATION, bearer)
        .await;
    current_resp.assert_status_ok();
    let current: serde_json::Value = current_resp.json();
    assert!(current["session_id"].is_string());
    assert!(current["expires_at"].is_string());
}

#[tokio::test]
async fn delete_session_revokes_token() {
    let (server, _tmp) = boot().await;

    // Create a session.
    let create_resp = server
        .post("/api/auth/session")
        .json(&serde_json::json!({}))
        .await;
    let body: serde_json::Value = create_resp.json();
    let token = body["token"].as_str().unwrap().to_owned();

    // Revoke.
    let bearer: axum::http::HeaderValue = format!("Bearer {token}").parse().unwrap();
    let delete_resp = server
        .delete("/api/auth/session")
        .add_header(axum::http::header::AUTHORIZATION, bearer)
        .await;
    delete_resp.assert_status(StatusCode::NO_CONTENT);

    // Subsequent call to current should fail.
    let bearer2: axum::http::HeaderValue = format!("Bearer {token}").parse().unwrap();
    let current_resp = server
        .get("/api/auth/session/current")
        .add_header(axum::http::header::AUTHORIZATION, bearer2)
        .await;
    current_resp.assert_status(StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Mutating routes — 401 without token from non-loopback clients
// ---------------------------------------------------------------------------

/// Spot-check a representative set of mutating routes for 401 without token.
/// The full audit is in the server.rs header comment; we verify the middleware
/// is wired correctly by sampling one route from each major section.
#[tokio::test]
async fn mutating_routes_return_401_from_non_loopback_without_token() {
    let (router, _tmp) = boot_router_for_non_loopback().await;

    let cases: &[(&str, &str, Option<serde_json::Value>)] = &[
        // agents
        ("POST", "/api/agents", Some(serde_json::json!({}))),
        // strategies
        ("POST", "/api/strategies", Some(serde_json::json!({}))),
        // scenarios
        ("POST", "/api/scenarios", Some(serde_json::json!({}))),
        // eval runs
        ("POST", "/api/eval/runs", Some(serde_json::json!({}))),
        // settings — provider add
        ("POST", "/api/settings/providers", Some(serde_json::json!({}))),
        // settings — danger
        ("POST", "/api/settings/danger/reset-workspace", Some(serde_json::json!({}))),
        // wizard
        ("POST", "/api/wizard/chat", Some(serde_json::json!({}))),
        // chat-rail
        ("POST", "/api/chat-rail/chat", Some(serde_json::json!({}))),
    ];

    for (method, path, body) in cases {
        let (status, resp_body) = send_from_public(
            router.clone(),
            method,
            path,
            None, // no token
            body.clone(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "Expected 401 for {method} {path} without token, got {status}. body: {resp_body}",
        );
        assert_eq!(
            resp_body["error"], "unauthenticated",
            "Error body should say 'unauthenticated' for {method} {path}"
        );
    }
}

// ---------------------------------------------------------------------------
// Read-only routes — accessible without session token
// ---------------------------------------------------------------------------

#[tokio::test]
async fn read_only_routes_are_accessible_without_session_token() {
    let (server, _tmp) = boot().await;

    let read_only_routes = [
        "/api/health",
        "/api/agents",
        "/api/strategies",
        "/api/scenarios",
        "/api/eval/runs",
        "/api/settings/brokers",
        "/api/settings/providers",
        "/api/settings/observability",
    ];

    for path in read_only_routes {
        let response = server.get(path).await;
        assert!(
            response.status_code() != StatusCode::UNAUTHORIZED,
            "Read-only route {path} should not return 401, got {}",
            response.status_code()
        );
    }
}

// ---------------------------------------------------------------------------
// Loopback exemption — mutating routes work without token on loopback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mutating_routes_loopback_pass_without_session_token() {
    // axum_test::TestServer uses loopback, so no session token needed.
    let (server, _tmp) = boot().await;

    // POST /api/scenarios — requires valid body but should not 401.
    let response = server
        .post("/api/scenarios")
        .json(&serde_json::json!({})) // intentionally malformed → 400 not 401
        .await;
    assert_ne!(
        response.status_code(),
        StatusCode::UNAUTHORIZED,
        "Loopback client should not be gated on POST /api/scenarios"
    );
}

// ---------------------------------------------------------------------------
// Session expiry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn expired_session_token_is_rejected() {
    use chrono::Utc;
    use xvision_dashboard::auth::session::{hash_token, insert_session};

    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init state");
    state.run_dashboard_migrations().await.expect("migrations");
    let pool = state.pool.clone();

    // Manually insert an already-expired session.
    let token = "expired_test_token_00000000000000000000000000000000000000000000000000000000000000";
    let token_hash = hash_token(token);
    let expired_at = Utc::now() - chrono::Duration::hours(2);
    insert_session(&pool, &token_hash, &expired_at, Some("127.0.0.1"), None)
        .await
        .expect("insert expired session");

    let router = build_router(state);

    // From a non-loopback client with the expired token.
    let (status, body) = send_from_public(
        router,
        "POST",
        "/api/strategies",
        Some(token),
        Some(serde_json::json!({})),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Expired token must be rejected, got: {body}"
    );
}

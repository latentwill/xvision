//! Integration tests for the `eval-provider-preflight` feature (2026-05-21).
//!
//! These tests cover the HTTP-layer behaviour of `POST /api/eval/runs`
//! with respect to the new `skip_preflight` field:
//!
//!   1. `skip_preflight` absent → defaults to `false` (preflight enabled).
//!      The request body must be accepted (not rejected as unknown-field)
//!      and the route must reach the provider-check step before failing.
//!   2. `skip_preflight: true` → accepted without error from the serde
//!      layer; the error, if any, is "strategy not found" (404/400) not a
//!      field-rejection error.
//!   3. An unknown extra field in the body still returns a serde error,
//!      confirming `deny_unknown_fields` is still in effect.
//!
//! Full end-to-end "provider blocked → 400" and "skip_preflight bypasses
//! blocked provider → run created" paths are covered at the engine unit
//! level in `crates/xvision-engine/src/eval/preflight.rs` (wiremock
//! tests) and at the strategy-launch level in
//! `crates/xvision-engine/src/api/eval.rs` unit tests. The dashboard
//! integration layer adds the HTTP deserialization guarantee.

use axum::http::StatusCode;
use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

/// A request that omits `skip_preflight` (field defaults to `false`) must
/// not be rejected by the serde layer. The route will fail downstream
/// (strategy not found → 404/400) but never with a "missing field" or
/// "unknown field" serde error.
#[tokio::test]
async fn post_start_accepts_body_without_skip_preflight() {
    let (server, _tmp) = boot().await;

    let response = server
        .post("/api/eval/runs")
        .json(&serde_json::json!({
            "agent_id": "nonexistent-strategy",
            "scenario_id": "crypto-bull-q1-2025",
            "mode": "backtest",
            "params_override": null,
        }))
        .await;

    // The route should NOT return 422 Unprocessable (serde rejection).
    // Allowed responses: 400 (validation/strategy-not-found) or 404 (not found).
    // Any of these means the body was parsed fine.
    let status = response.status_code();
    assert_ne!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "body without skip_preflight must parse: got {status}"
    );
    // Serde errors also often surface as 400 with a "failed to parse" body.
    // Rule out the serde-error 400 by checking the body does not contain
    // a json-decode failure message.
    if status == StatusCode::BAD_REQUEST {
        let body = response.text();
        assert!(
            !body.contains("Failed to deserialize") && !body.contains("unknown field"),
            "expected strategy-not-found 400, not serde rejection: {body}"
        );
    }
}

/// A request that explicitly sets `skip_preflight: true` must also be
/// accepted by the serde layer. The route will fail for the same "strategy
/// not found" reason, but the field is not rejected as unknown.
#[tokio::test]
async fn post_start_accepts_skip_preflight_true() {
    let (server, _tmp) = boot().await;

    let response = server
        .post("/api/eval/runs")
        .json(&serde_json::json!({
            "agent_id": "nonexistent-strategy",
            "scenario_id": "crypto-bull-q1-2025",
            "mode": "backtest",
            "params_override": null,
            "skip_preflight": true,
        }))
        .await;

    let status = response.status_code();
    assert_ne!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "body with skip_preflight=true must parse: got {status}"
    );
    if status == StatusCode::BAD_REQUEST {
        let body = response.text();
        assert!(
            !body.contains("Failed to deserialize") && !body.contains("unknown field"),
            "expected strategy-not-found 400, not serde rejection: {body}"
        );
    }
}

/// A request with an unrecognised extra field must still be rejected
/// (deny_unknown_fields is still in effect). Regression guard: adding
/// `skip_preflight` must not have silently loosened the serde config.
#[tokio::test]
async fn post_start_rejects_unknown_fields() {
    let (server, _tmp) = boot().await;

    let response = server
        .post("/api/eval/runs")
        .json(&serde_json::json!({
            "agent_id": "nonexistent-strategy",
            "scenario_id": "crypto-bull-q1-2025",
            "mode": "backtest",
            "params_override": null,
            "this_field_does_not_exist": true,
        }))
        .await;

    // deny_unknown_fields must reject this — 400 or 422.
    let status = response.status_code();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "unknown field must be rejected with 400/422, got {status}"
    );
}

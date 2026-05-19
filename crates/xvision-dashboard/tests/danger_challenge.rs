//! End-to-end tests for `qa-dashboard-auth-hardening` danger-phrase
//! verification. Each route now requires its own typed phrase; the
//! prior `yes-i-am-sure` token is no longer accepted.
//!
//! Updated for F-4 (2026-05-18): the legacy `wipe_db` route has been
//! replaced by the selective `reset_workspace` op. The
//! `WIPE DATABASE` phrase is no longer recognized by any route.

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::api::settings::danger::{FACTORY_RESET_CONFIRM, RESET_WORKSPACE_CONFIRM};

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

#[tokio::test]
async fn reset_workspace_rejects_legacy_token() {
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/reset-workspace")
        .json(&serde_json::json!({ "confirm": "yes-i-am-sure" }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.text();
    assert!(
        body.contains(RESET_WORKSPACE_CONFIRM),
        "rejection must guide operator to the new phrase, got: {body}"
    );
}

#[tokio::test]
async fn reset_workspace_rejects_legacy_wipe_db_phrase() {
    // F-4: `WIPE DATABASE` no longer satisfies any route — the
    // selective reset uses a distinct phrase that signals scope.
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/reset-workspace")
        .json(&serde_json::json!({ "confirm": "WIPE DATABASE" }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.text();
    assert!(
        body.contains(RESET_WORKSPACE_CONFIRM),
        "rejection must point operator at RESET WORKSPACE, got: {body}"
    );
}

#[tokio::test]
async fn reset_workspace_accepts_correct_phrase() {
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/reset-workspace")
        .json(&serde_json::json!({ "confirm": RESET_WORKSPACE_CONFIRM }))
        .await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn factory_reset_accepts_correct_phrase() {
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/factory-reset")
        .json(&serde_json::json!({ "confirm": FACTORY_RESET_CONFIRM }))
        .await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn factory_reset_rejects_legacy_token() {
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/factory-reset")
        .json(&serde_json::json!({ "confirm": "yes-i-am-sure" }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.text();
    assert!(
        body.contains(FACTORY_RESET_CONFIRM),
        "rejection must guide operator to the factory reset phrase, got: {body}"
    );
}

#[tokio::test]
async fn factory_reset_rejects_reset_workspace_phrase() {
    // Per-route phrases must defend against a single typed string
    // accidentally firing the wrong destructive op. The
    // RESET_WORKSPACE phrase must not satisfy factory_reset.
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/factory-reset")
        .json(&serde_json::json!({ "confirm": RESET_WORKSPACE_CONFIRM }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.text();
    assert!(body.contains(FACTORY_RESET_CONFIRM));
}

#[tokio::test]
async fn reset_workspace_rejects_empty_confirm() {
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/reset-workspace")
        .json(&serde_json::json!({ "confirm": "" }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn wipe_db_route_is_gone() {
    // F-4: the old route must 404 — clients hitting `wipe-db` after
    // this PR ships should get a clean signal rather than silently
    // dropping their action.
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/wipe-db")
        .json(&serde_json::json!({ "confirm": "WIPE DATABASE" }))
        .await;
    resp.assert_status(axum::http::StatusCode::NOT_FOUND);
}

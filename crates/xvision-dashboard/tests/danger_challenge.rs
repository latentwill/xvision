//! End-to-end tests for `qa-dashboard-auth-hardening` danger-phrase
//! verification. Each route now requires its own typed phrase; the
//! prior `yes-i-am-sure` token is no longer accepted.

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::api::settings::danger::{
    FACTORY_RESET_CONFIRM, WIPE_DB_CONFIRM,
};

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

#[tokio::test]
async fn wipe_db_rejects_legacy_token() {
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/wipe-db")
        .json(&serde_json::json!({ "confirm": "yes-i-am-sure" }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.text();
    assert!(
        body.contains(WIPE_DB_CONFIRM),
        "rejection must guide operator to the new phrase, got: {body}"
    );
}

#[tokio::test]
async fn wipe_db_accepts_correct_phrase() {
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/wipe-db")
        .json(&serde_json::json!({ "confirm": WIPE_DB_CONFIRM }))
        .await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn factory_reset_rejects_wipe_db_phrase() {
    // Per-route phrases must defend against a single typed string
    // accidentally firing the wrong destructive op. The WIPE_DB phrase
    // must not satisfy factory_reset.
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/factory-reset")
        .json(&serde_json::json!({ "confirm": WIPE_DB_CONFIRM }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.text();
    assert!(body.contains(FACTORY_RESET_CONFIRM));
}

#[tokio::test]
async fn wipe_db_rejects_empty_confirm() {
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/settings/danger/wipe-db")
        .json(&serde_json::json!({ "confirm": "" }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

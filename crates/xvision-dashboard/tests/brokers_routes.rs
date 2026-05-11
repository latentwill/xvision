//! Integration tests for the Alpaca-credential CRUD routes added
//! alongside the broker-settings persistence work. Validates that
//! POST /api/settings/brokers/alpaca writes the secrets file, the
//! follow-up GET reflects the redacted summary, and DELETE clears it.

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

#[tokio::test]
async fn post_alpaca_persists_and_get_reports_redacted() {
    let (server, _tmp, _state) = boot().await;

    let resp = server
        .post("/api/settings/brokers/alpaca")
        .json(&serde_json::json!({
            "api_key_id": "PKEXAMPLE00000001",
            "api_secret_key": "supersecretsecret",
            "base_url": null
        }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["stored"], true);
    assert_eq!(body["stored_key_id_suffix"], "0001");

    let snapshot = server.get("/api/settings/brokers").await;
    snapshot.assert_status_ok();
    let snap_body: serde_json::Value = snapshot.json();
    assert_eq!(snap_body["alpaca"]["stored"], true);
    assert_eq!(snap_body["alpaca"]["configured"], true);
    assert_eq!(snap_body["alpaca"]["stored_key_id_suffix"], "0001");
    // Critical: the secret never appears in the read surface.
    let serialized = snap_body.to_string();
    assert!(!serialized.contains("supersecretsecret"));
    assert!(!serialized.contains("PKEXAMPLE00000001"));
}

#[tokio::test]
async fn post_alpaca_rejects_empty_fields() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .post("/api/settings/brokers/alpaca")
        .json(&serde_json::json!({
            "api_key_id": "",
            "api_secret_key": "supersecretsecret",
            "base_url": null
        }))
        .await;
    resp.assert_status_bad_request();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn delete_alpaca_clears_stored_state() {
    let (server, _tmp, _state) = boot().await;
    server
        .post("/api/settings/brokers/alpaca")
        .json(&serde_json::json!({
            "api_key_id": "PKEXAMPLE00000002",
            "api_secret_key": "secret-2",
            "base_url": "https://paper-api.alpaca.markets"
        }))
        .await
        .assert_status(axum::http::StatusCode::CREATED);

    let del = server.delete("/api/settings/brokers/alpaca").await;
    del.assert_status(axum::http::StatusCode::NO_CONTENT);

    let snap = server.get("/api/settings/brokers").await;
    let body: serde_json::Value = snap.json();
    assert_eq!(body["alpaca"]["stored"], false);
    assert_eq!(body["alpaca"]["stored_key_id_suffix"], serde_json::Value::Null);
}

#[tokio::test]
async fn delete_alpaca_is_idempotent_on_fresh_home() {
    let (server, _tmp, _state) = boot().await;
    let del = server.delete("/api/settings/brokers/alpaca").await;
    del.assert_status(axum::http::StatusCode::NO_CONTENT);
}

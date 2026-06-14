//! WU7 — Route tests for `POST /api/strategy/import/pine`.
//!
//! Tests:
//! 1. A valid Pine source → 200 with `{ strategy, fidelity_report }` both present.
//! 2. A malformed Pine source → 400 with a structured error.
//! 3. Optional `name` override applies to the returned strategy's display_name.

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

// Valid Pine source that exercises the import path.
const VALID_PINE_SRC: &str = r#"//@version=5
strategy("RSI Test", overlay=false)
rsi_val = ta.rsi(close, 14)
if rsi_val < 30
    strategy.entry("Long", strategy.long)
strategy.exit("Long Exit", "Long", loss=2.0, profit=4.0)
"#;

// Malformed Pine source (unclosed parenthesis in header call).
const MALFORMED_PINE_SRC: &str = "//@version=5\nstrategy(\"Broken\", overlay=true\n";

// ── Test 1: valid source → 200 with strategy + fidelity_report ────────────────

#[tokio::test]
async fn post_import_pine_valid_source_returns_200() {
    let (server, _dir) = boot().await;

    let body = serde_json::json!({ "source": VALID_PINE_SRC });
    let response = server.post("/api/strategy/import/pine").json(&body).await;

    response.assert_status(axum::http::StatusCode::OK);

    let json: serde_json::Value = response.json();

    assert!(
        json.get("strategy").is_some(),
        "response must have `strategy` key; got: {json}"
    );
    assert!(
        json.get("fidelity_report").is_some(),
        "response must have `fidelity_report` key; got: {json}"
    );

    // fidelity_report must have captured/approximated/dropped keys
    let report = json["fidelity_report"]
        .as_object()
        .expect("fidelity_report must be object");
    assert!(
        report.contains_key("captured"),
        "fidelity_report must have `captured`; got: {report:?}"
    );
    assert!(
        report.contains_key("approximated"),
        "fidelity_report must have `approximated`; got: {report:?}"
    );
    assert!(
        report.contains_key("dropped"),
        "fidelity_report must have `dropped`; got: {report:?}"
    );
}

// ── Test 2: malformed source → 400 ────────────────────────────────────────────

#[tokio::test]
async fn post_import_pine_malformed_source_returns_400() {
    let (server, _dir) = boot().await;

    let body = serde_json::json!({ "source": MALFORMED_PINE_SRC });
    let response = server.post("/api/strategy/import/pine").json(&body).await;

    response.assert_status(axum::http::StatusCode::BAD_REQUEST);

    let json: serde_json::Value = response.json();

    // Error response should have some error indicator
    let has_error =
        json.get("code").is_some() || json.get("error").is_some() || json.get("message").is_some();
    assert!(has_error, "400 response must have structured error; got: {json}");
}

// ── Test 3: optional name override ────────────────────────────────────────────

#[tokio::test]
async fn post_import_pine_name_override_applied() {
    let (server, _dir) = boot().await;

    let custom_name = "Custom Override Name";
    let body = serde_json::json!({
        "source": VALID_PINE_SRC,
        "name": custom_name,
    });
    let response = server.post("/api/strategy/import/pine").json(&body).await;

    response.assert_status(axum::http::StatusCode::OK);

    let json: serde_json::Value = response.json();

    let strategy_name = json["strategy"]["manifest"]["display_name"]
        .as_str()
        .expect("strategy.manifest.display_name must be a string");
    assert_eq!(
        strategy_name, custom_name,
        "display_name must match the `name` field in request body"
    );
}

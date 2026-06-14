//! WU9 — Route tests for `GET /api/strategy/pine-library` and
//! `POST /api/strategy/pine-library/{id}/import`.
//!
//! Tests:
//! 1. GET /api/strategy/pine-library → 200 with ≥10 summaries
//!    (each summary has `id`, `name`, `description`; no `source` field).
//! 2. POST /api/strategy/pine-library/{id}/import with a known id → 200
//!    with `{ strategy, fidelity_report }`.
//! 3. POST /api/strategy/pine-library/{id}/import with an unknown id → 404.

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

// ── Test 1: GET pine-library → 200 with ≥10 summaries ────────────────────────

#[tokio::test]
async fn get_pine_library_returns_200_with_at_least_ten_summaries() {
    let (server, _dir) = boot().await;

    let response = server.get("/api/strategy/pine-library").await;

    response.assert_status(axum::http::StatusCode::OK);

    let json: serde_json::Value = response.json();

    let items = json
        .get("items")
        .and_then(|v| v.as_array())
        .expect("response must have `items` array");

    assert!(
        items.len() >= 10,
        "GET /api/strategy/pine-library must return ≥10 items; got {}",
        items.len()
    );

    // Each item must have id, name, description — but NO source field.
    for item in items {
        assert!(
            item.get("id")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false),
            "each library item must have non-empty `id`; got: {item}"
        );
        assert!(
            item.get("name")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false),
            "each library item must have non-empty `name`; got: {item}"
        );
        assert!(
            item.get("description")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false),
            "each library item must have non-empty `description`; got: {item}"
        );
        assert!(
            item.get("source").is_none(),
            "library list summaries must NOT expose `source`; got: {item}"
        );
    }
}

// ── Test 2: POST pine-library/{id}/import → 200 with strategy + fidelity ─────

#[tokio::test]
async fn post_import_known_library_entry_returns_200() {
    let (server, _dir) = boot().await;

    // Use a known id from the library.
    let response = server
        .post("/api/strategy/pine-library/rsi-threshold/import")
        .await;

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

    // fidelity_report must have captured/approximated/dropped keys.
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

// ── Test 3: POST pine-library/{id}/import with unknown id → 404 ──────────────

#[tokio::test]
async fn post_import_unknown_library_entry_returns_404() {
    let (server, _dir) = boot().await;

    let response = server
        .post("/api/strategy/pine-library/does-not-exist-xyz/import")
        .await;

    response.assert_status(axum::http::StatusCode::NOT_FOUND);
}

// ── Test 4: GET returns a stable, consistent list (spot-check known id) ───────

#[tokio::test]
async fn get_pine_library_includes_known_entry_ids() {
    let (server, _dir) = boot().await;

    let response = server.get("/api/strategy/pine-library").await;
    response.assert_status(axum::http::StatusCode::OK);

    let json: serde_json::Value = response.json();
    let items = json["items"].as_array().expect("`items` array");

    let ids: Vec<&str> = items.iter().filter_map(|item| item["id"].as_str()).collect();

    // At minimum, the two canonical starter entries must be present.
    assert!(
        ids.contains(&"rsi-threshold"),
        "library must include `rsi-threshold`; got ids: {ids:?}"
    );
    assert!(
        ids.contains(&"ma-crossover"),
        "library must include `ma-crossover`; got ids: {ids:?}"
    );
}

// ── Test 5: POST import ma-crossover entry → strategy + fidelity ──────────────

#[tokio::test]
async fn post_import_ma_crossover_returns_strategy() {
    let (server, _dir) = boot().await;

    let response = server
        .post("/api/strategy/pine-library/ma-crossover/import")
        .await;

    response.assert_status(axum::http::StatusCode::OK);

    let json: serde_json::Value = response.json();

    // Strategy manifest must have a non-empty id.
    let strategy_id = json["strategy"]["manifest"]["id"]
        .as_str()
        .expect("strategy.manifest.id must be a string");
    assert!(
        !strategy_id.is_empty(),
        "imported library strategy must have non-empty manifest.id"
    );
}

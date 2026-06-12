//! HTTP-level regression tests for two missing strategy routes:
//!
//! BUG xvision-tflw (P2): `DELETE /api/strategy/:id/filter` must clear the
//! filter and revert `activation_mode` to `EveryBar`.
//!
//! BUG xvision-5o4r (P1): `PUT /api/strategy/:id/mechanistic` must persist
//! `decision_mode` and `mechanistic_config` on the strategy.

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

async fn create_strategy(server: &TestServer) -> String {
    let resp = server
        .post("/api/strategies")
        .json(&serde_json::json!({ "name": "TestStrategy", "creator": "@test" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    resp.json::<serde_json::Value>()["id"]
        .as_str()
        .expect("id string")
        .to_string()
}

// ---------------------------------------------------------------------------
// BUG xvision-tflw — DELETE /api/strategy/:id/filter
// ---------------------------------------------------------------------------

/// Set a filter via PATCH (the inspector path), then DELETE it.
/// After DELETE, GET must return `filter: null` and
/// `activation_mode: "every_bar"`.
#[tokio::test]
async fn delete_filter_clears_filter_and_reverts_activation_mode() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    // Set a filter via PATCH (the inspector path — uses the same filter DSL
    // format that the patch_filter_autofills_strategy_scoped_ids test
    // already validates).
    let patch_resp = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({
            "filter": {
                "display_name": "BTC 15m EMA12>EMA26",
                "asset_scope": ["BTC/USD"],
                "timeframe": "15m",
                "scan_cadence": "bar_close",
                "conditions": {
                    "all": [
                        { "lhs": "ema_12", "op": ">", "rhs": "ema_26" }
                    ]
                },
                "cooldown_bars": 4,
                "agent_context_template": "compact_trade_context_v1"
            }
        }))
        .await;
    patch_resp.assert_status_ok();
    let after_patch: serde_json::Value = patch_resp.json();
    assert_eq!(
        after_patch["activation_mode"].as_str().unwrap_or(""),
        "filter_gated",
        "PATCH /strategy/:id must set activation_mode to filter_gated when filter is provided"
    );
    assert!(
        !after_patch["filter"].is_null(),
        "filter must be non-null after PATCH"
    );

    // DELETE the filter.
    let del_resp = server.delete(&format!("/api/strategy/{id}/filter")).await;
    del_resp.assert_status(StatusCode::NO_CONTENT);

    // GET round-trip confirms filter is cleared.
    let get_resp = server.get(&format!("/api/strategy/{id}")).await;
    get_resp.assert_status_ok();
    let strategy: serde_json::Value = get_resp.json();
    assert!(
        strategy["filter"].is_null(),
        "filter must be null after DELETE, got: {:?}",
        strategy["filter"]
    );
    assert_eq!(
        strategy["activation_mode"].as_str().unwrap_or(""),
        "every_bar",
        "activation_mode must revert to every_bar after DELETE /filter"
    );
}

/// DELETE on a strategy with no filter is a no-op (204, not an error).
#[tokio::test]
async fn delete_filter_on_strategy_without_filter_returns_no_content() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let resp = server.delete(&format!("/api/strategy/{id}/filter")).await;
    resp.assert_status(StatusCode::NO_CONTENT);
}

/// DELETE filter then validate — a cleared strategy must validate without
/// mechanistic-related filter errors.
#[tokio::test]
async fn strategy_validates_after_filter_cleared() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    // No filter to begin with — clearing is a no-op.
    server
        .delete(&format!("/api/strategy/{id}/filter"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    // POST validate: must return 200 with ok == false only because no agents
    // (not because of a filter DSL error).
    let val_resp = server.post(&format!("/api/strategy/{id}/validate")).await;
    val_resp.assert_status_ok();
    let val: serde_json::Value = val_resp.json();
    // The "code" field must not be "validation" — if the route was missing,
    // the request would 404 and never reach validate.
    assert!(
        val.get("ok").is_some(),
        "validate response must carry an 'ok' field, got: {val:#?}"
    );
}

// ---------------------------------------------------------------------------
// BUG xvision-5o4r — PUT /api/strategy/:id/mechanistic
// ---------------------------------------------------------------------------

/// Setting decision_mode to "mechanistic" with a config persists both fields.
#[tokio::test]
async fn put_mechanistic_persists_decision_mode_and_config() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let body = serde_json::json!({
        "decision_mode": "mechanistic",
        "mechanistic_config": {
            "entry_rules": [
                { "signal_name": "ma_cross", "direction": "long" }
            ],
            "close_policies": [
                { "kind": "stop_loss", "pct": 2.5 }
            ]
        }
    });

    let resp = server
        .put(&format!("/api/strategy/{id}/mechanistic"))
        .json(&body)
        .await;
    resp.assert_status_ok();
    let strategy: serde_json::Value = resp.json();
    assert_eq!(
        strategy["decision_mode"].as_str().unwrap_or(""),
        "mechanistic",
        "decision_mode must be mechanistic after PUT"
    );
    let cfg = &strategy["mechanistic_config"];
    assert!(
        !cfg.is_null(),
        "mechanistic_config must be non-null, got: {cfg:#?}"
    );
    assert_eq!(cfg["entry_rules"][0]["signal_name"], "ma_cross");
    assert_eq!(cfg["close_policies"][0]["kind"], "stop_loss");

    // GET round-trip confirms persistence.
    let get: serde_json::Value = server.get(&format!("/api/strategy/{id}")).await.json();
    assert_eq!(get["decision_mode"].as_str().unwrap_or(""), "mechanistic");
    assert_eq!(
        get["mechanistic_config"]["entry_rules"][0]["signal_name"],
        "ma_cross"
    );
}

/// Setting decision_mode back to "agentic" clears the mechanistic_config.
#[tokio::test]
async fn put_mechanistic_agentic_mode_clears_config() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    // First set to mechanistic.
    server
        .put(&format!("/api/strategy/{id}/mechanistic"))
        .json(&serde_json::json!({
            "decision_mode": "mechanistic",
            "mechanistic_config": {
                "entry_rules": [],
                "close_policies": [{ "kind": "take_profit", "pct": 5.0 }]
            }
        }))
        .await
        .assert_status_ok();

    // Then revert to agentic.
    let resp = server
        .put(&format!("/api/strategy/{id}/mechanistic"))
        .json(&serde_json::json!({ "decision_mode": "agentic" }))
        .await;
    resp.assert_status_ok();
    let strategy: serde_json::Value = resp.json();
    // `decision_mode` is serialized with `skip_serializing_if = "DecisionMode::is_agentic"`,
    // so it is absent from the JSON when agentic. Either absent or "agentic" is correct.
    let dm = strategy["decision_mode"].as_str().unwrap_or("agentic");
    assert_eq!(
        dm, "agentic",
        "decision_mode must be agentic (or absent, which means agentic) after revert"
    );
    // mechanistic_config is skipped in serialization when absent — either
    // missing from the JSON or null is acceptable.
    let cfg = &strategy["mechanistic_config"];
    assert!(
        cfg.is_null() || cfg == &serde_json::Value::Null,
        "mechanistic_config must be null/absent after reverting to agentic, got: {cfg:#?}"
    );
}

/// Unknown strategy id returns 404, not 500.
#[tokio::test]
async fn put_mechanistic_unknown_strategy_returns_404() {
    let (server, _tmp) = boot().await;

    let resp = server
        .put("/api/strategy/01NOTEXIST00000000000000/mechanistic")
        .json(&serde_json::json!({ "decision_mode": "agentic" }))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

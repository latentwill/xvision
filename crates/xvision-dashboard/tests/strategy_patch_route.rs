//! HTTP-level coverage for `PATCH /api/strategy/:id` — the metadata
//! patch surface added by the
//! `strategy-edit-top-level-fields` track (QA operator round 4, item 2).
//!
//! The patch covers `display_name`, `plain_summary`, and
//! `asset_universe`. Anything else (id, creator, template,
//! published_at, agents, pipeline, risk) is out of scope and has its
//! own route or is immutable post-create.

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
    let response = server
        .post("/api/strategies")
        .json(&serde_json::json!({
            "name": "PatchMe",
            "creator": "@operator"
        }))
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json::<serde_json::Value>()["id"]
        .as_str()
        .expect("created strategy returns id")
        .to_string()
}

#[tokio::test]
async fn patch_metadata_updates_display_name_and_preserves_id() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({ "display_name": "Renamed Strategy" }))
        .await;
    response.assert_status_ok();
    let updated: serde_json::Value = response.json();
    assert_eq!(updated["manifest"]["id"], id, "id must be stable across patch");
    assert_eq!(updated["manifest"]["display_name"], "Renamed Strategy");

    // GET round-trip — the dashboard surface and the on-disk store
    // both reflect the new display_name.
    let response = server.get(&format!("/api/strategy/{id}")).await;
    response.assert_status_ok();
    let fetched: serde_json::Value = response.json();
    assert_eq!(fetched["manifest"]["id"], id);
    assert_eq!(fetched["manifest"]["display_name"], "Renamed Strategy");
}

#[tokio::test]
async fn patch_metadata_with_empty_body_is_noop_returning_200() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let before = server.get(&format!("/api/strategy/{id}")).await;
    let before_body: serde_json::Value = before.json();

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({}))
        .await;
    response.assert_status_ok();
    let after_body: serde_json::Value = response.json();
    // No-op: every relevant field is identical pre/post.
    assert_eq!(
        before_body["manifest"]["display_name"],
        after_body["manifest"]["display_name"]
    );
    assert_eq!(
        before_body["manifest"]["plain_summary"],
        after_body["manifest"]["plain_summary"]
    );
    assert_eq!(
        before_body["manifest"]["asset_universe"],
        after_body["manifest"]["asset_universe"]
    );
    assert_eq!(before_body["manifest"]["id"], after_body["manifest"]["id"]);
}

#[tokio::test]
async fn patch_filter_autofills_strategy_scoped_ids() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({
            "filter": {
                "display_name": "BTC 15m EMA12>EMA26 + RSI throttle",
                "asset_scope": ["BTC/USD"],
                "timeframe": "15m",
                "scan_cadence": "bar_close",
                "conditions": {
                    "all": [
                        { "lhs": "ema_12", "op": ">", "rhs": "ema_26" },
                        { "lhs": "close", "op": "crosses_above", "rhs": "ema_12" },
                        { "lhs": "rsi_14", "op": "between", "rhs": [55, 75] }
                    ]
                },
                "cooldown_bars": 12,
                "max_wakeups_per_day": 4,
                "wake_when_in_position": "on_invalidation_or_target_only",
                "agent_context_template": "compact_trade_context_v1"
            }
        }))
        .await;
    response.assert_status_ok();
    let updated: serde_json::Value = response.json();

    assert_eq!(updated["activation_mode"], "filter_gated");
    assert_eq!(updated["filter"]["strategy_id"], id);
    assert!(
        updated["filter"]["id"].as_str().unwrap_or_default().len() >= 20,
        "backend should assign a persisted filter id, got: {}",
        updated["filter"]["id"]
    );
    assert_eq!(
        updated["filter"]["display_name"],
        "BTC 15m EMA12>EMA26 + RSI throttle"
    );
}

#[tokio::test]
async fn patch_metadata_empty_display_name_returns_classified_400() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({ "display_name": "   " }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["code"], "validation",
        "expected classified validation error, got {body}"
    );
    let msg = body["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("display_name"),
        "operator-readable remediation must reference the field, got: {msg}"
    );

    // Disk side-effect check: the stored display_name did NOT change.
    let response = server.get(&format!("/api/strategy/{id}")).await;
    let fetched: serde_json::Value = response.json();
    assert_eq!(
        fetched["manifest"]["display_name"], "PatchMe",
        "rejected patch must not partially mutate disk state"
    );
}

#[tokio::test]
async fn patch_metadata_rejects_blank_plain_summary() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({ "plain_summary": "" }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
    assert!(body["message"]
        .as_str()
        .unwrap_or_default()
        .contains("plain_summary"));
}

#[tokio::test]
async fn patch_metadata_rejects_invalid_asset_symbol() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({ "asset_universe": ["just-a-word"] }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
    let msg = body["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("asset_universe"),
        "remediation must surface the asset_universe field, got: {msg}"
    );
}

#[tokio::test]
async fn patch_metadata_rejects_out_of_scope_fields() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let before = server.get(&format!("/api/strategy/{id}")).await;
    before.assert_status_ok();
    let before_body: serde_json::Value = before.json();

    let cases = [
        serde_json::json!({
            "display_name": "ShouldNotPersist",
            "id": "01JSHOULDNOTCHANGE0000000000",
        }),
        serde_json::json!({
            "display_name": "ShouldNotPersist",
            "creator": "@other",
        }),
        serde_json::json!({
            "display_name": "ShouldNotPersist",
            "template": "other_template",
        }),
        serde_json::json!({
            "display_name": "ShouldNotPersist",
            "unknown_patch_key": true,
        }),
    ];

    for payload in cases {
        let response = server.patch(&format!("/api/strategy/{id}")).json(&payload).await;
        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

        let response = server.get(&format!("/api/strategy/{id}")).await;
        response.assert_status_ok();
        let fetched: serde_json::Value = response.json();
        assert_eq!(
            fetched["manifest"], before_body["manifest"],
            "out-of-scope payload must not mutate manifest fields: {payload}"
        );
    }
}

#[tokio::test]
async fn patch_metadata_invalid_multi_field_patch_is_atomic() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let before = server.get(&format!("/api/strategy/{id}")).await;
    before.assert_status_ok();
    let before_body: serde_json::Value = before.json();

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({
            "display_name": "ShouldNotPersist",
            "plain_summary": "This should not be persisted.",
            "asset_universe": ["just-a-word"],
        }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("asset_universe"),
        "rejected multi-field patch should name the invalid field, got: {body}"
    );

    let response = server.get(&format!("/api/strategy/{id}")).await;
    response.assert_status_ok();
    let fetched: serde_json::Value = response.json();
    assert_eq!(
        fetched["manifest"]["display_name"], before_body["manifest"]["display_name"],
        "valid display_name in a rejected patch must not be partially written"
    );
    assert_eq!(
        fetched["manifest"]["plain_summary"], before_body["manifest"]["plain_summary"],
        "valid plain_summary in a rejected patch must not be partially written"
    );
    assert_eq!(
        fetched["manifest"]["asset_universe"], before_body["manifest"]["asset_universe"],
        "valid fields in a rejected patch must not be partially written"
    );
}

#[tokio::test]
async fn patch_metadata_normalizes_and_dedupes_asset_universe() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({
            "asset_universe": ["eth/usd", "BTC/usd", "ETH/USD"]
        }))
        .await;
    response.assert_status_ok();
    let updated: serde_json::Value = response.json();
    let assets = updated["manifest"]["asset_universe"]
        .as_array()
        .expect("asset_universe must be an array");
    let symbols: Vec<&str> = assets.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(symbols, vec!["ETH/USD", "BTC/USD"]);
}

#[tokio::test]
async fn patch_metadata_unknown_strategy_returns_404() {
    let (server, _tmp) = boot().await;

    let response = server
        .patch("/api/strategy/01J0NOSUCHSTRATEGYAAAAAAAA")
        .json(&serde_json::json!({ "display_name": "Ghost" }))
        .await;
    response.assert_status_not_found();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn patch_metadata_refreshes_search_index_after_rename() {
    // PR #322 review (P2): the route previously called
    // FilesystemStore::update_metadata directly, bypassing
    // index_strategy_after_mutation. The command palette / search
    // would then return the OLD display_name until something else
    // touched the strategy. Engine API wrapper fixes that —
    // confirm an immediate /api/search hit reflects the new name.
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({ "display_name": "FreshlyRenamed" }))
        .await;
    response.assert_status_ok();

    let response = server
        .get("/api/search")
        .add_query_param("q", "FreshlyRenamed")
        .add_query_param("kind", "strategy")
        .await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let hits = body["hits"].as_array().expect("search returns hits array");
    assert!(
        hits.iter().any(|hit| hit["artifact_id"] == id),
        "renamed strategy must appear in search results for its new display_name; got: {hits:#?}",
    );
}

#[tokio::test]
async fn patch_metadata_combined_update_round_trips() {
    let (server, _tmp) = boot().await;
    let id = create_strategy(&server).await;

    let response = server
        .patch(&format!("/api/strategy/{id}"))
        .json(&serde_json::json!({
            "display_name": "Multi-Update",
            "plain_summary": "Updated summary text.",
            "asset_universe": ["BTC/USD", "ETH/USD"]
        }))
        .await;
    response.assert_status_ok();

    let response = server.get(&format!("/api/strategy/{id}")).await;
    let fetched: serde_json::Value = response.json();
    assert_eq!(fetched["manifest"]["display_name"], "Multi-Update");
    assert_eq!(fetched["manifest"]["plain_summary"], "Updated summary text.");
    let assets = fetched["manifest"]["asset_universe"].as_array().expect("array");
    assert_eq!(assets.len(), 2);
    assert_eq!(assets[0], "BTC/USD");
    assert_eq!(assets[1], "ETH/USD");
    assert_eq!(fetched["manifest"]["id"], id, "id stable after combined patch");
}

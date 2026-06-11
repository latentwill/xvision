//! Integration tests for marketplace bundle delivery:
//! `GET /api/marketplace/listings/:id/bundle` and
//! `POST /api/marketplace/listings/:id/import` — hand-built snapshots
//! injected into `AppState`, no chain. The ipfs:// resolution path is
//! exercised against a local in-process gateway (env `PINATA_GATEWAY`).

mod support;

use axum::http::StatusCode;
use axum::{routing::get, Router};
use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;
use xvision_dashboard::marketplace_index::{IndexedListing, MarketplaceSnapshot};
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::autooptimizer::content_hash::canonical_json;
use xvision_identity::manifest_hash_hex;

const ALICE: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn listing(listing_id: u64, content_uri: &str, content_hash: &str) -> IndexedListing {
    IndexedListing {
        listing_id,
        agent_nft_id: "7".to_string(),
        agent_id: "agent-7".to_string(),
        seller: ALICE.to_string(),
        content_hash: content_hash.to_string(),
        content_uri: content_uri.to_string(),
        tier: 0,
        price_usdc: 49.0,
        transferable_license: false,
        revoked: false,
        gen_art_seed: format!("agent-7:{content_hash}"),
        name: "xvn strategy 7".to_string(),
        symmetry: "Radial".into(),
        palette: "Ember".into(),
        attestation_count: 0,
        units_sold: 0,
        earned_usdc: 0.0,
    }
}

async fn boot() -> (TestServer, AppState, TempDir) {
    let (state, tmp) = support::state_with_tempdir().await;
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, state, tmp)
}

async fn inject_snapshot(state: &AppState, listings: Vec<IndexedListing>) {
    let mut snap = state.marketplace_snapshot.write().await;
    *snap = MarketplaceSnapshot {
        total_onchain: listings.len() as u64,
        listings,
        last_poll_unix: 1_700_000_000,
        last_error: None,
    };
}

/// Creates a strategy through the API and returns `(id, canonical, hash)` —
/// the exact bytes the publish route would pin and their content hash.
async fn seed_strategy(server: &TestServer, state: &AppState) -> (String, String, String) {
    let response = server
        .post("/api/strategies")
        .json(&serde_json::json!({ "name": "DeliverMe", "creator": "@seller" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let id = response.json::<Value>()["id"].as_str().unwrap().to_string();

    let strategy = xvision_engine::api::strategy::get(&state.api_context(), &id)
        .await
        .unwrap();
    let value = serde_json::to_value(&strategy).unwrap();
    let canonical = canonical_json(&value);
    let hash = manifest_hash_hex(&canonical);
    (id, canonical, hash)
}

// ── GET /api/marketplace/listings/:id/bundle ────────────────────────────────

#[tokio::test]
async fn bundle_unknown_listing_is_404() {
    let (server, _state, _tmp) = boot().await;
    let response = server.get("/api/marketplace/listings/99/bundle").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bundle_xvn_uri_verifies_and_returns_manifest() {
    let (server, state, _tmp) = boot().await;
    let (id, _canonical, hash) = seed_strategy(&server, &state).await;
    inject_snapshot(&state, vec![listing(1, &format!("xvn://strategy/{id}"), &hash)]).await;

    let response = server.get("/api/marketplace/listings/1/bundle").await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["listing_id"], 1);
    assert_eq!(body["verified"], true);
    assert_eq!(body["content_uri"], format!("xvn://strategy/{id}"));
    assert_eq!(body["manifest"]["manifest"]["id"], id);
}

#[tokio::test]
async fn bundle_hash_mismatch_is_409() {
    let (server, state, _tmp) = boot().await;
    let (id, _canonical, _hash) = seed_strategy(&server, &state).await;
    let wrong_hash = "ab".repeat(32);
    inject_snapshot(
        &state,
        vec![listing(1, &format!("xvn://strategy/{id}"), &wrong_hash)],
    )
    .await;

    let response = server.get("/api/marketplace/listings/1/bundle").await;
    response.assert_status(StatusCode::CONFLICT);
    let body: Value = response.json();
    assert_eq!(body["code"], "conflict");
    let msg = body["message"].as_str().unwrap();
    assert!(msg.contains("integrity"), "message names the failure: {msg}");
}

#[tokio::test]
async fn bundle_xvn_uri_missing_local_strategy_is_404() {
    let (server, state, _tmp) = boot().await;
    inject_snapshot(
        &state,
        vec![listing(
            1,
            "xvn://strategy/01TOTALLYMISSINGAGENTID000",
            &"cd".repeat(32),
        )],
    )
    .await;
    let response = server.get("/api/marketplace/listings/1/bundle").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bundle_ipfs_uri_fetches_from_gateway_and_verifies() {
    // Single test owns PINATA_GATEWAY (crate-wide env-mutation convention).
    let (server, state, _tmp) = boot().await;
    let (_id, canonical, hash) = seed_strategy(&server, &state).await;

    // In-process "gateway" serving the canonical bytes at /ipfs/:cid.
    let payload = canonical.clone();
    let gateway_app = Router::new().route(
        "/ipfs/bafytestcid",
        get(move || {
            let payload = payload.clone();
            async move { payload }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, gateway_app).await.unwrap();
    });

    std::env::set_var("PINATA_GATEWAY", format!("http://{addr}"));
    inject_snapshot(&state, vec![listing(1, "ipfs://bafytestcid", &hash)]).await;
    let response = server.get("/api/marketplace/listings/1/bundle").await;
    std::env::remove_var("PINATA_GATEWAY");

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["verified"], true);
    assert_eq!(body["content_uri"], "ipfs://bafytestcid");
    assert!(body["manifest"]["manifest"]["id"].is_string());
}

#[tokio::test]
async fn bundle_unsupported_scheme_is_503() {
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![listing(1, "ftp://nope", &"ab".repeat(32))]).await;
    let response = server.get("/api/marketplace/listings/1/bundle").await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
}

// ── POST /api/marketplace/listings/:id/import ───────────────────────────────

#[tokio::test]
async fn import_invalid_address_is_400() {
    let (server, _state, _tmp) = boot().await;
    let response = server
        .post("/api/marketplace/listings/1/import")
        .json(&serde_json::json!({ "address": "not-an-address" }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = response.json();
    assert_eq!(body["field"], "address");
}

#[tokio::test]
async fn import_unknown_listing_is_404() {
    let (server, _state, _tmp) = boot().await;
    let response = server
        .post("/api/marketplace/listings/99/import")
        .json(&serde_json::json!({ "address": ALICE }))
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn import_without_license_env_is_503() {
    // No XVN_LICENSE_TOKEN / chain env in the test environment → the
    // license gate degrades loudly rather than skipping the check.
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![listing(1, "xvn://strategy/x", &"ab".repeat(32))]).await;
    let response = server
        .post("/api/marketplace/listings/1/import")
        .json(&serde_json::json!({ "address": ALICE }))
        .await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
}

// ── POST /api/marketplace/listings/:id/update ───────────────────────────────

#[tokio::test]
async fn update_unknown_listing_is_404() {
    let (server, _state, _tmp) = boot().await;
    let response = server.post("/api/marketplace/listings/99/update").await;
    response.assert_status(StatusCode::NOT_FOUND);
    let body: Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn update_missing_local_strategy_is_404_named() {
    // The listing exists in the snapshot but its agent_id has no local
    // strategy on this host -> 404 with an explicit "local strategy" message
    // (distinct from the unknown-listing 404).
    let (server, state, _tmp) = boot().await;
    inject_snapshot(
        &state,
        vec![listing(1, "xvn://strategy/agent-7", &"ab".repeat(32))],
    )
    .await;
    let response = server.post("/api/marketplace/listings/1/update").await;
    response.assert_status(StatusCode::NOT_FOUND);
    let body: Value = response.json();
    let msg = body["message"].as_str().unwrap();
    assert!(
        msg.contains("local strategy"),
        "404 must name the missing local strategy: {msg}"
    );
}

#[tokio::test]
async fn update_without_chain_env_is_503() {
    // Removal-only (same contract as the revoke/buy 503 tests).
    std::env::remove_var("XVN_RPC_URL");
    std::env::remove_var("XVN_CHAIN_ID");
    std::env::remove_var("XVN_PUBLISHER_PK");

    let (server, state, _tmp) = boot().await;
    let (id, _canonical, hash) = seed_strategy(&server, &state).await;
    let mut l = listing(1, &format!("xvn://strategy/{id}"), &hash);
    l.agent_id = id.clone();
    inject_snapshot(&state, vec![l]).await;

    let response = server.post("/api/marketplace/listings/1/update").await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = response.json();
    assert_eq!(body["code"], "service_unavailable");
}

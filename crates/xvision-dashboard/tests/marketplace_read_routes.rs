//! Integration tests for the `/api/marketplace/*` read routes — hand-built
//! snapshots injected into `AppState`, no chain.

mod support;

use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;
use xvision_dashboard::marketplace_index::{IndexedListing, MarketplaceSnapshot};
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

const ALICE: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn listing(listing_id: u64, agent_nft_id: &str, seller: &str, revoked: bool) -> IndexedListing {
    IndexedListing {
        listing_id,
        agent_nft_id: agent_nft_id.to_string(),
        agent_id: format!("agent-{agent_nft_id}"),
        seller: seller.to_string(),
        content_hash: "ab".repeat(32),
        content_uri: format!("xvn://strategy/agent-{agent_nft_id}"),
        tier: 0,
        price_usdc: 49.0,
        transferable_license: false,
        revoked,
        gen_art_seed: format!("agent-{agent_nft_id}:{}", "ab".repeat(32)),
        name: format!("xvn strategy {agent_nft_id}"),
        symmetry: "Radial".into(),
        palette: "Ember".into(),
        attestation_count: 0,
        units_sold: 0,
        earned_usdc: 0.0,
        return30d_pct: None,
        sharpe: None,
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

// ── status ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_dormant_shape() {
    let (server, _state, _tmp) = boot().await;
    let response = server.get("/api/marketplace/status").await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["active"], false);
    assert_eq!(body["last_poll_unix"], 0);
    assert_eq!(body["total_onchain"], 0);
    assert!(body["last_error"].is_null());
}

#[tokio::test]
async fn status_lit_block_is_null_when_unconfigured() {
    let (server, _state, _tmp) = boot().await;
    let body: Value = server.get("/api/marketplace/status").await.json();
    assert!(
        body["lit"].is_null(),
        "lit block null when Lit unconfigured: {body}"
    );
}

#[tokio::test]
async fn status_lit_block_exposes_public_fields_never_api_key() {
    use xvision_dashboard::chain_config::{LitConfig, MarketplaceChainConfig};
    let cfg = MarketplaceChainConfig {
        chain: None,
        registry_addresses: None,
        marketplace_addresses: None,
        ipfs: None,
        indexer: None,
        license_token: None,
        lit: Some(LitConfig {
            api_base: "https://api.chipotle.litprotocol.com".into(),
            api_key: "super-secret-api-key".into(),
            pkp_id: "pkp-123".into(),
            gate_action_cid: "bafygatecid".into(),
            encrypt_action_cid: "bafyencryptcid".into(),
        }),
        chain_timeout: std::time::Duration::from_secs(45),
    };
    let (state, _tmp) = support::state_with_chain_config(cfg).await;
    let server = TestServer::new(build_router(state.clone())).unwrap();

    let resp = server.get("/api/marketplace/status").await;
    let raw = resp.text();
    let body: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(body["lit"]["api_base"], "https://api.chipotle.litprotocol.com");
    assert_eq!(body["lit"]["gate_action_cid"], "bafygatecid");
    assert_eq!(body["lit"]["pkp_id"], "pkp-123");
    // The API key must NEVER be exposed — not as a field, not anywhere.
    assert!(body["lit"].get("api_key").is_none(), "no api_key field: {body}");
    assert!(
        !raw.contains("super-secret-api-key"),
        "api key must not leak: {raw}"
    );
}

#[tokio::test]
async fn status_public_gateway_defaults_to_vendor_neutral() {
    // No pinata config → the status route must surface the vendor-neutral
    // default read gateway, never a vendor product (no pinata.cloud).
    let (server, _state, _tmp) = boot().await;
    let resp = server.get("/api/marketplace/status").await;
    let raw = resp.text();
    let body: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(body["public_gateway"], "https://dweb.link");
    assert!(!raw.contains("pinata"), "no vendor gateway baked in: {raw}");
    // The pinning API URL must NEVER appear on the status route.
    assert!(!raw.contains("api.pinata.cloud"), "no API url: {raw}");
}

#[tokio::test]
async fn status_public_gateway_reflects_configured_gateway() {
    use xvision_dashboard::chain_config::{IpfsBackend, MarketplaceChainConfig};
    use xvision_marketplace::PinataDriver;
    let cfg = MarketplaceChainConfig {
        chain: None,
        registry_addresses: None,
        marketplace_addresses: None,
        // The JWT is a pinning credential — it must never leak to the status
        // route, only the read gateway should surface.
        ipfs: Some(IpfsBackend::Pinata(PinataDriver::new(
            "super-secret-pin-jwt",
            "https://ipfs.mynode.example",
        ))),
        indexer: None,
        license_token: None,
        lit: None,
        chain_timeout: std::time::Duration::from_secs(45),
    };
    let (state, _tmp) = support::state_with_chain_config(cfg).await;
    let server = TestServer::new(build_router(state.clone())).unwrap();

    let resp = server.get("/api/marketplace/status").await;
    let raw = resp.text();
    let body: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(body["public_gateway"], "https://ipfs.mynode.example");
    // Never the pinning JWT or any API URL.
    assert!(!raw.contains("super-secret-pin-jwt"), "jwt must not leak: {raw}");
    assert!(!raw.contains("api.pinata.cloud"), "no API url: {raw}");
}

#[tokio::test]
async fn status_active_after_spawn_and_first_poll() {
    let (server, state, _tmp) = boot().await;
    state.mark_marketplace_indexer_active();

    // Spawned but no poll completed yet → still not active.
    let body: Value = server.get("/api/marketplace/status").await.json();
    assert_eq!(body["active"], false);

    inject_snapshot(&state, vec![listing(1, "7", ALICE, false)]).await;
    let body: Value = server.get("/api/marketplace/status").await.json();
    assert_eq!(body["active"], true);
    assert_eq!(body["last_poll_unix"], 1_700_000_000);
    assert_eq!(body["total_onchain"], 1);
}

#[tokio::test]
async fn status_contracts_object_reflects_env() {
    // Phase A: vars absent → all-null contracts object. No test in this
    // binary sets the two vars we probe afterwards, and the set/remove of
    // phase B is confined to this single test (the crate-wide env-mutation
    // convention), so the phases cannot race siblings.
    std::env::remove_var("XVN_MARKETPLACE_CONTRACT");
    std::env::remove_var("XVN_MARKETPLACE_USDC");

    let (server, _state, _tmp) = boot().await;
    let body: Value = server.get("/api/marketplace/status").await.json();
    assert!(body["contracts"].is_object(), "contracts object always present");
    assert!(body["contracts"]["marketplace"].is_null());
    assert!(body["contracts"]["usdc"].is_null());
    assert!(body["contracts"]["license_token"].is_null());
    assert!(body["contracts"]["listing_registry"].is_null());
    assert!(body["contracts"]["identity_registry"].is_null());

    // Phase B: set the two marketplace-only vars (NOT listing/identity
    // registry — those gate the indexer/receipts dormancy asserted elsewhere).
    let marketplace = "0x1111111111111111111111111111111111111111";
    let usdc = "0x2222222222222222222222222222222222222222";
    std::env::set_var("XVN_MARKETPLACE_CONTRACT", marketplace);
    std::env::set_var("XVN_MARKETPLACE_USDC", usdc);
    let body: Value = server.get("/api/marketplace/status").await.json();
    std::env::remove_var("XVN_MARKETPLACE_CONTRACT");
    std::env::remove_var("XVN_MARKETPLACE_USDC");

    assert_eq!(body["contracts"]["marketplace"], marketplace);
    assert_eq!(body["contracts"]["usdc"], usdc);
    assert!(body["contracts"]["license_token"].is_null());
}

// ── listings ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn listings_filter_revoked_by_default_include_with_flag() {
    let (server, state, _tmp) = boot().await;
    inject_snapshot(
        &state,
        vec![listing(1, "7", ALICE, false), listing(2, "8", ALICE, true)],
    )
    .await;

    let body: Value = server.get("/api/marketplace/listings").await.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["listing_id"], 1);
    assert_eq!(body["total"], 1);

    let body: Value = server
        .get("/api/marketplace/listings?include_revoked=1")
        .await
        .json();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["total"], 2);
}

// ── listing detail ──────────────────────────────────────────────────────────

#[tokio::test]
async fn listing_detail_found_and_404() {
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![listing(1, "7", ALICE, false)]).await;

    let response = server.get("/api/marketplace/listings/1").await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["listing_id"], 1);
    assert_eq!(body["agent_id"], "agent-7");
    // P4 trust fields are always present on the wire (zeros when the
    // attestation/marketplace contracts are unconfigured).
    assert_eq!(body["attestation_count"], 0);
    assert_eq!(body["units_sold"], 0);
    assert_eq!(body["earned_usdc"], 0.0);

    let response = server.get("/api/marketplace/listings/999").await;
    response.assert_status_not_found();
    let body: Value = response.json();
    assert_eq!(body["code"], "not_found");
}

// ── revoke ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn revoke_without_chain_env_is_503() {
    // Ensure chain env vars are absent so the route degrades loudly.
    std::env::remove_var("XVN_RPC_URL");
    std::env::remove_var("XVN_CHAIN_ID");
    std::env::remove_var("XVN_PUBLISHER_PK");

    let (server, _state, _tmp) = boot().await;
    let response = server.post("/api/marketplace/listings/5/revoke").await;
    response.assert_status_service_unavailable();
    let body: Value = response.json();
    assert_eq!(body["code"], "service_unavailable");
}

// ── buy ─────────────────────────────────────────────────────────────────────

/// A structurally valid buy body: recipient == authorization.from (M-2),
/// decimal-string value, 0x-64-hex nonce/r/s, v as a number.
fn buy_body(recipient: &str, from: &str, v: u64) -> Value {
    serde_json::json!({
        "listing_id": 1,
        "recipient": recipient,
        "authorization": {
            "from": from,
            "to": "0xcccccccccccccccccccccccccccccccccccccccc",
            "value": "49000000",
            "valid_after": 0,
            "valid_before": 1_893_456_000u64,
            "nonce": format!("0x{}", "ab".repeat(32)),
            "v": v,
            "r": format!("0x{}", "cd".repeat(32)),
            "s": format!("0x{}", "ef".repeat(32)),
        }
    })
}

#[tokio::test]
async fn buy_bad_recipient_address_is_400() {
    let (server, _state, _tmp) = boot().await;
    let response = server
        .post("/api/marketplace/buy")
        .json(&buy_body("not-an-address", ALICE, 27))
        .await;
    response.assert_status_bad_request();
    let body: Value = response.json();
    assert_eq!(body["code"], "validation");
    assert_eq!(body["field"], "recipient");
}

#[tokio::test]
async fn buy_v_out_of_range_is_400() {
    let (server, _state, _tmp) = boot().await;
    let response = server
        .post("/api/marketplace/buy")
        .json(&buy_body(ALICE, ALICE, 300))
        .await;
    response.assert_status_bad_request();
    let body: Value = response.json();
    assert_eq!(body["code"], "validation");
    assert_eq!(body["field"], "authorization.v");
}

#[tokio::test]
async fn buy_recipient_not_payer_is_400_m2() {
    let (server, _state, _tmp) = boot().await;
    let other = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let response = server
        .post("/api/marketplace/buy")
        .json(&buy_body(other, ALICE, 27))
        .await;
    response.assert_status_bad_request();
    let body: Value = response.json();
    assert_eq!(body["code"], "validation");
    assert!(
        body["message"].as_str().unwrap().contains("RecipientMustBePayer"),
        "message must name the contract revert: {body}"
    );
}

#[tokio::test]
async fn buy_without_chain_env_is_503() {
    // Removal-only (same contract as revoke_without_chain_env_is_503).
    std::env::remove_var("XVN_RPC_URL");
    std::env::remove_var("XVN_CHAIN_ID");
    std::env::remove_var("XVN_PUBLISHER_PK");

    let (server, _state, _tmp) = boot().await;
    let response = server
        .post("/api/marketplace/buy")
        .json(&buy_body(ALICE, ALICE, 27))
        .await;
    response.assert_status_service_unavailable();
    let body: Value = response.json();
    assert_eq!(body["code"], "service_unavailable");
}

// ── attest (write) ──────────────────────────────────────────────────────────

#[tokio::test]
async fn attest_unknown_listing_is_404() {
    let (server, _state, _tmp) = boot().await;
    let response = server
        .post("/api/marketplace/listings/99/attest")
        .json(&serde_json::json!({ "cycles": 20, "sharpe": 1.5 }))
        .await;
    response.assert_status_not_found();
    let body: Value = response.json();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn attest_without_chain_env_is_503() {
    // Removal-only (same contract as revoke_without_chain_env_is_503).
    std::env::remove_var("XVN_RPC_URL");
    std::env::remove_var("XVN_CHAIN_ID");
    std::env::remove_var("XVN_PUBLISHER_PK");

    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![listing(1, "7", ALICE, false)]).await;
    let response = server
        .post("/api/marketplace/listings/1/attest")
        .json(&serde_json::json!({ "cycles": 20, "sharpe": 1.5 }))
        .await;
    response.assert_status_service_unavailable();
    let body: Value = response.json();
    assert_eq!(body["code"], "service_unavailable");
}

// ── attestations (read) ─────────────────────────────────────────────────────

#[tokio::test]
async fn attestations_unknown_listing_is_404() {
    let (server, _state, _tmp) = boot().await;
    let response = server.get("/api/marketplace/listings/99/attestations").await;
    response.assert_status_not_found();
    let body: Value = response.json();
    assert_eq!(body["code"], "not_found");
}

/// Injected-config path (xvision-df3): the route reads the startup-resolved
/// `MarketplaceChainConfig` from `AppState`, not the env. An indexer config
/// WITHOUT an attestation registry yields the registry-specific 503 — no
/// network, no env mutation.
#[tokio::test]
async fn attestations_503_names_registry_with_injected_config_sans_attestation() {
    use xvision_dashboard::chain_config::MarketplaceChainConfig;
    use xvision_dashboard::marketplace_index::IndexerCfg;

    let cfg = MarketplaceChainConfig {
        chain: None,
        registry_addresses: None,
        marketplace_addresses: None,
        ipfs: None,
        indexer: Some(IndexerCfg {
            rpc_url: "http://127.0.0.1:9".into(),
            listing_registry: "0x1111111111111111111111111111111111111111".parse().unwrap(),
            identity_registry: "0x2222222222222222222222222222222222222222".parse().unwrap(),
            eval_attestation: None,
            marketplace: None,
            marketplace_deploy_block: None,
        }),
        license_token: None,
        lit: None,
        chain_timeout: std::time::Duration::from_secs(45),
    };
    let (state, _tmp) = support::state_with_chain_config(cfg).await;
    let server = TestServer::new(build_router(state.clone())).unwrap();
    inject_snapshot(&state, vec![listing(1, "7", ALICE, false)]).await;

    let response = server.get("/api/marketplace/listings/1/attestations").await;
    response.assert_status_service_unavailable();
    let body: Value = response.json();
    assert!(
        body["message"].as_str().unwrap().contains("XVN_EVAL_ATTESTATION"),
        "the injected indexer config must be honored (the 503 names the \
         missing attestation registry, not the chain env): {body}"
    );
}

#[tokio::test]
async fn attestations_503_when_chain_env_dormant() {
    // Removal-only: IndexerCfg::from_env requires XVN_RPC_URL (+ registries);
    // nothing in this binary ever sets XVN_RPC_URL or XVN_IDENTITY_REGISTRY.
    std::env::remove_var("XVN_RPC_URL");
    std::env::remove_var("XVN_IDENTITY_REGISTRY");

    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![listing(1, "7", ALICE, false)]).await;
    let response = server.get("/api/marketplace/listings/1/attestations").await;
    response.assert_status_service_unavailable();
    let body: Value = response.json();
    assert_eq!(body["code"], "service_unavailable");
}

// ── receipts ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn receipt_bad_tx_hash_is_400() {
    let (server, _state, _tmp) = boot().await;
    let response = server.get("/api/marketplace/receipts/0x1234").await;
    response.assert_status_bad_request();
    let body: Value = response.json();
    assert_eq!(body["code"], "validation");
    assert_eq!(body["field"], "tx_hash");
}

#[tokio::test]
async fn receipt_503_when_chain_env_dormant() {
    // Removal-only: IndexerCfg::from_env requires XVN_RPC_URL (+ registries);
    // nothing in this binary ever sets XVN_RPC_URL or XVN_IDENTITY_REGISTRY.
    std::env::remove_var("XVN_RPC_URL");
    std::env::remove_var("XVN_IDENTITY_REGISTRY");

    let (server, _state, _tmp) = boot().await;
    let tx = format!("0x{}", "ab".repeat(32));
    let response = server.get(&format!("/api/marketplace/receipts/{tx}")).await;
    response.assert_status_service_unavailable();
    let body: Value = response.json();
    assert_eq!(body["code"], "service_unavailable");
}

// ── wallet ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn wallet_bad_address_is_400() {
    let (server, state, _tmp) = boot().await;
    state.mark_marketplace_indexer_active();
    let response = server.get("/api/marketplace/wallet/not-an-address").await;
    response.assert_status_bad_request();
    let body: Value = response.json();
    assert_eq!(body["code"], "validation");
    assert_eq!(body["field"], "address");
}

#[tokio::test]
async fn wallet_503_when_indexer_dormant() {
    let (server, _state, _tmp) = boot().await;
    let response = server.get(&format!("/api/marketplace/wallet/{ALICE}")).await;
    response.assert_status_service_unavailable();
    let body: Value = response.json();
    assert_eq!(body["code"], "service_unavailable");
}

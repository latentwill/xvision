//! Integration tests for marketplace bundle delivery:
//! `GET /api/marketplace/listings/:id/bundle` and
//! `POST /api/marketplace/listings/:id/import` — hand-built snapshots
//! injected into `AppState`, no chain. The ipfs:// resolution path is
//! exercised against a local in-process gateway (env `PINATA_GATEWAY`).

mod support;

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
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
use xvision_marketplace::{MarketplaceError, SealedBundleCrypto};

const ALICE: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Deterministic fake crypto: records the last plaintext it was asked to
/// encrypt and returns `ENC(<plaintext>)`. Lets a route test prove the
/// sealed-publish encrypt path runs without a live Lit endpoint.
struct FakeCrypto {
    last_plaintext: Mutex<Option<String>>,
}

impl FakeCrypto {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            last_plaintext: Mutex::new(None),
        })
    }
}

#[async_trait]
impl SealedBundleCrypto for FakeCrypto {
    async fn encrypt(&self, plaintext: &[u8]) -> Result<String, MarketplaceError> {
        let s = String::from_utf8_lossy(plaintext).to_string();
        *self.last_plaintext.lock().unwrap() = Some(s.clone());
        Ok(format!("ENC({s})"))
    }
    fn gate_action_cid(&self) -> &str {
        "bafyfakegate"
    }
    fn is_configured(&self) -> bool {
        true
    }
}

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

/// Injected-config path (xvision-df3): a config with a license token but no
/// indexer sub-config gets past the license gate and 503s on the missing
/// chain access — proving the route reads `AppState` config, not env.
#[tokio::test]
async fn import_with_injected_license_token_but_no_indexer_is_503_chain_access() {
    use xvision_dashboard::chain_config::MarketplaceChainConfig;

    let cfg = MarketplaceChainConfig {
        chain: None,
        registry_addresses: None,
        marketplace_addresses: None,
        ipfs: None,
        indexer: None,
        license_token: Some("0x3333333333333333333333333333333333333333".parse().unwrap()),
        lit: None,
        chain_timeout: std::time::Duration::from_secs(45),
    };
    let (state, _tmp) = support::state_with_chain_config(cfg).await;
    let server = TestServer::new(build_router(state.clone())).unwrap();
    inject_snapshot(&state, vec![listing(1, "xvn://strategy/x", &"ab".repeat(32))]).await;

    let response = server
        .post("/api/marketplace/listings/1/import")
        .json(&serde_json::json!({ "address": ALICE }))
        .await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = response.json();
    assert!(
        body["message"].as_str().unwrap().contains("chain access"),
        "license gate must pass with the injected token; the 503 names the \
         missing chain access: {body}"
    );
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

// ── GET /api/marketplace/listings/:id/bundle (sealed tier) ───────────────────

fn sealed_listing(listing_id: u64, content_uri: &str, content_hash: &str) -> IndexedListing {
    let mut l = listing(listing_id, content_uri, content_hash);
    l.tier = 1;
    l
}

#[tokio::test]
async fn sealed_bundle_returns_ciphertext_not_manifest() {
    // Single test owns PINATA_GATEWAY (crate-wide env-mutation convention).
    let (server, state, _tmp) = boot().await;
    let hash = "ab".repeat(32);
    let ciphertext = "ENC(opaque-sealed-blob)".to_string();

    // In-process gateway serving the CIPHERTEXT (sealed blobs are pinned as-is).
    let payload = ciphertext.clone();
    let gateway_app = Router::new().route(
        "/ipfs/bafysealedcid",
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
    inject_snapshot(&state, vec![sealed_listing(1, "ipfs://bafysealedcid", &hash)]).await;
    let response = server.get("/api/marketplace/listings/1/bundle").await;
    std::env::remove_var("PINATA_GATEWAY");

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["encrypted"], true);
    assert_eq!(body["ciphertext"], ciphertext);
    assert_eq!(body["content_hash"], hash);
    assert_eq!(body["content_uri"], "ipfs://bafysealedcid");
    // The plaintext manifest must NOT appear in a sealed response.
    assert!(
        body.get("manifest").is_none(),
        "sealed bundle leaks no manifest: {body}"
    );
}

#[tokio::test]
async fn sealed_bundle_non_ipfs_uri_is_503() {
    let (server, state, _tmp) = boot().await;
    inject_snapshot(
        &state,
        vec![sealed_listing(1, "xvn://strategy/x", &"ab".repeat(32))],
    )
    .await;
    let response = server.get("/api/marketplace/listings/1/bundle").await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
}

// ── POST /api/marketplace/listings/:id/import-sealed ─────────────────────────

#[tokio::test]
async fn import_sealed_invalid_address_is_400() {
    let (server, _state, _tmp) = boot().await;
    let response = server
        .post("/api/marketplace/listings/1/import-sealed")
        .json(&serde_json::json!({ "address": "nope", "manifest": {} }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = response.json();
    assert_eq!(body["field"], "address");
}

#[tokio::test]
async fn import_sealed_unknown_listing_is_404() {
    let (server, _state, _tmp) = boot().await;
    let response = server
        .post("/api/marketplace/listings/99/import-sealed")
        .json(&serde_json::json!({ "address": ALICE, "manifest": {} }))
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn import_sealed_without_license_env_is_503() {
    // A VALID proof gets past the proof stage; the license gate then 503s
    // because no license-token env is configured. The proof must run first so
    // a legit buyer hits the gate (not a proof rejection).
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![sealed_listing(1, "ipfs://x", &"ab".repeat(32))]).await;
    let (nonce, expiry) = get_challenge(&server, 1).await;
    let buyer = buyer_signer();
    let message = challenge_message(1, &nonce, expiry);
    let signature = sign(&buyer, &message);
    let response = server
        .post("/api/marketplace/listings/1/import-sealed")
        .json(&serde_json::json!({
            "address": format!("{:#x}", buyer.address()),
            "manifest": {},
            "message": message,
            "signature": signature,
        }))
        .await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
}

/// With the license gate passed (injected token) but no indexer chain access,
/// the gate still 503s on the missing chain access — proving the sealed import
/// reuses the same gate as open import and reads `AppState` config, not env.
#[tokio::test]
async fn import_sealed_with_token_but_no_indexer_is_503() {
    use xvision_dashboard::chain_config::MarketplaceChainConfig;
    let cfg = MarketplaceChainConfig {
        chain: None,
        registry_addresses: None,
        marketplace_addresses: None,
        ipfs: None,
        indexer: None,
        license_token: Some("0x3333333333333333333333333333333333333333".parse().unwrap()),
        lit: None,
        chain_timeout: std::time::Duration::from_secs(45),
    };
    let (state, _tmp) = support::state_with_chain_config(cfg).await;
    let server = TestServer::new(build_router(state.clone())).unwrap();
    inject_snapshot(&state, vec![sealed_listing(1, "ipfs://x", &"ab".repeat(32))]).await;
    // Valid proof so the request reaches the license gate (which then 503s on
    // the missing indexer chain access).
    let (nonce, expiry) = get_challenge(&server, 1).await;
    let buyer = buyer_signer();
    let message = challenge_message(1, &nonce, expiry);
    let signature = sign(&buyer, &message);
    let response = server
        .post("/api/marketplace/listings/1/import-sealed")
        .json(&serde_json::json!({
            "address": format!("{:#x}", buyer.address()),
            "manifest": {},
            "message": message,
            "signature": signature,
        }))
        .await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = response.json();
    assert!(
        body["message"].as_str().unwrap().contains("chain access"),
        "{body}"
    );
}

// ── POST /api/marketplace/publish (sealed tier) ──────────────────────────────

/// A fully-populated chain config (dummy addresses) so the sealed-publish path
/// gets past the chain gate to the encrypt+pin step. The IPFS backend is a
/// Pinata driver with an EMPTY jwt so the pin fails fast (NotConfigured)
/// WITHOUT network — the encrypt has already run by then, which is what these
/// tests assert.
fn full_chain_config_empty_pinata() -> xvision_dashboard::chain_config::MarketplaceChainConfig {
    use alloy::signers::local::PrivateKeySigner;
    use xvision_dashboard::chain_config::{ChainSigner, IpfsBackend, MarketplaceChainConfig};
    use xvision_identity::{MarketplaceAddresses, RegistryAddresses};
    use xvision_marketplace::PinataDriver;

    // A well-known Anvil dev key — only used to satisfy ChainSigner; no chain
    // call is reached because the pin aborts first.
    let signer: PrivateKeySigner = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap();
    let addr = "0x1111111111111111111111111111111111111111".parse().unwrap();
    MarketplaceChainConfig {
        chain: Some(ChainSigner {
            rpc_url: "http://127.0.0.1:1".into(),
            chain_id: 5003,
            signer,
        }),
        registry_addresses: Some(RegistryAddresses::custom(addr, addr)),
        marketplace_addresses: Some(MarketplaceAddresses {
            xvn_deployer: addr,
            listing_registry: addr,
            marketplace: addr,
            license_token: addr,
            eval_attestation: addr,
            validation_registry: addr,
            usdc: addr,
            platform_agent_token_id: 0,
        }),
        ipfs: Some(IpfsBackend::Pinata(PinataDriver::new(
            String::new(),
            String::new(),
        ))),
        indexer: None,
        license_token: None,
        lit: None,
        chain_timeout: std::time::Duration::from_secs(45),
    }
}

#[tokio::test]
async fn publish_sealed_without_lit_is_400() {
    // Full chain config but NO sealed crypto override → NoopSealed (not
    // configured) → 400 before any encrypt/pin/mint.
    let cfg = full_chain_config_empty_pinata();
    let (state, _tmp) = support::state_with_chain_config(cfg).await;
    let server = TestServer::new(build_router(state.clone())).unwrap();
    let (id, _canonical, _hash) = seed_strategy(&server, &state).await;

    let response = server
        .post("/api/marketplace/publish")
        .json(&serde_json::json!({
            "strategy_id": id, "tier": "sealed", "price_usdc": 49.0
        }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = response.json();
    assert_eq!(body["field"], "tier");
    assert!(
        body["message"].as_str().unwrap().contains("Lit configuration"),
        "{body}"
    );
}

#[tokio::test]
async fn publish_sealed_encrypts_canonical_before_pin() {
    // Full chain config + an injected fake crypto + empty-jwt pinata. The
    // encrypt runs (recorded by the fake), then the pin fails fast → 503; the
    // assertion is that the fake saw the EXACT canonical plaintext bytes.
    let cfg = full_chain_config_empty_pinata();
    let (state, _tmp) = support::state_with_chain_config(cfg).await;
    let fake = FakeCrypto::new();
    let state = state.with_sealed_crypto(fake.clone());
    let server = TestServer::new(build_router(state.clone())).unwrap();
    let (id, canonical, _hash) = seed_strategy(&server, &state).await;

    let response = server
        .post("/api/marketplace/publish")
        .json(&serde_json::json!({
            "strategy_id": id, "tier": "sealed", "price_usdc": 49.0
        }))
        .await;

    // The pin fails fast (empty JWT) → 503, but the encrypt already ran.
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    let recorded = fake.last_plaintext.lock().unwrap().clone();
    assert_eq!(
        recorded.as_deref(),
        Some(canonical.as_str()),
        "sealed publish must encrypt the canonical plaintext before pinning"
    );
}

#[tokio::test]
async fn publish_open_does_not_encrypt() {
    // The open tier never touches the sealed crypto: a fake that PANICS on
    // encrypt proves the open path doesn't call it. Open with empty-jwt pinata
    // fails at the open pin → 503.
    struct PanicCrypto;
    #[async_trait]
    impl SealedBundleCrypto for PanicCrypto {
        async fn encrypt(&self, _: &[u8]) -> Result<String, MarketplaceError> {
            panic!("open-tier publish must never call sealed encrypt");
        }
        fn gate_action_cid(&self) -> &str {
            ""
        }
        fn is_configured(&self) -> bool {
            true
        }
    }

    let cfg = full_chain_config_empty_pinata();
    let (state, _tmp) = support::state_with_chain_config(cfg).await;
    let state = state.with_sealed_crypto(Arc::new(PanicCrypto));
    let server = TestServer::new(build_router(state.clone())).unwrap();
    let (id, _canonical, _hash) = seed_strategy(&server, &state).await;

    let response = server
        .post("/api/marketplace/publish")
        .json(&serde_json::json!({
            "strategy_id": id, "tier": "open", "price_usdc": 49.0
        }))
        .await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
}

// ── POST /api/marketplace/listings/:id/import-sealed (proof-of-address) ───────
//
// Lane cgz: the sealed-import route requires a real EIP-191 personal_sign
// proof-of-address. The proof check runs BEFORE the license gate, so a wrong
// signer is rejected with 403 (not the 503 the no-chain license gate would
// otherwise return) — these tests assert that ordering.

use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;

/// Well-known Anvil dev key #0 — its address is the claimed buyer address.
fn buyer_signer() -> PrivateKeySigner {
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap()
}

/// Sign `message` with `s`, returning the 0x-prefixed 65-byte hex signature
/// (viem/wagmi `signMessage` shape).
fn sign(s: &PrivateKeySigner, message: &str) -> String {
    let sig = s.sign_message_sync(message.as_bytes()).unwrap();
    format!("0x{}", alloy::hex::encode(sig.as_bytes()))
}

/// Build the exact challenge message the gate/server expect (byte-compatible
/// with `buildSealedMessage` in sealed.ts).
fn challenge_message(listing_id: u64, nonce: &str, expiry_unix: u64) -> String {
    format!(
        "xvision sealed-bundle license request\nListing: {listing_id}\nNonce: {nonce}\nExpiry: {expiry_unix}"
    )
}

/// GET the import-challenge for a listing and return `(nonce, expiry_unix)`.
async fn get_challenge(server: &TestServer, listing_id: u64) -> (String, u64) {
    let response = server
        .get(&format!(
            "/api/marketplace/listings/{listing_id}/import-challenge"
        ))
        .await;
    response.assert_status_ok();
    let body: Value = response.json();
    let nonce = body["nonce"].as_str().unwrap().to_string();
    let expiry = body["expiry_unix"].as_u64().unwrap();
    (nonce, expiry)
}

#[tokio::test]
async fn import_challenge_unknown_listing_is_404() {
    let (server, _state, _tmp) = boot().await;
    let response = server.get("/api/marketplace/listings/99/import-challenge").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn import_challenge_issues_nonce_and_message() {
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![sealed_listing(1, "ipfs://x", &"ab".repeat(32))]).await;
    let response = server.get("/api/marketplace/listings/1/import-challenge").await;
    response.assert_status_ok();
    let body: Value = response.json();
    let nonce = body["nonce"].as_str().unwrap();
    assert!(nonce.len() >= 8, "nonce must clear MIN_NONCE_LEN: {nonce}");
    assert!(body["expiry_unix"].as_u64().unwrap() > 1_700_000_000);
    // The returned message must be the exact byte string the client signs and
    // embed the listing + the issued nonce.
    let message = body["message"].as_str().unwrap();
    assert!(
        message.starts_with("xvision sealed-bundle license request"),
        "{message}"
    );
    assert!(message.contains("Listing: 1"), "{message}");
    assert!(message.contains(&format!("Nonce: {nonce}")), "{message}");
}

#[tokio::test]
async fn import_sealed_missing_signature_is_400() {
    // Body without `signature`/`message` must be rejected (serde requires them).
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![sealed_listing(1, "ipfs://x", &"ab".repeat(32))]).await;
    let response = server
        .post("/api/marketplace/listings/1/import-sealed")
        .json(&serde_json::json!({ "address": ALICE, "manifest": {} }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn import_sealed_wrong_signer_is_403() {
    // The proof gate runs BEFORE the license gate: a signature from a DIFFERENT
    // key than the claimed `address` must 403 at the proof stage — proving the
    // proof runs first (a passing-proof request would 503 at the no-chain
    // license gate instead).
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![sealed_listing(1, "ipfs://x", &"ab".repeat(32))]).await;
    let (nonce, expiry) = get_challenge(&server, 1).await;

    let buyer = buyer_signer();
    // A different key signs the message, but the body claims buyer's address.
    let attacker: PrivateKeySigner = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
        .parse()
        .unwrap();
    let message = challenge_message(1, &nonce, expiry);
    let signature = sign(&attacker, &message);

    let response = server
        .post("/api/marketplace/listings/1/import-sealed")
        .json(&serde_json::json!({
            "address": format!("{:#x}", buyer.address()),
            "manifest": {},
            "message": message,
            "signature": signature,
        }))
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
    let body: Value = response.json();
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("does not prove control"),
        "{body}"
    );
}

#[tokio::test]
async fn import_sealed_unknown_nonce_is_401() {
    // A never-issued nonce → 401 (the proof itself is valid, but the nonce was
    // never issued so it cannot be consumed).
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![sealed_listing(1, "ipfs://x", &"ab".repeat(32))]).await;

    let buyer = buyer_signer();
    let nonce = "deadbeefdeadbeefdeadbeefdeadbeef"; // never issued
    let expiry = 9_999_999_999u64;
    let message = challenge_message(1, nonce, expiry);
    let signature = sign(&buyer, &message);

    let response = server
        .post("/api/marketplace/listings/1/import-sealed")
        .json(&serde_json::json!({
            "address": format!("{:#x}", buyer.address()),
            "manifest": {},
            "message": message,
            "signature": signature,
        }))
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn import_sealed_replayed_nonce_is_401() {
    // Issue one nonce, use it once (it reaches the proof + nonce-consume stage,
    // then 503s at the no-chain license gate), then reuse the SAME nonce +
    // signature → 401 (the nonce was already consumed). Proves single-use.
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![sealed_listing(1, "ipfs://x", &"ab".repeat(32))]).await;
    let (nonce, expiry) = get_challenge(&server, 1).await;

    let buyer = buyer_signer();
    let message = challenge_message(1, &nonce, expiry);
    let signature = sign(&buyer, &message);
    let body = serde_json::json!({
        "address": format!("{:#x}", buyer.address()),
        "manifest": {},
        "message": message,
        "signature": signature,
    });

    // First use: passes the proof + consumes the nonce, then 503 at the
    // no-chain license gate (proof ran, nonce is now spent).
    let first = server
        .post("/api/marketplace/listings/1/import-sealed")
        .json(&body)
        .await;
    first.assert_status(StatusCode::SERVICE_UNAVAILABLE);

    // Replay the identical request → 401, because the nonce is consumed.
    let replay = server
        .post("/api/marketplace/listings/1/import-sealed")
        .json(&body)
        .await;
    replay.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn import_sealed_valid_proof_passes_to_license_gate() {
    // A valid signature from the claimed address over a fresh issued nonce gets
    // PAST the proof stage and reaches the license gate, which 503s in the
    // no-chain test env. The point: a good proof fails LATER (503) than a bad
    // proof (403/401) — proving the signature stage accepts a correct proof.
    let (server, state, _tmp) = boot().await;
    inject_snapshot(&state, vec![sealed_listing(1, "ipfs://x", &"ab".repeat(32))]).await;
    let (nonce, expiry) = get_challenge(&server, 1).await;

    let buyer = buyer_signer();
    let message = challenge_message(1, &nonce, expiry);
    let signature = sign(&buyer, &message);

    let response = server
        .post("/api/marketplace/listings/1/import-sealed")
        .json(&serde_json::json!({
            "address": format!("{:#x}", buyer.address()),
            "manifest": {},
            "message": message,
            "signature": signature,
        }))
        .await;
    // 503 = passed the proof, stopped at the no-chain license gate.
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
}

//! Integration tests for publish idempotency (bead xvision-4dn).
//!
//! `POST /api/marketplace/publish` must NOT mint a duplicate NFT + listing on
//! re-publish of an already-published `agent_id` (the strategy ULID, which
//! becomes the NFT token id). A publish-receipt store keyed by `agent_id`
//! short-circuits a re-publish with 409 Conflict BEFORE any chain or IPFS work.
//!
//! The 409 short-circuit is fully integration-testable: pre-seed a receipt row,
//! POST publish with that strategy_id, and assert 409 — which is distinguishable
//! from the 503 the un-short-circuited publish returns (chain gate / pin abort).
//! That 409-vs-503 distinction is the proof the lookup fires before any chain or
//! IPFS side effect. The happy-path 201 cannot be asserted end-to-end (the mint
//! hits a real RPC and there is no Anvil harness); the insert/lookup helpers are
//! covered by direct pool unit tests in `publish_receipts.rs`.

mod support;

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::Value;
use xvision_dashboard::routes::publish_receipts::insert_receipt;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

/// Seed a strategy via the real create route and return its id (the agent_id).
async fn seed_strategy(server: &TestServer) -> String {
    let response = server
        .post("/api/strategies")
        .json(&serde_json::json!({ "name": "PublishMe", "creator": "@seller" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json::<Value>()["id"].as_str().unwrap().to_string()
}

/// A fully-populated chain config (dummy addresses) so an OPEN publish that is
/// NOT short-circuited gets past the chain gate to the pin step, where the
/// empty-jwt Pinata backend aborts with 503 WITHOUT a network call. The 503
/// from this path is the negative control that proves the 409 short-circuit
/// genuinely precedes the chain/IPFS work.
fn full_chain_config_empty_pinata() -> xvision_dashboard::chain_config::MarketplaceChainConfig {
    use alloy::signers::local::PrivateKeySigner;
    use xvision_dashboard::chain_config::{ChainSigner, IpfsBackend, MarketplaceChainConfig};
    use xvision_identity::{MarketplaceAddresses, RegistryAddresses};
    use xvision_marketplace::PinataDriver;

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

/// Build a state with the empty-pinata chain config AND the dashboard
/// migrations (which create the `publish_receipts` table). `state_with_chain_config`
/// deliberately does NOT run the dashboard migrations, so the test must call it.
async fn boot_with_chain() -> (TestServer, AppState, tempfile::TempDir) {
    let cfg = full_chain_config_empty_pinata();
    let (state, tmp) = support::state_with_chain_config(cfg).await;
    state
        .run_dashboard_migrations()
        .await
        .expect("dashboard migrations create publish_receipts");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, state, tmp)
}

/// Pre-seeding a receipt for an agent_id makes a re-publish 409 Conflict BEFORE
/// any chain or IPFS side effect. If the short-circuit did NOT fire, this open
/// publish would instead reach the pin step and return 503 — so 409 (not 503)
/// is the proof the receipt lookup precedes the chain/IPFS work.
#[tokio::test]
async fn republish_existing_agent_id_is_409_no_chain() {
    let (server, state, _tmp) = boot_with_chain().await;
    let agent_id = seed_strategy(&server).await;

    // Manually seed a receipt as if a prior publish minted token_id=42, listing=7.
    insert_receipt(
        &state.pool,
        &agent_id,
        "42",
        "7",
        "ab".repeat(32).as_str(),
        "2026-06-13T00:00:00Z",
        None,
    )
    .await
    .expect("seed receipt");

    let response = server
        .post("/api/marketplace/publish")
        .json(&serde_json::json!({
            "strategy_id": agent_id, "tier": "open", "price_usdc": 49.0
        }))
        .await;

    // 409, NOT 503: the receipt lookup short-circuited before the chain gate.
    response.assert_status(StatusCode::CONFLICT);
    let body: Value = response.json();
    assert_eq!(body["code"], "conflict", "body: {body}");
    let msg = body["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("42"),
        "the 409 message must name the existing token_id: {msg}"
    );
}

/// Negative control: with NO pre-seeded receipt, the same open publish falls
/// through the lookup unchanged and reaches the pin step, returning 503 (the
/// empty-jwt Pinata abort). This guards against the lookup accidentally
/// swallowing the normal path.
#[tokio::test]
async fn publish_without_receipt_still_reaches_pin_503() {
    let (server, _state, _tmp) = boot_with_chain().await;
    let agent_id = seed_strategy(&server).await;

    let response = server
        .post("/api/marketplace/publish")
        .json(&serde_json::json!({
            "strategy_id": agent_id, "tier": "open", "price_usdc": 49.0
        }))
        .await;

    // No receipt → lookup miss → falls through to the pin abort (503).
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = response.json();
    assert_eq!(body["code"], "service_unavailable", "body: {body}");
}

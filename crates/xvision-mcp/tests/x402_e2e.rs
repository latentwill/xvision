//! End-to-end x402 purchase smoke test (non-custodial, first-party client).
//!
//! `#[ignore]` by default — it drives a REAL running dashboard over HTTP and
//! settles a REAL on-chain transaction, so it needs live creds + infra and must
//! never run in CI.
//!
//! ## How to run (Mantle Sepolia, before mainnet)
//! 1. Start the dashboard configured for testnet (chain relay env set):
//!      XVN_RPC_URL=<mantle-sepolia-rpc> XVN_CHAIN_ID=5003 XVN_PUBLISHER_PK=<gas-relayer>
//!      XVN_LISTING_REGISTRY=… XVN_MARKETPLACE_CONTRACT=… XVN_MARKETPLACE_USDC=…
//!      XVN_IDENTITY_REGISTRY=…   (so the indexer + chain routes are live)
//! 2. Fund a buyer wallet with test USDC (the MockUSDC3009 `faucet`) and export
//!      its key locally as the agent's own key (NEVER sent to the platform):
//!      XVN_AGENT_PK=0x<buyer-key>
//! 3. Point the client at the running dashboard + pick a real listing id:
//!      XVN_MARKETPLACE_API=http://127.0.0.1:8080  X402_TEST_LISTING_ID=<id>
//! 4. Run:
//!      cargo test -p xvision-mcp --test x402_e2e -- --ignored --nocapture
//!
//! Asserts the full non-custodial handshake: GET 402 → sign EIP-3009 locally →
//! POST settle → real tx_hash + license_token_id → import installs the strategy.

use xvision_mcp::marketplace_client;

fn test_listing_id() -> u64 {
    std::env::var("X402_TEST_LISTING_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .expect("set X402_TEST_LISTING_ID to a real listing on the running dashboard")
}

#[tokio::test]
#[ignore = "needs a running testnet dashboard + funded XVN_AGENT_PK; run with --ignored"]
async fn x402_buy_then_import_round_trip() {
    let id = test_listing_id();

    // 1. Browse must return the listing set (read path is live).
    let listings = marketplace_client::browse()
        .await
        .expect("browse the running dashboard");
    assert!(
        listings.is_array() || listings.is_object(),
        "unexpected browse shape: {listings}"
    );

    // 2. Buy over x402: GET 402 → sign locally → POST settle.
    let receipt = marketplace_client::buy(id)
        .await
        .expect("x402 buy should settle on-chain");
    let tx = receipt
        .get("tx_hash")
        .and_then(|v| v.as_str())
        .expect("settle returns a tx_hash");
    assert!(tx.starts_with("0x") && tx.len() == 66, "tx_hash shape: {tx}");
    assert!(
        receipt.get("license_token_id").is_some(),
        "settle returns a license_token_id: {receipt}"
    );

    // 3. Import: on-chain license check installs the strategy locally.
    let imported = marketplace_client::import(id)
        .await
        .expect("import after a confirmed purchase");
    assert!(
        imported.is_object() || imported.is_array(),
        "unexpected import shape: {imported}"
    );
}

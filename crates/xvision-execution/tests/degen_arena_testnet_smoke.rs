//! Live Hyperliquid **testnet** smoke for the Degen Arena venue.
//!
//! `#[ignore]` by default — it talks to the real testnet and needs creds in env.
//! Run explicitly:
//!
//! ```bash
//! DEGEN_HL_API_KEY=0x... DEGEN_HL_ACCOUNT_ADDRESS=0x... DEGEN_HL_NETWORK=testnet \
//!   scripts/cargo test -p xvision-execution --test degen_arena_testnet_smoke -- --ignored --nocapture
//! ```
//!
//! What it proves:
//! - **Reads** (`balance`, `position`) hit `/info` by address — no signing —
//!   confirming the live reqwest read path + JSON parsing.
//! - **One signed order** hits `/exchange`, exercising native EIP-712 signing.
//!   On an unfunded account this is rejected for *margin* — which still confirms
//!   the **signature was accepted** (a bad signature returns a different,
//!   signature/identity error). On a funded account it fills.
//!
//! The private key is read from the process env and never printed.

use xvision_execution::broker_surface::{BrokerSurface, OrderRequest, Side};
use xvision_execution::DegenArenaSurface;

#[tokio::test]
#[ignore = "live testnet; needs DEGEN_HL_* env (order leg needs a funded wallet)"]
async fn degen_arena_testnet_smoke() {
    let key = std::env::var("DEGEN_HL_API_KEY").unwrap_or_default();
    let addr = std::env::var("DEGEN_HL_ACCOUNT_ADDRESS").unwrap_or_default();
    let network = std::env::var("DEGEN_HL_NETWORK").unwrap_or_else(|_| "testnet".into());

    if key.trim().is_empty() || addr.trim().is_empty() {
        eprintln!("SKIP: set DEGEN_HL_API_KEY and DEGEN_HL_ACCOUNT_ADDRESS to run the live smoke");
        return;
    }

    let surface = DegenArenaSurface::from_credentials(&key, &addr, &network)
        .expect("build DegenArenaSurface from credentials");

    println!("── Degen Arena testnet smoke ({network}) ──────────────────────");

    // 1. Read-only — no signing. Proves the live reqwest read path + parsing.
    match surface.balance().await {
        Ok(b) => println!("[read] account_value = {b} USD"),
        Err(e) => println!("[read] balance error: {e:#}"),
    }
    match surface.position("BTC").await {
        Ok(p) => println!("[read] BTC position    = {p}"),
        Err(e) => println!("[read] position error: {e:#}"),
    }

    // 2. Open a tiny BTC long (~$13 notional, aggressive IOC). Proves native
    //    EIP-712 signing live + a real fill on the (agent-authorized) account.
    let open = OrderRequest {
        asset: "BTC".into(),
        side: Side::Buy,
        size: 0.0002,
        reference_price_usd: 64_700.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "degen-testnet-smoke-open".into(),
    };
    match surface.submit_order(open).await {
        Ok(conf) => println!("[open ] FILLED: {conf:?}"),
        Err(e) => println!("[open ] rejected: {e:#}"),
    }
    match surface.position("BTC").await {
        Ok(p) => println!("[read ] BTC position after open = {p}"),
        Err(e) => println!("[read ] position error: {e:#}"),
    }

    // 3. Close it flat — opposing IOC sell of the same size.
    let close = OrderRequest {
        asset: "BTC".into(),
        side: Side::Sell,
        size: 0.0002,
        reference_price_usd: 64_700.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "degen-testnet-smoke-close".into(),
    };
    match surface.submit_order(close).await {
        Ok(conf) => println!("[close] FILLED: {conf:?}"),
        Err(e) => println!("[close] rejected: {e:#}"),
    }
    match surface.position("BTC").await {
        Ok(p) => println!("[read ] BTC position after close = {p}"),
        Err(e) => println!("[read ] position error: {e:#}"),
    }
    println!("───────────────────────────────────────────────────────────────");
}

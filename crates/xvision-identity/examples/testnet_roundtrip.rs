//! T1.2 on-chain round-trip sanity against Mantle Sepolia (chain 5003).
//!
//! Registers a NEW test agent on the deployed ERC-8004 IdentityRegistry,
//! posts one giveFeedback entry to the ReputationRegistry, then reads the
//! full reputation log back to verify decode.
//!
//! Env vars:
//! - `MANTLE_TESTNET_IDENTITY_REGISTRY`   (required) — IdentityRegistry address
//! - `MANTLE_TESTNET_REPUTATION_REGISTRY` (required) — ReputationRegistry address
//! - `PRIVATE_KEY`                        (required) — signer hex key (never printed)
//! - `XVN_RPC_URL`        (default `https://rpc.sepolia.mantle.xyz`)
//! - `XVN_CHAIN_ID`       (default `5003`)
//! - `XVN_TOKEN_ID`       (optional) — skip registration, post feedback to this token
//!
//! Usage:
//! ```sh
//! PRIVATE_KEY=$(op read "op://Olympus/XVN Wallet/private key") \
//!   cargo run -p xvision-identity --example testnet_roundtrip
//! # second feedback run against an existing token:
//! XVN_TOKEN_ID=<id> PRIVATE_KEY=... cargo run -p xvision-identity --example testnet_roundtrip
//! ```

use alloy::signers::local::PrivateKeySigner;
use chrono::Utc;
use uuid::Uuid;
use xvision_identity::{IdentityClient, RegistryAddresses, TokenId, TradeOutcome};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rpc_url = std::env::var("XVN_RPC_URL").unwrap_or_else(|_| "https://rpc.sepolia.mantle.xyz".into());
    let chain_id: u64 = std::env::var("XVN_CHAIN_ID")
        .unwrap_or_else(|_| "5003".into())
        .parse()?;

    let addresses = RegistryAddresses::mantle_testnet()
        .ok_or("MANTLE_TESTNET_IDENTITY_REGISTRY / MANTLE_TESTNET_REPUTATION_REGISTRY must be set")?;

    let signer: PrivateKeySigner = std::env::var("PRIVATE_KEY")
        .map_err(|_| "PRIVATE_KEY env var required")?
        .parse()
        .map_err(|_| "PRIVATE_KEY is not a valid hex private key")?;
    println!("signer address: {}", signer.address());

    let client = IdentityClient::connect(&rpc_url, addresses, chain_id).await?;
    println!("connected: chain_id={}", client.chain_id());

    // Step 1: register a new test agent (unless XVN_TOKEN_ID re-uses one).
    let token_id = match std::env::var("XVN_TOKEN_ID") {
        Ok(t) => {
            let id = TokenId::from_u64(t.parse()?);
            println!("reusing existing token id: {id}");
            id
        }
        Err(_) => {
            let uri: url::Url =
                format!("https://xvision.example/t1.2-roundtrip/{}.json", Uuid::new_v4()).parse()?;
            println!("registering new agent: agentURI={uri}");
            let id = client.register(&uri, &signer).await?;
            println!("MINTED token_id={id}");
            id
        }
    };

    // Step 2: post one reputation entry.
    let cycle_id = Uuid::new_v4();
    let outcome = TradeOutcome {
        cycle_id,
        realized_pnl_usd: 12.34,
        action: "close".to_string(),
        closed_at: Utc::now(),
    };
    let tx = client
        .post_reputation(token_id.clone(), cycle_id, outcome, &signer)
        .await?;
    println!("FEEDBACK tx_hash={tx} cycle_id={cycle_id}");

    // Step 3: read the full reputation log back (decode sanity).
    let entries = client.read_reputation(token_id.clone()).await?;
    println!("READBACK count={}", entries.len());
    for (i, e) in entries.iter().enumerate() {
        println!(
            "  [{i}] cycle_id={} pnl={} action={} anchor={}",
            e.cycle_id, e.outcome.realized_pnl_usd, e.outcome.action, e.tx_hash
        );
    }

    println!("ROUNDTRIP OK token_id={token_id}");
    Ok(())
}

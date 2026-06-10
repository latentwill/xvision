//! `xvn marketplace ...` — marketplace listing / buy / attest verbs.

use std::path::PathBuf;

use alloy::primitives::{keccak256, Address, B256, U256};
use alloy::signers::local::PrivateKeySigner;
use clap::{Args, Subcommand};
use xvision_marketplace::{
    AnchorDriver, AttestRequest, BuyRequest, Erc8004MantleDriver, MarketplaceAddresses, MockDriver,
    PublishRequest,
};

use crate::exit::{CliError, CliResult};

#[derive(Args, Debug)]
pub struct MarketplaceCmd {
    #[command(subcommand)]
    action: MarketplaceAction,
}

#[derive(Subcommand, Debug)]
enum MarketplaceAction {
    /// List marketplace listings from the local fixture file.
    ///
    /// Reads `XVN_MARKETPLACE_FIXTURE` or `$XVN_HOME/marketplace/listings.json`.
    /// Prints: agent_id | version | price_usdc | seller | status
    List,
    /// Publish a strategy listing to the marketplace.
    Publish {
        /// Agent id (ULID) of the strategy variant.
        #[arg(long)]
        agent_id: String,
        /// Listing price in USDC (e.g. `10.0`).
        #[arg(long)]
        price: f64,
        /// Path to the manifest JSON file.
        #[arg(long)]
        manifest_path: PathBuf,
    },
    /// Buy a marketplace listing.
    Buy {
        /// Listing id (numeric) returned from `publish`.
        #[arg(long)]
        listing_id: u64,
        /// Buyer wallet address (hex, e.g. `0x0000...`).
        #[arg(long)]
        buyer: String,
    },
    /// Post an eval attestation for a listing.
    Attest {
        /// Listing id (numeric) returned from `publish`.
        #[arg(long)]
        listing_id: u64,
        /// Number of eval cycles run.
        #[arg(long)]
        cycles: u64,
        /// Sharpe ratio of the eval result.
        #[arg(long)]
        sharpe: f64,
    },
}

pub async fn run(cmd: MarketplaceCmd) -> CliResult<()> {
    match cmd.action {
        MarketplaceAction::List => list().await,
        MarketplaceAction::Publish {
            agent_id,
            price,
            manifest_path,
        } => publish(&agent_id, price, &manifest_path).await,
        MarketplaceAction::Buy { listing_id, buyer } => buy(listing_id, &buyer).await,
        MarketplaceAction::Attest {
            listing_id,
            cycles,
            sharpe,
        } => attest(listing_id, cycles, sharpe).await,
    }
}

fn fixture_path() -> CliResult<PathBuf> {
    if let Ok(p) = std::env::var("XVN_MARKETPLACE_FIXTURE") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let home = crate::commands::home::resolve_xvn_home_env()
        .map_err(|e| CliError::usage(anyhow::anyhow!("resolve XVN_HOME: {e}")))?;
    Ok(home.join("marketplace").join("listings.json"))
}

fn driver() -> CliResult<Box<dyn AnchorDriver>> {
    if std::env::var("MARKETPLACE_DRIVER").as_deref() == Ok("onchain") {
        let key_hex = std::env::var("MANTLE_PRIVATE_KEY").map_err(|_| {
            CliError::usage(anyhow::anyhow!(
                "MARKETPLACE_DRIVER=onchain requires MANTLE_PRIVATE_KEY to be set"
            ))
        })?;
        let addresses = MarketplaceAddresses::from_env().ok_or_else(|| {
            CliError::usage(anyhow::anyhow!(
                "MARKETPLACE_DRIVER=onchain requires XVN_LISTING_REGISTRY (hex contract address)"
            ))
        })?;
        let rpc_url = std::env::var("XVN_MANTLE_RPC_URL")
            .unwrap_or_else(|_| "https://rpc.sepolia.mantle.xyz".to_string());
        let chain_id: u64 = std::env::var("XVN_MANTLE_CHAIN_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5003);
        let signer: PrivateKeySigner = key_hex.trim_start_matches("0x").parse().map_err(|e| {
            CliError::usage(anyhow::anyhow!(
                "MANTLE_PRIVATE_KEY is not a valid hex private key: {e}"
            ))
        })?;
        return Ok(Box::new(Erc8004MantleDriver::with_signer(
            addresses, rpc_url, chain_id, signer,
        )));
    }
    Ok(Box::new(MockDriver::new()))
}

async fn list() -> CliResult<()> {
    let path = fixture_path()?;
    if !path.exists() {
        println!("(no listings)");
        return Ok(());
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("read {}: {e}", path.display())))?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&raw)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("parse listings JSON: {e}")))?;
    if rows.is_empty() {
        println!("(no listings)");
        return Ok(());
    }
    println!(
        "{:<24} {:>7} {:>12} {:<42} {}",
        "agent_id", "version", "price_usdc", "seller", "status"
    );
    let limit = rows.len().min(200);
    for row in rows.iter().take(limit) {
        let agent_id = row["agent_id"].as_str().unwrap_or("-");
        let version = row["version"].as_str().unwrap_or("-");
        let price = row["price_usdc"].as_str().unwrap_or("-");
        let seller = row["seller"].as_str().unwrap_or("-");
        let status = row["status"].as_str().unwrap_or("-");
        println!(
            "{:<24} {:>7} {:>12} {:<42} {}",
            agent_id, version, price, seller, status
        );
    }
    Ok(())
}

fn price_to_u256(price_usdc: f64) -> CliResult<U256> {
    if !price_usdc.is_finite() || price_usdc < 0.0 {
        return Err(CliError::usage(anyhow::anyhow!(
            "--price must be a non-negative finite number"
        )));
    }
    let micro = (price_usdc * 1_000_000.0).round() as u64;
    Ok(U256::from(micro))
}

fn parse_address(buyer: &str) -> CliResult<Address> {
    buyer
        .parse::<Address>()
        .map_err(|e| CliError::usage(anyhow::anyhow!("invalid --buyer address: {e}")))
}

async fn publish(agent_id: &str, price: f64, manifest_path: &PathBuf) -> CliResult<()> {
    let manifest_raw = std::fs::read_to_string(manifest_path)
        .map_err(|e| CliError::usage(anyhow::anyhow!("read {}: {e}", manifest_path.display())))?;
    let _manifest: serde_json::Value = serde_json::from_str(&manifest_raw)
        .map_err(|e| CliError::usage(anyhow::anyhow!("parse manifest JSON: {e}")))?;

    let content_hash: B256 = keccak256(manifest_raw.as_bytes());
    let price_usdc = price_to_u256(price)?;
    let req = PublishRequest {
        agent_nft_id: U256::ZERO,
        content_hash,
        content_uri: format!("file://{}", manifest_path.display()),
        tier: 0,
        price_usdc,
        transferable_license: false,
    };

    let d = driver()?;
    let listing_ref = d
        .publish_listing(req)
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("publish_listing: {e}")))?;

    let listing_id: u64 = u64::try_from(listing_ref.listing_id).unwrap_or(u64::MAX);
    println!("listing_id={listing_id} agent_id={agent_id}");
    Ok(())
}

async fn buy(listing_id: u64, buyer: &str) -> CliResult<()> {
    let recipient = parse_address(buyer)?;
    let req = BuyRequest {
        listing_id: U256::from(listing_id),
        recipient,
        authorization: None,
    };

    let d = driver()?;
    let receipt = d.buy_listing(req).await.map_err(|e| match e {
        xvision_marketplace::MarketplaceError::UnknownListing(id) => {
            CliError::not_found(anyhow::anyhow!("listing {id} not found"))
        }
        xvision_marketplace::MarketplaceError::ListingRevoked(id) => {
            CliError::usage(anyhow::anyhow!("listing {id} was revoked"))
        }
        other => CliError::upstream(anyhow::anyhow!("buy_listing: {other}")),
    })?;

    let token_id: u64 = u64::try_from(receipt.license_token_id).unwrap_or(u64::MAX);
    println!("tx_hash={:#x} license_token_id={token_id}", receipt.tx_hash);
    Ok(())
}

async fn attest(listing_id: u64, cycles: u64, sharpe: f64) -> CliResult<()> {
    if !sharpe.is_finite() {
        return Err(CliError::usage(anyhow::anyhow!(
            "--sharpe must be a finite number"
        )));
    }
    let payload = serde_json::json!({ "cycles": cycles, "sharpe": sharpe });
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|e| CliError::upstream(anyhow::anyhow!("serialize attest payload: {e}")))?;
    let eval_result_hash: B256 = keccak256(&payload_bytes);

    let req = AttestRequest {
        listing_id: U256::from(listing_id),
        eval_result_hash,
        eval_result_uri: format!("xvn://eval/listing/{listing_id}"),
        schema: B256::ZERO,
    };

    let d = driver()?;
    let tx_hash = d.attest_eval(req).await.map_err(|e| match e {
        xvision_marketplace::MarketplaceError::UnknownListing(id) => {
            CliError::not_found(anyhow::anyhow!("listing {id} not found"))
        }
        other => CliError::upstream(anyhow::anyhow!("attest_eval: {other}")),
    })?;

    println!("tx_hash={:#x}", tx_hash);
    Ok(())
}

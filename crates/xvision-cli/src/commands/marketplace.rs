//! `xvn marketplace ...` — marketplace listing / buy / attest verbs.

use std::path::PathBuf;

use alloy::primitives::{keccak256, Address, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use clap::{Args, Subcommand};
use xvision_identity::client::IIdentityRegistry;
use xvision_identity::contracts::IListingRegistry;
use xvision_identity::token_metadata::{decode_svg_image, decode_token_metadata};
use xvision_marketplace::{
    AnchorDriver, AttestRequest, BuyRequest, Erc8004MantleDriver, MarketplaceAddresses, MockDriver,
    PublishRequest,
};

use crate::exit::{CliError, CliResult};

/// Driver selection (env): by default all verbs run against the in-memory
/// mock driver. Set `MARKETPLACE_DRIVER=onchain` to write to the deployed
/// Mantle contracts instead (see `--help` for the full env contract).
#[derive(Args, Debug)]
#[command(after_help = ONCHAIN_ENV_HELP)]
pub struct MarketplaceCmd {
    #[command(subcommand)]
    action: MarketplaceAction,
}

/// Env contract for `MARKETPLACE_DRIVER=onchain`, shown in `--help`.
const ONCHAIN_ENV_HELP: &str = "\
Onchain driver (MARKETPLACE_DRIVER=onchain):
  MANTLE_PRIVATE_KEY     signer key (hex). Retrieve from 1Password, never paste:
                           MANTLE_PRIVATE_KEY=$(op read \"op://Olympus/XVN Wallet/private key\")
  XVN_LISTING_REGISTRY   ListingRegistry proxy address (required)
  XVN_IDENTITY_REGISTRY  IdentityRegistry proxy address (required for onchain
                         list and show-token)
  XVN_MARKETPLACE_CONTRACT  Marketplace proxy address (required for buy)
  XVN_LICENSE_TOKEN      LicenseToken proxy address (required for buy)
  XVN_EVAL_ATTESTATION   EvalAttestationRegistry proxy address (required for attest)
  XVN_MANTLE_RPC_URL     RPC endpoint (default https://rpc.sepolia.mantle.xyz)
  XVN_MANTLE_CHAIN_ID    chain id (default 5003, Mantle Sepolia)

Deployed Mantle Sepolia addresses live in config/mantle-sepolia.toml.
`publish`, `buy`, and `attest` are live onchain. Direct `buy` requires the
signer wallet to hold a standing USDC allowance for the Marketplace
(MockUSDC3009 0x68aA91F73f359035875759e1d4C4949A27c84C88 on Mantle Sepolia).

Read-only verbs (no signer key needed):
  `list` under MARKETPLACE_DRIVER=onchain enumerates the ListingRegistry
  directly (XVN_LISTING_REGISTRY + XVN_IDENTITY_REGISTRY + XVN_MANTLE_RPC_URL).
  `show-token` fetches and decodes IdentityRegistry.tokenURI(token_id)
  (XVN_IDENTITY_REGISTRY + XVN_MANTLE_RPC_URL).";

#[derive(Subcommand, Debug)]
enum MarketplaceAction {
    /// List marketplace listings.
    ///
    /// Default: reads the local fixture file (`XVN_MARKETPLACE_FIXTURE` or
    /// `$XVN_HOME/marketplace/listings.json`) and prints
    /// `agent_id | version | price_usdc | seller | status`.
    ///
    /// With `MARKETPLACE_DRIVER=onchain`: enumerates the deployed
    /// ListingRegistry read-only (no signer) and prints
    /// `listing_id | agent_id | price_usdc | seller | revoked`.
    /// Requires XVN_LISTING_REGISTRY + XVN_IDENTITY_REGISTRY
    /// (+ XVN_MANTLE_RPC_URL, defaulted).
    List,
    /// Show the decoded genart tokenURI metadata for an IdentityRegistry NFT.
    ///
    /// Read-only (no signer). Requires XVN_IDENTITY_REGISTRY
    /// (+ XVN_MANTLE_RPC_URL, defaulted). Prints name, agent_id, the
    /// Symmetry/Palette/Density/Layers attributes, and the decoded SVG
    /// byte length.
    ShowToken {
        /// IdentityRegistry NFT token id.
        #[arg(long)]
        token_id: u64,
        /// Write the decoded SVG to this path.
        #[arg(long)]
        svg_out: Option<PathBuf>,
    },
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
        MarketplaceAction::ShowToken { token_id, svg_out } => show_token(token_id, svg_out.as_deref()).await,
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

/// Whether the operator selected the real on-chain driver via env.
fn onchain_selected() -> bool {
    std::env::var("MARKETPLACE_DRIVER").as_deref() == Ok("onchain")
}

fn driver() -> CliResult<Box<dyn AnchorDriver>> {
    if onchain_selected() {
        let key_hex = std::env::var("MANTLE_PRIVATE_KEY").map_err(|_| {
            CliError::usage(anyhow::anyhow!(
                "MARKETPLACE_DRIVER=onchain requires MANTLE_PRIVATE_KEY to be set \
                 (hex signer key; retrieve via \
                 MANTLE_PRIVATE_KEY=$(op read \"op://Olympus/XVN Wallet/private key\"))"
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

/// `XVN_MANTLE_RPC_URL`, defaulted to Mantle Sepolia (same default as the
/// onchain write driver).
fn rpc_url() -> String {
    std::env::var("XVN_MANTLE_RPC_URL").unwrap_or_else(|_| "https://rpc.sepolia.mantle.xyz".to_string())
}

/// Reads a required contract-address env var; usage error naming the var
/// when missing or unparseable.
fn required_address_env(var: &str) -> CliResult<Address> {
    let raw = std::env::var(var)
        .map_err(|_| CliError::usage(anyhow::anyhow!("{var} must be set (hex contract address)")))?;
    raw.parse::<Address>()
        .map_err(|e| CliError::usage(anyhow::anyhow!("{var} is not a valid address: {e}")))
}

/// One output row for the chain-backed `list`. Pure so it's unit-testable.
fn format_onchain_listing_row(
    listing_id: u64,
    agent_id: &str,
    price_usdc: f64,
    seller: &str,
    revoked: bool,
) -> String {
    format!("{listing_id} | {agent_id} | {price_usdc} | {seller} | {revoked}")
}

/// Converts an on-chain 6-decimal USDC amount to whole USDC.
fn usdc6_to_f64(v: u128) -> f64 {
    v as f64 / 1_000_000.0
}

async fn list() -> CliResult<()> {
    if onchain_selected() {
        return list_onchain().await;
    }
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

/// Chain-backed `list`: read-only enumeration of the ListingRegistry
/// (mirrors the dashboard indexer's `poll_once`, without the snapshot).
async fn list_onchain() -> CliResult<()> {
    let listing_registry_addr = required_address_env("XVN_LISTING_REGISTRY")?;
    let identity_registry_addr = required_address_env("XVN_IDENTITY_REGISTRY")?;
    let rpc = rpc_url();

    let provider = ProviderBuilder::new()
        .connect(rpc.as_str())
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("connect read provider to {rpc}: {e}")))?;
    let listing_registry = IListingRegistry::new(listing_registry_addr, &provider);
    let identity_registry = IIdentityRegistry::new(identity_registry_addr, &provider);

    let total_u256 = listing_registry
        .totalListings()
        .call()
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("totalListings(): {e}")))?;
    let total: u64 = total_u256.try_into().unwrap_or(u64::MAX);
    // Testnet-scale guard (same cap as the dashboard indexer): bound the
    // enumeration so a hostile/buggy registry can't make us issue unbounded
    // RPC calls.
    let total = total.min(10_000);

    if total == 0 {
        println!("(no listings)");
        return Ok(());
    }

    println!("listing_id | agent_id | price_usdc | seller | revoked");
    // Listing ids start at 1; totalListings() returns `_nextListingId - 1`.
    for id in 1..=total {
        let listing = match listing_registry.getListing(U256::from(id)).call().await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("warning: getListing({id}) failed; skipping: {e}");
                continue;
            }
        };
        // agent_id comes from the decoded tokenURI metadata; blank if the
        // tokenURI is unfetchable or undecodable (never drops the listing).
        let agent_id = match identity_registry.tokenURI(listing.agentNftId).call().await {
            Ok(uri) => decode_token_metadata(&uri).agent_id,
            Err(_) => String::new(),
        };
        println!(
            "{}",
            format_onchain_listing_row(
                u64::try_from(listing.listingId).unwrap_or(id),
                &agent_id,
                usdc6_to_f64(listing.priceUSDC.to::<u128>()),
                &format!("{:#x}", listing.seller),
                listing.revoked,
            )
        );
    }
    Ok(())
}

/// `show-token`: fetch + decode `IdentityRegistry.tokenURI(token_id)`.
async fn show_token(token_id: u64, svg_out: Option<&std::path::Path>) -> CliResult<()> {
    let identity_registry_addr = required_address_env("XVN_IDENTITY_REGISTRY")?;
    let rpc = rpc_url();

    let provider = ProviderBuilder::new()
        .connect(rpc.as_str())
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("connect read provider to {rpc}: {e}")))?;
    let identity_registry = IIdentityRegistry::new(identity_registry_addr, &provider);

    let uri = identity_registry
        .tokenURI(U256::from(token_id))
        .call()
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("tokenURI({token_id}): {e}")))?;

    let meta = decode_token_metadata(&uri);
    let svg = decode_svg_image(&meta.image);

    println!("token_id: {token_id}");
    println!("name: {}", meta.name);
    println!("agent_id: {}", meta.agent_id);
    println!("symmetry: {}", meta.symmetry);
    println!("palette: {}", meta.palette);
    println!("density: {}", meta.density);
    println!("layers: {}", meta.layers);
    println!("svg_bytes: {}", svg.as_ref().map(Vec::len).unwrap_or_default());

    if let Some(path) = svg_out {
        let bytes = svg.ok_or_else(|| {
            CliError::upstream(anyhow::anyhow!(
                "tokenURI metadata has no decodable SVG image; cannot write --svg-out"
            ))
        })?;
        std::fs::write(path, bytes)
            .map_err(|e| CliError::upstream(anyhow::anyhow!("write {}: {e}", path.display())))?;
        println!("svg_out: {}", path.display());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onchain_listing_row_format() {
        assert_eq!(
            format_onchain_listing_row(
                7,
                "01HXTESTAGENT",
                49.5,
                "0xb5d2a3734af76efb7bc258b35c970f1cc9c4e553",
                false
            ),
            "7 | 01HXTESTAGENT | 49.5 | 0xb5d2a3734af76efb7bc258b35c970f1cc9c4e553 | false"
        );
        // Undecodable tokenURI → blank agent_id column, row still printed.
        assert_eq!(
            format_onchain_listing_row(3, "", 0.0, "0x00", true),
            "3 |  | 0 | 0x00 | true"
        );
    }

    #[test]
    fn usdc6_conversion() {
        assert_eq!(usdc6_to_f64(1_000_000), 1.0);
        assert_eq!(usdc6_to_f64(49_500_000), 49.5);
        assert_eq!(usdc6_to_f64(0), 0.0);
    }
}

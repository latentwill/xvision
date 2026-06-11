//! Marketplace indexer core — chain reader + tokenURI decoder + shared snapshot.
//!
//! Polls the on-chain `ListingRegistry` (ids start at 1; `totalListings()`
//! returns `_nextListingId - 1`, so the live id range is `1..=total`) and the
//! `IdentityRegistry` (`tokenURI(agentNftId)` → `data:application/json;base64,…`
//! genart metadata from `xvision_identity::generate_token_uri`). The decoded
//! result is held in a [`SharedSnapshot`] for read routes (wired in a later
//! task — this module only defines the types, the one-shot poll, and the
//! background spawn).
//!
//! Read-only chain access: a plain `ProviderBuilder::new().connect(rpc_url)`
//! provider, no signer (same construction as `IdentityClient::connect`, minus
//! the chain-id check — the indexer trusts the configured RPC).
//!
//! Degradation policy:
//! - a failed `getListing(id)` skips that id with a logged warning;
//! - a failed/undecodable `tokenURI` keeps the listing with empty metadata
//!   fields ([`decode_token_metadata`] never errors);
//! - a failed poll keeps the previous snapshot's listings and surfaces the
//!   error in `last_error`.

use std::time::Duration;

use alloy::primitives::{Address, U256};
use alloy::providers::ProviderBuilder;
use anyhow::Context;

use xvision_identity::client::IIdentityRegistry;
use xvision_identity::contracts::IListingRegistry;
use xvision_identity::token_metadata::{decode_token_metadata, TokenMetadata};

// ---------------------------------------------------------------------------
// Snapshot types
// ---------------------------------------------------------------------------

/// One decoded on-chain listing, denormalized for the read API / frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexedListing {
    pub listing_id: u64,
    /// IdentityRegistry NFT id (U256 as decimal string).
    pub agent_nft_id: String,
    /// Pre-mint agent ULID decoded from the tokenURI metadata JSON
    /// (`""` if the tokenURI was unfetchable or undecodable).
    pub agent_id: String,
    /// Seller address, `0x…` lowercase (non-checksummed).
    pub seller: String,
    /// keccak256 manifest hash, 64-char lowercase hex (no `0x`).
    pub content_hash: String,
    pub content_uri: String,
    pub tier: u8,
    /// On-chain 6-decimal USDC amount converted to whole USDC.
    pub price_usdc: f64,
    pub transferable_license: bool,
    pub revoked: bool,
    /// `"{agent_id}:{content_hash}"` — an empty agent_id still yields
    /// `":{hash}"` so the genart renderer gets a deterministic seed.
    pub gen_art_seed: String,
    /// Metadata `"name"` (`""` if undecodable).
    pub name: String,
    /// `Symmetry` attribute value, for display (`""` if absent).
    pub symmetry: String,
    /// `Palette` attribute value, for display (`""` if absent).
    pub palette: String,
}

/// The full indexed view of the marketplace, replaced atomically per poll.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct MarketplaceSnapshot {
    pub listings: Vec<IndexedListing>,
    pub last_poll_unix: i64,
    pub last_error: Option<String>,
    pub total_onchain: u64,
}

/// Shared handle: the indexer task writes, read routes read.
pub type SharedSnapshot = std::sync::Arc<tokio::sync::RwLock<MarketplaceSnapshot>>;

/// Indexer connection config (read-only — no signer).
pub struct IndexerCfg {
    pub rpc_url: String,
    pub listing_registry: Address,
    pub identity_registry: Address,
}

impl IndexerCfg {
    /// Reads `XVN_RPC_URL`, `XVN_LISTING_REGISTRY`, `XVN_IDENTITY_REGISTRY`.
    /// Returns `None` when any is missing or an address fails to parse —
    /// the indexer then stays dormant (mirrors `ChainEnv::from_env` in
    /// `routes/marketplace.rs`).
    pub fn from_env() -> Option<Self> {
        let rpc_url = std::env::var("XVN_RPC_URL").ok()?;
        let listing_registry: Address = std::env::var("XVN_LISTING_REGISTRY").ok()?.parse().ok()?;
        let identity_registry: Address = std::env::var("XVN_IDENTITY_REGISTRY").ok()?.parse().ok()?;
        Some(Self {
            rpc_url,
            listing_registry,
            identity_registry,
        })
    }
}

// ---------------------------------------------------------------------------
// Pure field helpers
// ---------------------------------------------------------------------------

/// Converts an on-chain 6-decimal USDC amount to whole USDC.
pub(crate) fn usdc6_to_f64(v: u128) -> f64 {
    v as f64 / 1_000_000.0
}

/// Composes the deterministic genart seed: `"{agent_id}:{content_hash}"`.
/// An empty agent_id still yields `":{hash}"`.
pub(crate) fn gen_art_seed(agent_id: &str, content_hash: &str) -> String {
    format!("{agent_id}:{content_hash}")
}

/// `bytes32` → 64-char lowercase hex without the `0x` prefix.
fn hex64(bytes: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(64);
    for b in bytes {
        write!(out, "{b:02x}").expect("string write");
    }
    out
}

// ---------------------------------------------------------------------------
// Chain reader
// ---------------------------------------------------------------------------

/// One full read pass over the marketplace contracts.
///
/// Errors only on connection / `totalListings()` failure. Per-listing
/// failures degrade: a failed `getListing` skips the id (logged), a failed
/// `tokenURI` keeps the listing with empty metadata.
pub async fn poll_once(cfg: &IndexerCfg) -> anyhow::Result<MarketplaceSnapshot> {
    let provider = ProviderBuilder::new()
        .connect(cfg.rpc_url.as_str())
        .await
        .with_context(|| format!("connecting read provider to {}", cfg.rpc_url))?;

    let listing_registry = IListingRegistry::new(cfg.listing_registry, &provider);
    let identity_registry = IIdentityRegistry::new(cfg.identity_registry, &provider);

    let total_u256 = listing_registry
        .totalListings()
        .call()
        .await
        .context("totalListings()")?;
    let total: u64 = total_u256.try_into().unwrap_or(u64::MAX);
    // Testnet-scale guard: cap the per-poll enumeration so a hostile/buggy
    // registry can't make us issue unbounded RPC calls. Revisit with
    // persistence/pagination past ~500 listings (per plan).
    let total = total.min(10_000);

    let mut listings = Vec::with_capacity(total as usize);
    // Listing ids start at 1 (`_nextListingId = 1` in ListingRegistry.sol);
    // totalListings() returns `_nextListingId - 1`, so the range is 1..=total.
    for id in 1..=total {
        let listing = match listing_registry.getListing(U256::from(id)).call().await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(listing_id = id, error = %e, "getListing failed; skipping listing");
                continue;
            }
        };

        let meta = match identity_registry.tokenURI(listing.agentNftId).call().await {
            Ok(uri) => decode_token_metadata(&uri),
            Err(e) => {
                tracing::warn!(
                    listing_id = id,
                    agent_nft_id = %listing.agentNftId,
                    error = %e,
                    "tokenURI fetch failed; keeping listing with empty metadata"
                );
                TokenMetadata::default()
            }
        };

        let content_hash = hex64(&listing.contentHash.0);
        listings.push(IndexedListing {
            listing_id: u64::try_from(listing.listingId).unwrap_or(id),
            agent_nft_id: listing.agentNftId.to_string(),
            agent_id: meta.agent_id.clone(),
            seller: format!("{:#x}", listing.seller),
            gen_art_seed: gen_art_seed(&meta.agent_id, &content_hash),
            content_hash,
            content_uri: listing.contentURI.clone(),
            tier: listing.tier,
            price_usdc: usdc6_to_f64(listing.priceUSDC.to::<u128>()),
            transferable_license: listing.transferableLicense,
            revoked: listing.revoked,
            name: meta.name,
            symmetry: meta.symmetry,
            palette: meta.palette,
        });
    }

    Ok(MarketplaceSnapshot {
        listings,
        last_poll_unix: chrono::Utc::now().timestamp(),
        last_error: None,
        total_onchain: total,
    })
}

// ---------------------------------------------------------------------------
// Background task
// ---------------------------------------------------------------------------

/// Spawns the 30s polling loop. First tick fires immediately. A successful
/// poll replaces the snapshot wholesale; a failed poll keeps the previous
/// listings and records `last_error` + the attempt time.
pub fn spawn_indexer(snapshot: SharedSnapshot, cfg: IndexerCfg) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            match poll_once(&cfg).await {
                Ok(fresh) => {
                    *snapshot.write().await = fresh;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "marketplace indexer poll failed; keeping previous snapshot");
                    let mut guard = snapshot.write().await;
                    guard.last_error = Some(e.to_string());
                    guard.last_poll_unix = chrono::Utc::now().timestamp();
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // decode_token_metadata / base64 tests moved to
    // `xvision_identity::token_metadata` with the decoder (2026-06-11).

    // -- price conversion --------------------------------------------------

    #[test]
    fn usdc6_conversion() {
        assert_eq!(usdc6_to_f64(1_000_000), 1.0);
        assert_eq!(usdc6_to_f64(49_500_000), 49.5);
        assert_eq!(usdc6_to_f64(0), 0.0);
    }

    // -- gen_art_seed -------------------------------------------------------

    #[test]
    fn gen_art_seed_composition() {
        let hash = "ab".repeat(32);
        assert_eq!(
            gen_art_seed("01HXTESTAGENT", &hash),
            format!("01HXTESTAGENT:{hash}")
        );
        // Empty agent_id still produces ":{hash}".
        assert_eq!(gen_art_seed("", &hash), format!(":{hash}"));
    }

    // -- IndexerCfg::from_env ------------------------------------------------

    #[test]
    fn indexer_cfg_missing_env_is_none() {
        // This test only REMOVES the vars (never sets them) — the same
        // contract as `chain_env_missing_is_none` in routes/marketplace.rs,
        // so the two cannot race under parallel test threads.
        std::env::remove_var("XVN_RPC_URL");
        std::env::remove_var("XVN_LISTING_REGISTRY");
        std::env::remove_var("XVN_IDENTITY_REGISTRY");
        assert!(IndexerCfg::from_env().is_none());
    }

    // -- hex64 ---------------------------------------------------------------

    #[test]
    fn hex64_lowercase_no_prefix() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0xAB;
        bytes[31] = 0x01;
        let h = hex64(&bytes);
        assert_eq!(h.len(), 64);
        assert!(h.starts_with("ab"));
        assert!(h.ends_with("01"));
        assert!(!h.contains("0x"));
    }
}

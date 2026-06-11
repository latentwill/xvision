//! Read routes over the marketplace indexer snapshot.
//!
//! - `GET /api/marketplace/status` — indexer liveness + last poll info.
//! - `GET /api/marketplace/listings` — indexed listings (revoked filtered
//!   out unless `?include_revoked=1`).
//! - `GET /api/marketplace/listings/:id` — single listing or 404.
//! - `GET /api/marketplace/wallet/:address` — per-wallet view: owned
//!   strategy NFTs, license balances, and seller listings.
//!
//! Handlers stay thin: all aggregation is the pure [`wallet_view`] over the
//! snapshot plus [`OwnershipFacts`] gathered from the chain. Chain access
//! (ownerOf / ERC-1155 balanceOf) reuses the indexer's read-only provider
//! construction; license lookups additionally need `XVN_LICENSE_TOKEN` and
//! silently yield an empty `licenses` array when it's unset.

use std::collections::{HashMap, HashSet};

use alloy::primitives::{Address, U256};
use alloy::providers::ProviderBuilder;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use xvision_identity::client::IIdentityRegistry;
use xvision_identity::contracts::ILicenseToken;

use crate::error::DashboardError;
use crate::marketplace_index::{IndexedListing, IndexerCfg, MarketplaceSnapshot};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/marketplace/status
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct StatusOut {
    /// True only when the indexer task was spawned AND at least one poll
    /// attempt has completed.
    pub active: bool,
    pub last_poll_unix: i64,
    pub total_onchain: u64,
    pub last_error: Option<String>,
}

pub async fn get_status(State(state): State<AppState>) -> Json<StatusOut> {
    let snap = state.marketplace_snapshot.read().await;
    Json(StatusOut {
        active: state.marketplace_indexer_active() && snap.last_poll_unix > 0,
        last_poll_unix: snap.last_poll_unix,
        total_onchain: snap.total_onchain,
        last_error: snap.last_error.clone(),
    })
}

// ---------------------------------------------------------------------------
// GET /api/marketplace/listings[?include_revoked=1]
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListingsQuery {
    #[serde(default)]
    include_revoked: Option<String>,
}

impl ListingsQuery {
    fn include_revoked(&self) -> bool {
        matches!(self.include_revoked.as_deref(), Some("1") | Some("true"))
    }
}

#[derive(Debug, Serialize)]
pub struct ListingsOut {
    pub items: Vec<IndexedListing>,
    /// Count of `items` (post-filter), NOT the on-chain total — that lives
    /// on the status route as `total_onchain`.
    pub total: usize,
}

pub async fn get_listings(
    State(state): State<AppState>,
    Query(q): Query<ListingsQuery>,
) -> Json<ListingsOut> {
    let snap = state.marketplace_snapshot.read().await;
    let items: Vec<IndexedListing> = snap
        .listings
        .iter()
        .filter(|l| q.include_revoked() || !l.revoked)
        .cloned()
        .collect();
    let total = items.len();
    Json(ListingsOut { items, total })
}

// ---------------------------------------------------------------------------
// GET /api/marketplace/listings/:id
// ---------------------------------------------------------------------------

pub async fn get_listing(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<IndexedListing>, DashboardError> {
    let snap = state.marketplace_snapshot.read().await;
    snap.listings
        .iter()
        .find(|l| l.listing_id == id)
        .cloned()
        .map(Json)
        .ok_or_else(|| DashboardError::NotFound(format!("listing {id} not in indexed snapshot")))
}

// ---------------------------------------------------------------------------
// GET /api/marketplace/wallet/:address
// ---------------------------------------------------------------------------

/// One strategy NFT the wallet owns, denormalized from its listing(s).
#[derive(Debug, Serialize, PartialEq)]
pub struct WalletStrategy {
    /// IdentityRegistry token id (decimal string — `agent_nft_id`).
    pub token_id: String,
    pub agent_id: String,
    pub name: String,
    pub gen_art_seed: String,
    /// True when at least one non-revoked listing references this token.
    pub listed: bool,
    /// The first non-revoked listing's id, when `listed`.
    pub listing_id: Option<u64>,
}

/// One ERC-1155 license position (balance > 0).
#[derive(Debug, Serialize, PartialEq)]
pub struct WalletLicense {
    pub listing_id: u64,
    pub agent_id: String,
    pub name: String,
    pub gen_art_seed: String,
    pub balance: u64,
}

#[derive(Debug, Serialize)]
pub struct WalletView {
    pub address: String,
    pub strategies: Vec<WalletStrategy>,
    pub licenses: Vec<WalletLicense>,
    /// Listings where this wallet is the seller (revoked included — the
    /// seller wants to see their own dead listings).
    pub listings: Vec<IndexedListing>,
}

/// Chain-derived facts about one wallet, fed into [`wallet_view`].
#[derive(Debug, Default, Clone)]
pub struct OwnershipFacts {
    /// `agent_nft_id`s (decimal strings) whose `ownerOf` == the wallet.
    pub owned_token_ids: HashSet<String>,
    /// listing_id → ERC-1155 license balance (only entries > 0).
    pub license_balances: HashMap<u64, u64>,
}

/// Pure aggregation of the wallet view from the snapshot + chain facts.
///
/// Address comparison is case-insensitive (the snapshot stores sellers as
/// lowercase `0x…`, but the caller may pass checksummed input). Ordering is
/// deterministic: snapshot listing order, first occurrence per token.
pub fn wallet_view(
    snapshot: &MarketplaceSnapshot,
    address: &str,
    ownership: &OwnershipFacts,
) -> WalletView {
    let address_lc = address.to_lowercase();

    // Owned strategies: one entry per unique owned token, metadata from the
    // first listing referencing it.
    let mut seen_tokens = HashSet::new();
    let mut strategies = Vec::new();
    for l in &snapshot.listings {
        if !ownership.owned_token_ids.contains(&l.agent_nft_id)
            || !seen_tokens.insert(l.agent_nft_id.clone())
        {
            continue;
        }
        let live = snapshot
            .listings
            .iter()
            .find(|x| x.agent_nft_id == l.agent_nft_id && !x.revoked);
        strategies.push(WalletStrategy {
            token_id: l.agent_nft_id.clone(),
            agent_id: l.agent_id.clone(),
            name: l.name.clone(),
            gen_art_seed: l.gen_art_seed.clone(),
            listed: live.is_some(),
            listing_id: live.map(|x| x.listing_id),
        });
    }

    let licenses = snapshot
        .listings
        .iter()
        .filter_map(|l| {
            let balance = *ownership.license_balances.get(&l.listing_id)?;
            (balance > 0).then(|| WalletLicense {
                listing_id: l.listing_id,
                agent_id: l.agent_id.clone(),
                name: l.name.clone(),
                gen_art_seed: l.gen_art_seed.clone(),
                balance,
            })
        })
        .collect();

    let listings = snapshot
        .listings
        .iter()
        .filter(|l| l.seller.to_lowercase() == address_lc)
        .cloned()
        .collect();

    WalletView {
        address: address_lc,
        strategies,
        licenses,
        listings,
    }
}

/// Validates a `0x` + 40-hex-char Ethereum address (any case).
fn is_eth_address(s: &str) -> bool {
    s.len() == 42
        && s.starts_with("0x")
        && s.as_bytes()[2..].iter().all(u8::is_ascii_hexdigit)
}

/// Reads the optional `XVN_LICENSE_TOKEN` address. `None` → license lookups
/// are skipped (empty `licenses`), never an error.
fn license_token_from_env() -> Option<Address> {
    std::env::var("XVN_LICENSE_TOKEN").ok()?.parse().ok()
}

/// Gathers [`OwnershipFacts`] from the chain. Degrades, never errors: a
/// failed provider connect or a reverted call (e.g. `ownerOf` on a burned
/// token) is treated as not-owned / zero-balance.
async fn fetch_ownership_facts(
    cfg: &IndexerCfg,
    snapshot: &MarketplaceSnapshot,
    address: Address,
) -> OwnershipFacts {
    let mut facts = OwnershipFacts::default();
    let provider = match ProviderBuilder::new().connect(cfg.rpc_url.as_str()).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "wallet view: provider connect failed; returning empty ownership facts");
            return facts;
        }
    };

    let identity = IIdentityRegistry::new(cfg.identity_registry, &provider);
    let mut seen = HashSet::new();
    for l in &snapshot.listings {
        if !seen.insert(l.agent_nft_id.as_str()) {
            continue;
        }
        let Ok(token) = U256::from_str_radix(&l.agent_nft_id, 10) else {
            continue;
        };
        // Revert / missing token → not owned.
        if let Ok(owner) = identity.ownerOf(token).call().await {
            if owner == address {
                facts.owned_token_ids.insert(l.agent_nft_id.clone());
            }
        }
    }

    if let Some(license_token) = license_token_from_env() {
        let license = ILicenseToken::new(license_token, &provider);
        for l in &snapshot.listings {
            if let Ok(balance) = license.balanceOf(address, U256::from(l.listing_id)).call().await {
                let balance: u64 = balance.try_into().unwrap_or(u64::MAX);
                if balance > 0 {
                    facts.license_balances.insert(l.listing_id, balance);
                }
            }
        }
    }

    facts
}

pub async fn get_wallet(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<WalletView>, DashboardError> {
    if !is_eth_address(&address) {
        return Err(DashboardError::Validation {
            field: "address".into(),
            msg: "must be a 0x-prefixed 40-hex-char address".into(),
        });
    }
    if !state.marketplace_indexer_active() {
        return Err(DashboardError::ServiceUnavailable(
            "marketplace indexer not running: set XVN_RPC_URL, XVN_LISTING_REGISTRY, \
             XVN_IDENTITY_REGISTRY"
                .into(),
        ));
    }

    let snapshot = state.marketplace_snapshot.read().await.clone();

    // The flag implies the env parsed at startup; a vanished env mid-process
    // degrades to empty facts rather than erroring.
    let facts = match (IndexerCfg::from_env(), address.parse::<Address>()) {
        (Some(cfg), Ok(addr)) => fetch_ownership_facts(&cfg, &snapshot, addr).await,
        _ => OwnershipFacts::default(),
    };

    Ok(Json(wallet_view(&snapshot, &address, &facts)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BOB: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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
        }
    }

    fn snapshot(listings: Vec<IndexedListing>) -> MarketplaceSnapshot {
        MarketplaceSnapshot {
            total_onchain: listings.len() as u64,
            listings,
            last_poll_unix: 1_700_000_000,
            last_error: None,
        }
    }

    #[test]
    fn owned_token_marked_listed_with_listing_id() {
        let snap = snapshot(vec![listing(1, "7", ALICE, false)]);
        let facts = OwnershipFacts {
            owned_token_ids: HashSet::from(["7".to_string()]),
            license_balances: HashMap::new(),
        };
        let view = wallet_view(&snap, ALICE, &facts);
        assert_eq!(view.strategies.len(), 1);
        let s = &view.strategies[0];
        assert_eq!(s.token_id, "7");
        assert_eq!(s.agent_id, "agent-7");
        assert!(s.listed);
        assert_eq!(s.listing_id, Some(1));
    }

    #[test]
    fn owned_token_with_only_revoked_listing_is_unlisted() {
        let snap = snapshot(vec![listing(1, "7", ALICE, true)]);
        let facts = OwnershipFacts {
            owned_token_ids: HashSet::from(["7".to_string()]),
            license_balances: HashMap::new(),
        };
        let view = wallet_view(&snap, ALICE, &facts);
        assert_eq!(view.strategies.len(), 1);
        assert!(!view.strategies[0].listed);
        assert_eq!(view.strategies[0].listing_id, None);
    }

    #[test]
    fn license_balance_surfaces() {
        let snap = snapshot(vec![listing(1, "7", BOB, false), listing(2, "8", BOB, false)]);
        let facts = OwnershipFacts {
            owned_token_ids: HashSet::new(),
            license_balances: HashMap::from([(2, 3u64)]),
        };
        let view = wallet_view(&snap, ALICE, &facts);
        assert!(view.strategies.is_empty());
        assert_eq!(view.licenses.len(), 1);
        let lic = &view.licenses[0];
        assert_eq!(lic.listing_id, 2);
        assert_eq!(lic.agent_id, "agent-8");
        assert_eq!(lic.balance, 3);
    }

    #[test]
    fn seller_listings_filter_includes_revoked_and_excludes_others() {
        let snap = snapshot(vec![
            listing(1, "7", ALICE, false),
            listing(2, "8", ALICE, true),
            listing(3, "9", BOB, false),
        ]);
        let view = wallet_view(&snap, ALICE, &OwnershipFacts::default());
        let ids: Vec<u64> = view.listings.iter().map(|l| l.listing_id).collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn address_comparison_is_case_insensitive() {
        let snap = snapshot(vec![listing(1, "7", ALICE, false)]);
        let checksummed = "0xAaAaAAaaaAAAAAaAaaaAAAaaAaaaAAaaAaAaaaaA";
        let view = wallet_view(&snap, checksummed, &OwnershipFacts::default());
        assert_eq!(view.listings.len(), 1);
        assert_eq!(view.address, ALICE);
    }

    #[test]
    fn eth_address_validation() {
        assert!(is_eth_address(ALICE));
        assert!(is_eth_address("0xAbCdEf0123456789abcdef0123456789ABCDEF01"));
        assert!(!is_eth_address("0x123")); // too short
        assert!(!is_eth_address(&format!("1x{}", "a".repeat(40)))); // bad prefix
        assert!(!is_eth_address(&format!("0x{}g", "a".repeat(39)))); // non-hex
    }
}

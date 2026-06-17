//! Read routes over the marketplace indexer snapshot.
//!
//! - `GET /api/marketplace/status` — indexer liveness + last poll info +
//!   the env-configured contract addresses (frontend discovers addresses
//!   here, nothing hardcoded in the bundle).
//! - `GET /api/marketplace/listings` — indexed listings (revoked filtered
//!   out unless `?include_revoked=1`).
//! - `GET /api/marketplace/listings/:id` — single listing or 404.
//! - `GET /api/marketplace/wallet/:address` — per-wallet view: owned
//!   strategy NFTs, license balances, and seller listings.
//! - `GET /api/marketplace/receipts/:tx_hash` — decoded `Sold` event for a
//!   purchase tx, joined with listing metadata from the snapshot.
//! - `GET /api/marketplace/listings/:id/bundle` — fetch the manifest bytes
//!   behind a listing's `content_uri` (ipfs:// gateway or xvn:// local
//!   store) and verify them against the on-chain `content_hash` (409 on
//!   mismatch).
//! - `GET /api/marketplace/listings/:id/attestations` — the listing's eval
//!   attestations read live from the `EvalAttestationRegistry`.
//!
//! Handlers stay thin: all aggregation is the pure [`wallet_view`] over the
//! snapshot plus [`OwnershipFacts`] gathered from the chain. Chain access
//! (ownerOf / ERC-1155 balanceOf) reuses the indexer's read-only provider
//! config from the startup-resolved `MarketplaceChainConfig`; license
//! lookups additionally need its `license_token` (`XVN_LICENSE_TOKEN`) and
//! silently yield an empty `licenses` array when it's unset.

use std::collections::{HashMap, HashSet};

use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::sol_types::SolEvent;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use xvision_engine::api::strategy;
use xvision_engine::autooptimizer::content_hash::canonical_json;
use xvision_identity::client::IIdentityRegistry;
use xvision_identity::contracts::{IEvalAttestationRegistry, ILicenseToken, IMarketplace};
use xvision_identity::manifest_hash_hex;
use xvision_marketplace::{IpfsStore, PinataDriver};

use super::marketplace::{explorer_base, network_label};
use crate::chain_config::{chain_call_timeout, with_chain_timeout};
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
    /// Env-configured contract addresses (each `null` when unset/invalid).
    /// The frontend discovers addresses here — never hardcoded in the bundle.
    pub contracts: ContractsOut,
    /// Lit Protocol (Chipotle) config for the sealed-tier client-side decrypt
    /// flow — `null` when Lit is unconfigured (sealed publishing disabled).
    /// The API key is NEVER exposed here; the browser only needs the public
    /// `api_base` / `gate_action_cid` / `pkp_id` to invoke the gate action.
    pub lit: Option<LitStatusOut>,
    /// Public IPFS read gateway base (no trailing slash, no `/ipfs`) the
    /// frontend uses to build "open bundle" links: `${public_gateway}/ipfs/<cid>`.
    /// Sourced from the configured backend's gateway (`PINATA_GATEWAY`) when
    /// set, else the vendor-neutral default. This is the READ gateway only —
    /// the pinning API URL is NEVER exposed here.
    pub public_gateway: String,
    /// The active chain the backend is configured for (from `XVN_CHAIN_ID` /
    /// the startup-resolved signer). `null` when no chain is configured (no
    /// signer) — the SPA then falls back to its build-time default network.
    ///
    /// This is the SINGLE SOURCE OF TRUTH for the network: the frontend selects
    /// its chain / EIP-712 USDC domain / wallet-switch target from `chain_id`
    /// here, so one prebuilt bundle works on testnet or mainnet purely by the
    /// backend's `XVN_CHAIN_ID` — no build-time `VITE_MARKETPLACE_NETWORK` needed.
    pub network: Option<NetworkOut>,
}

/// The active marketplace network, derived from the backend's configured chain
/// id. `network` is the same slug the receipt/explorer surfaces use
/// (`network_label`); `explorer_base` is its canonical Mantle explorer.
#[derive(Debug, Serialize)]
pub struct NetworkOut {
    pub chain_id: u64,
    pub network: String,
    pub explorer_base: Option<String>,
}

/// Pure mapping from a configured chain id to the public network block (env read
/// split out for testability, mirroring `resolve_public_gateway`). `None` chain
/// id → `None` block, so the frontend falls back to its build-time default.
fn network_out(chain_id: Option<u64>) -> Option<NetworkOut> {
    chain_id.map(|id| NetworkOut {
        chain_id: id,
        network: network_label(Some(id)),
        explorer_base: explorer_base(Some(id)).map(|s| s.to_string()),
    })
}

/// Vendor-neutral default public read gateway. Mirrors
/// `xvision_marketplace::ipfs::DEFAULT_GATEWAY` (kept in sync; that constant is
/// private to the marketplace crate). `dweb.link` is the IPFS-canonical public
/// gateway, not a vendor product.
const DEFAULT_PUBLIC_GATEWAY: &str = "https://dweb.link";

/// The public read gateway base for browser "open bundle" links. Resolution,
/// in order: `XVN_PUBLIC_GATEWAY` (the operator's branded public read gateway,
/// e.g. `https://ipfs.example.com`) → the legacy `PINATA_GATEWAY` (alternative
/// backend) → the vendor-neutral default `dweb.link`. Returns only the gateway
/// base — NEVER the pinning API URL, and never a localhost/API address.
fn public_gateway(state: &AppState) -> String {
    let backend_gateway = state
        .marketplace_chain()
        .and_then(|c| c.ipfs.as_ref())
        .map(|b| b.gateway().to_string());
    resolve_public_gateway(std::env::var("XVN_PUBLIC_GATEWAY").ok(), backend_gateway)
}

/// Pure resolver (env read split out for testability): `XVN_PUBLIC_GATEWAY`
/// override → configured backend gateway → vendor-neutral default. Trims
/// trailing slashes; empty/whitespace values are treated as unset.
fn resolve_public_gateway(env_override: Option<String>, backend: Option<String>) -> String {
    let clean = |s: String| {
        let t = s.trim().trim_end_matches('/').to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    };
    env_override
        .and_then(clean)
        .or_else(|| backend.and_then(clean))
        .unwrap_or_else(|| DEFAULT_PUBLIC_GATEWAY.to_string())
}

/// Public-safe Lit config for the frontend. Deliberately omits `api_key`.
#[derive(Debug, Serialize)]
pub struct LitStatusOut {
    pub api_base: String,
    pub gate_action_cid: String,
    pub pkp_id: String,
}

/// The marketplace contract address book, as configured via env. Still read
/// per request (cheap, and keeps the discovery surface honest if env changes
/// mid-process) — deliberately NOT moved to the startup-resolved
/// `MarketplaceChainConfig`: this is a display/discovery payload, not a
/// chain gate.
#[derive(Debug, Serialize)]
pub struct ContractsOut {
    pub marketplace: Option<String>,
    pub usdc: Option<String>,
    pub license_token: Option<String>,
    pub listing_registry: Option<String>,
    pub identity_registry: Option<String>,
}

/// Reads `key` and normalizes it to a lowercase `0x…` address string.
/// Unset or unparseable → `None` (the frontend treats null as "not deployed").
fn env_addr(key: &str) -> Option<String> {
    let addr: Address = std::env::var(key).ok()?.parse().ok()?;
    Some(format!("{addr:#x}"))
}

/// Env var names mirror `MarketplaceAddresses::from_env` in
/// `xvision_identity::contracts` (plus the identity registry used by the
/// indexer / publish mint path).
fn contracts_from_env() -> ContractsOut {
    ContractsOut {
        marketplace: env_addr("XVN_MARKETPLACE_CONTRACT"),
        usdc: env_addr("XVN_MARKETPLACE_USDC"),
        license_token: env_addr("XVN_LICENSE_TOKEN"),
        listing_registry: env_addr("XVN_LISTING_REGISTRY"),
        identity_registry: env_addr("XVN_IDENTITY_REGISTRY"),
    }
}

/// The public-safe Lit status block from the startup-resolved config. Returns
/// `None` when Lit is unconfigured. NEVER includes the API key.
fn lit_status(state: &AppState) -> Option<LitStatusOut> {
    let lit = state.marketplace_chain().and_then(|c| c.lit.as_ref())?;
    Some(LitStatusOut {
        api_base: lit.api_base.clone(),
        gate_action_cid: lit.gate_action_cid.clone(),
        pkp_id: lit.pkp_id.clone(),
    })
}

pub async fn get_status(State(state): State<AppState>) -> Json<StatusOut> {
    let snap = state.marketplace_snapshot.read().await;
    // Chain id from the startup-resolved signer (set from XVN_CHAIN_ID). `None`
    // when no signer is configured → the frontend keeps its build-time default.
    let chain_id = state
        .marketplace_chain()
        .and_then(|c| c.chain.as_ref())
        .map(|s| s.chain_id);
    Json(StatusOut {
        active: state.marketplace_indexer_active() && snap.last_poll_unix > 0,
        last_poll_unix: snap.last_poll_unix,
        total_onchain: snap.total_onchain,
        last_error: snap.last_error.clone(),
        contracts: contracts_from_env(),
        lit: lit_status(&state),
        public_gateway: public_gateway(&state),
        network: network_out(chain_id),
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

/// Resolves a listing by ULID-first, then integer-fallback.
///
/// When `id` matches an `IndexedListing.agent_id` (ULID), that listing is
/// returned. Otherwise `id` is parsed as a `u64` and matched against
/// `listing_id`. This lets both canonical ULID paths
/// (`/api/marketplace/listings/<ulid>`) and legacy numeric paths
/// (`/api/marketplace/listings/42`) work without a migration.
fn resolve_listing<'a>(listings: &'a [IndexedListing], id: &str) -> Option<&'a IndexedListing> {
    // ULID-first: exact match on agent_id.
    if let Some(l) = listings.iter().find(|l| l.agent_id == id) {
        return Some(l);
    }
    // Integer fallback: parse and match listing_id.
    let numeric_id: u64 = id.parse().ok()?;
    listings.iter().find(|l| l.listing_id == numeric_id)
}

pub async fn get_listing(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<IndexedListing>, DashboardError> {
    let snap = state.marketplace_snapshot.read().await;
    resolve_listing(&snap.listings, &id)
        .cloned()
        .map(Json)
        .ok_or_else(|| DashboardError::NotFound(format!("listing {id} not in indexed snapshot")))
}

// ---------------------------------------------------------------------------
// GET /api/marketplace/listings/:id/bundle
// ---------------------------------------------------------------------------

/// Response for a verified OPEN-tier bundle fetch. `verified` is always `true`
/// when this shape is returned at all — a hash mismatch is a 409, never a
/// `verified: false` payload, so no caller can accidentally use unverified
/// bytes.
#[derive(Debug, Serialize)]
pub struct BundleOut {
    pub listing_id: u64,
    pub content_uri: String,
    pub verified: bool,
    /// The canonical strategy manifest, parsed back into JSON.
    pub manifest: serde_json::Value,
}

/// Response for a SEALED-tier bundle fetch. The server NEVER decrypts — it
/// returns the opaque ciphertext (for the browser to hand to the Lit gate
/// action) plus the on-chain `content_hash` (the keccak256 of the canonical
/// PLAINTEXT) so the browser can integrity-check `keccak256(decrypted)` after
/// a licensed decrypt. No plaintext manifest is ever returned here.
#[derive(Debug, Serialize)]
pub struct SealedBundleOut {
    pub listing_id: u64,
    pub content_uri: String,
    /// Always `true` — marks this as the sealed shape so the frontend routes
    /// to the decrypt flow instead of expecting a plaintext manifest.
    pub encrypted: bool,
    /// The opaque sealed blob fetched from `content_uri` (handed to Lit).
    pub ciphertext: String,
    /// `0x`-less 64-hex keccak256 of the canonical plaintext manifest (the
    /// on-chain `content_hash`). The browser verifies the decrypted manifest
    /// against this.
    pub content_hash: String,
}

/// The two bundle shapes. Untagged so the open shape stays byte-identical to
/// the pre-sealed `BundleOut` (no wire-break for existing open-tier callers);
/// the sealed shape is distinguished by its `encrypted: true` marker.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum BundleResponse {
    Open(BundleOut),
    Sealed(SealedBundleOut),
}

/// Fetches the manifest bytes behind a listing's `content_uri` and verifies
/// them against the listing's on-chain `content_hash`. Shared by the bundle
/// route and the license-gated import route (`POST …/:id/import`).
///
/// Resolution:
/// - `ipfs://<cid>` — `get` through the startup-resolved IPFS backend
///   (Kubo when `XVN_IPFS_API_URL` is set, else Pinata). When no backend is
///   configured, falls back to an unauthenticated [`PinataDriver`] gateway
///   GET (`PINATA_GATEWAY` override, default the public Pinata gateway) so
///   already-pinned content stays fetchable. Fetch failure → 503 (upstream
///   dependency, not a client error).
/// - `xvn://strategy/<ulid>` — local store load + canonical JSON (404 when
///   the strategy file is absent on this host).
/// - anything else → 503 with the scheme named (an unsupported scheme means
///   this server version can't deliver the listing, not that it's gone).
///
/// Integrity failures (hash mismatch, non-UTF-8 or non-JSON bytes that
/// nevertheless hashed correctly) are 409 `Conflict` — the listing and its
/// content disagree.
pub(crate) async fn fetch_verified_manifest(
    state: &AppState,
    listing: &IndexedListing,
) -> Result<serde_json::Value, DashboardError> {
    let bytes: Vec<u8> = if let Some(cid) = listing.content_uri.strip_prefix("ipfs://") {
        let fetched = match state.marketplace_chain().and_then(|c| c.ipfs.as_ref()) {
            Some(backend) => backend.get(cid).await,
            None => {
                // No configured backend: keep already-pinned content
                // fetchable through a public gateway (gets are
                // unauthenticated; an empty JWT is fine).
                let gateway = std::env::var("PINATA_GATEWAY").unwrap_or_default();
                PinataDriver::new("", gateway).get(cid).await
            }
        };
        fetched.map_err(|e| {
            DashboardError::ServiceUnavailable(format!("ipfs gateway fetch failed for {cid}: {e}"))
        })?
    } else if let Some(agent_id) = listing.content_uri.strip_prefix("xvn://strategy/") {
        let strategy = strategy::get(&state.api_context(), agent_id).await?;
        let value = serde_json::to_value(&strategy)
            .map_err(|e| DashboardError::Internal(anyhow::anyhow!("serialize strategy: {e}")))?;
        canonical_json(&value).into_bytes()
    } else {
        return Err(DashboardError::ServiceUnavailable(format!(
            "unsupported content_uri scheme for listing {}: {}",
            listing.listing_id, listing.content_uri
        )));
    };

    let canonical = String::from_utf8(bytes).map_err(|_| {
        DashboardError::Conflict(format!(
            "bundle integrity check failed for listing {}: fetched bytes are not UTF-8",
            listing.listing_id
        ))
    })?;
    let fetched_hash = manifest_hash_hex(&canonical);
    let onchain_hash = listing.content_hash.trim_start_matches("0x").to_lowercase();
    if fetched_hash != onchain_hash {
        return Err(DashboardError::Conflict(format!(
            "bundle integrity check failed for listing {}: content hash mismatch \
             (on-chain {onchain_hash}, fetched bytes hash to {fetched_hash})",
            listing.listing_id
        )));
    }
    serde_json::from_str(&canonical).map_err(|e| {
        DashboardError::Conflict(format!(
            "bundle for listing {} verified but is not parseable JSON: {e}",
            listing.listing_id
        ))
    })
}

/// Fetches the RAW bytes behind a listing's `content_uri` without any
/// plaintext-hash verification — used for sealed bundles whose pinned blob is
/// the opaque ciphertext (the on-chain `content_hash` commits to the canonical
/// PLAINTEXT, not the ciphertext, so a plaintext-hash check here would always
/// fail). Only the `ipfs://` scheme is supported for sealed bundles: a sealed
/// blob must never be resolvable from a local `xvn://` plaintext store.
async fn fetch_sealed_ciphertext(listing: &IndexedListing) -> Result<String, DashboardError> {
    let cid = listing.content_uri.strip_prefix("ipfs://").ok_or_else(|| {
        DashboardError::ServiceUnavailable(format!(
            "sealed listing {} has a non-ipfs content_uri ({}); sealed bundles must be pinned",
            listing.listing_id, listing.content_uri
        ))
    })?;
    let gateway = std::env::var("PINATA_GATEWAY").unwrap_or_default();
    let jwt = std::env::var("PINATA_JWT").unwrap_or_default();
    let ipfs = PinataDriver::new(jwt, gateway);
    let bytes = ipfs.get(cid).await.map_err(|e| {
        DashboardError::ServiceUnavailable(format!("ipfs gateway fetch failed for {cid}: {e}"))
    })?;
    String::from_utf8(bytes).map_err(|_| {
        DashboardError::Conflict(format!(
            "sealed bundle for listing {} is not UTF-8 ciphertext",
            listing.listing_id
        ))
    })
}

/// `GET /api/marketplace/listings/:id/bundle` — 404 unknown listing, 409
/// integrity mismatch, 503 unreachable gateway / unsupported scheme.
///
/// - OPEN tier (`tier == 0`): 200 `{listing_id, content_uri, verified: true,
///   manifest}` — the plaintext manifest, hash-verified.
/// - SEALED tier (`tier == 1`): 200 `{listing_id, content_uri, encrypted:
///   true, ciphertext, content_hash}` — the opaque blob for the browser to
///   decrypt via Lit; NO plaintext manifest is returned.
pub async fn get_bundle(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BundleResponse>, DashboardError> {
    let listing = {
        let snap = state.marketplace_snapshot.read().await;
        resolve_listing(&snap.listings, &id)
            .cloned()
            .ok_or_else(|| DashboardError::NotFound(format!("listing {id} not in indexed snapshot")))?
    };

    let listing_id = listing.listing_id;
    if listing.tier == 1 {
        // Sealed: return the ciphertext + the on-chain plaintext content_hash;
        // never the manifest. The browser decrypts and self-verifies.
        let ciphertext = fetch_sealed_ciphertext(&listing).await?;
        return Ok(Json(BundleResponse::Sealed(SealedBundleOut {
            listing_id,
            content_uri: listing.content_uri,
            encrypted: true,
            ciphertext,
            content_hash: listing.content_hash.trim_start_matches("0x").to_lowercase(),
        })));
    }

    let manifest = fetch_verified_manifest(&state, &listing).await?;
    Ok(Json(BundleResponse::Open(BundleOut {
        listing_id,
        content_uri: listing.content_uri,
        verified: true,
        manifest,
    })))
}

// ---------------------------------------------------------------------------
// GET /api/marketplace/listings/:id/attestations
// ---------------------------------------------------------------------------

/// One eval attestation, decoded from the `EvalAttestationRegistry`.
#[derive(Debug, Serialize, PartialEq)]
pub struct AttestationOut {
    /// Lowercase `0x…` attester address.
    pub attester: String,
    pub posted_at_unix: u64,
    pub eval_result_uri: String,
    /// `0x` + 64-hex keccak256 of the eval payload.
    pub eval_result_hash: String,
    /// `0x` + 64-hex schema id (`0x00…00` for the v1 cycles/sharpe payload).
    pub schema: String,
}

#[derive(Debug, Serialize)]
pub struct AttestationsOut {
    pub items: Vec<AttestationOut>,
}

/// Pure mapping from the on-chain struct to the wire shape.
fn attestation_out(a: &IEvalAttestationRegistry::Attestation) -> AttestationOut {
    AttestationOut {
        attester: format!("{:#x}", a.attester),
        posted_at_unix: a.postedAt,
        eval_result_uri: a.evalResultURI.clone(),
        eval_result_hash: format!("0x{:x}", a.evalResultHash),
        schema: format!("0x{:x}", a.schema),
    }
}

/// `GET /api/marketplace/listings/:id/attestations` — read the listing's
/// eval attestations live from the `EvalAttestationRegistry` over the
/// read-only provider. 404 unknown listing; 503 when the chain env or
/// `XVN_EVAL_ATTESTATION` is dormant; 200 `{items: […]}`.
pub async fn get_attestations(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AttestationsOut>, DashboardError> {
    // a. Listing must be indexed (404 unknown); resolve ULID-first, then int.
    let listing_id = {
        let snap = state.marketplace_snapshot.read().await;
        resolve_listing(&snap.listings, &id)
            .map(|l| l.listing_id)
            .ok_or_else(|| DashboardError::NotFound(format!("listing {id} not in indexed snapshot")))?
    };

    // b. Same read-only provider config as the indexer (startup-resolved;
    //    see `chain_config`); dormant → 503.
    let cfg = state
        .marketplace_chain()
        .and_then(|c| c.indexer.as_ref())
        .ok_or_else(|| {
            DashboardError::ServiceUnavailable(
                "marketplace chain access not configured: set XVN_RPC_URL, XVN_LISTING_REGISTRY, \
                 XVN_IDENTITY_REGISTRY"
                    .into(),
            )
        })?;
    let registry_addr = cfg.eval_attestation.ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "attestation registry not configured: set XVN_EVAL_ATTESTATION".into(),
        )
    })?;

    let timeout = chain_call_timeout(state.marketplace_chain());
    let provider = with_chain_timeout(timeout, ProviderBuilder::new().connect(cfg.rpc_url.as_str()))
        .await?
        .map_err(|e| DashboardError::ServiceUnavailable(format!("rpc connect failed: {e}")))?;
    let attestations = with_chain_timeout(
        timeout,
        IEvalAttestationRegistry::new(registry_addr, &provider)
            .getAttestations(U256::from(listing_id))
            .call(),
    )
    .await?
    .map_err(|e| DashboardError::ServiceUnavailable(format!("attestation lookup failed: {e}")))?;

    Ok(Json(AttestationsOut {
        items: attestations.iter().map(attestation_out).collect(),
    }))
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
pub fn wallet_view(snapshot: &MarketplaceSnapshot, address: &str, ownership: &OwnershipFacts) -> WalletView {
    let address_lc = address.to_lowercase();

    // Owned strategies: one entry per unique owned token, metadata from the
    // first listing referencing it.
    let mut seen_tokens = HashSet::new();
    let mut strategies = Vec::new();
    for l in &snapshot.listings {
        if !ownership.owned_token_ids.contains(&l.agent_nft_id) || !seen_tokens.insert(l.agent_nft_id.clone())
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
    s.len() == 42 && s.starts_with("0x") && s.as_bytes()[2..].iter().all(u8::is_ascii_hexdigit)
}

/// Gathers [`OwnershipFacts`] from the chain. Degrades, never errors: a
/// failed provider connect or a reverted call (e.g. `ownerOf` on a burned
/// token) is treated as not-owned / zero-balance. `license_token` is the
/// startup-resolved `XVN_LICENSE_TOKEN` address; `None` → license lookups
/// are skipped (empty `licenses`), never an error.
async fn fetch_ownership_facts(
    cfg: &IndexerCfg,
    snapshot: &MarketplaceSnapshot,
    address: Address,
    license_token: Option<Address>,
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

    if let Some(license_token) = license_token {
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

    // The indexer-active flag implies the config resolved at startup; a
    // missing config here degrades to empty facts rather than erroring.
    let mp = state.marketplace_chain();
    let facts = match (mp.and_then(|c| c.indexer.as_ref()), address.parse::<Address>()) {
        (Some(cfg), Ok(addr)) => {
            // One deadline bounds the whole ownership gather (xvision-4fp).
            // This route degrades on chain failures by design, so a timeout
            // degrades to empty facts too (logged) instead of erroring.
            match with_chain_timeout(
                chain_call_timeout(mp),
                fetch_ownership_facts(cfg, &snapshot, addr, mp.and_then(|c| c.license_token)),
            )
            .await
            {
                Ok(facts) => facts,
                Err(e) => {
                    tracing::warn!(error = %e, "wallet view: ownership gather timed out; returning empty ownership facts");
                    OwnershipFacts::default()
                }
            }
        }
        _ => OwnershipFacts::default(),
    };

    Ok(Json(wallet_view(&snapshot, &address, &facts)))
}

// ---------------------------------------------------------------------------
// GET /api/marketplace/receipts/:tx_hash
// ---------------------------------------------------------------------------

/// A purchase receipt: the decoded `Sold` event joined with listing metadata
/// from the indexer snapshot.
#[derive(Debug, Serialize)]
pub struct ReceiptOut {
    pub tx_hash: String,
    pub listing_id: u64,
    /// From the snapshot join; `""` when the listing is not (yet) indexed.
    pub agent_id: String,
    pub gen_art_seed: String,
    pub name: String,
    /// The listing's `content_uri` (`ipfs://CID` or `xvn://strategy/<ulid>`)
    /// from the snapshot join; `""` when the listing is not (yet) indexed.
    /// Lets the receipt page surface the bundle CID without a second fetch.
    pub content_uri: String,
    /// Lowercase `0x…`.
    pub buyer: String,
    pub price_usdc: f64,
    pub seller_proceeds_usdc: f64,
    pub protocol_proceeds_usdc: f64,
    /// Decimal string (== listing_id by spec, but read from the event).
    pub license_token_id: String,
    pub purchase_path: u8,
    /// Unix seconds of the containing block; `0` when the block lookup
    /// fails/degrades (we prefer a partial receipt over a 500).
    pub block_time_unix: i64,
}

/// Validates a `0x` + 64-hex-char transaction hash (any case).
fn is_tx_hash(s: &str) -> bool {
    s.len() == 66 && s.starts_with("0x") && s.as_bytes()[2..].iter().all(u8::is_ascii_hexdigit)
}

/// Pure join of a decoded `Sold` event with the snapshot's listing metadata.
/// Missing listing → empty-string defaults (the chain facts still stand).
fn receipt_from_sold(
    tx_hash: &str,
    sold: &IMarketplace::Sold,
    snapshot: &MarketplaceSnapshot,
    block_time_unix: i64,
) -> ReceiptOut {
    let listing_id: u64 = sold.listingId.try_into().unwrap_or(u64::MAX);
    let listing = snapshot.listings.iter().find(|l| l.listing_id == listing_id);
    ReceiptOut {
        tx_hash: tx_hash.to_lowercase(),
        listing_id,
        agent_id: listing.map(|l| l.agent_id.clone()).unwrap_or_default(),
        gen_art_seed: listing.map(|l| l.gen_art_seed.clone()).unwrap_or_default(),
        name: listing.map(|l| l.name.clone()).unwrap_or_default(),
        content_uri: listing.map(|l| l.content_uri.clone()).unwrap_or_default(),
        buyer: format!("{:#x}", sold.buyer),
        price_usdc: crate::marketplace_index::usdc6_to_f64(sold.priceUSDC.to::<u128>()),
        seller_proceeds_usdc: crate::marketplace_index::usdc6_to_f64(sold.sellerProceeds.to::<u128>()),
        protocol_proceeds_usdc: crate::marketplace_index::usdc6_to_f64(sold.protocolProceeds.to::<u128>()),
        license_token_id: sold.licenseTokenId.to_string(),
        purchase_path: sold.purchasePath,
        block_time_unix,
    }
}

/// `GET /api/marketplace/receipts/:tx_hash` — fetch the tx receipt over the
/// read-only provider, decode the `Sold` event, join listing metadata from
/// the snapshot. 400 bad hash; 503 chain env dormant; 404 unknown tx or no
/// `Sold` log in the receipt.
pub async fn get_receipt(
    State(state): State<AppState>,
    Path(tx_hash): Path<String>,
) -> Result<Json<ReceiptOut>, DashboardError> {
    if !is_tx_hash(&tx_hash) {
        return Err(DashboardError::Validation {
            field: "tx_hash".into(),
            msg: "must be a 0x-prefixed 64-hex-char transaction hash".into(),
        });
    }
    // Same read-only provider config as the indexer (startup-resolved;
    // see `chain_config`); dormant → 503.
    let cfg = state
        .marketplace_chain()
        .and_then(|c| c.indexer.as_ref())
        .ok_or_else(|| {
            DashboardError::ServiceUnavailable(
                "marketplace chain access not configured: set XVN_RPC_URL, XVN_LISTING_REGISTRY, \
                 XVN_IDENTITY_REGISTRY"
                    .into(),
            )
        })?;
    let hash: B256 = tx_hash.parse().map_err(|_| DashboardError::Validation {
        field: "tx_hash".into(),
        msg: "must be a 0x-prefixed 64-hex-char transaction hash".into(),
    })?;

    let timeout = chain_call_timeout(state.marketplace_chain());
    let provider = with_chain_timeout(timeout, ProviderBuilder::new().connect(cfg.rpc_url.as_str()))
        .await?
        .map_err(|e| DashboardError::ServiceUnavailable(format!("rpc connect failed: {e}")))?;

    let receipt = with_chain_timeout(timeout, provider.get_transaction_receipt(hash))
        .await?
        .map_err(|e| DashboardError::ServiceUnavailable(format!("rpc receipt lookup failed: {e}")))?
        .ok_or_else(|| DashboardError::NotFound(format!("transaction {tx_hash} not found on chain")))?;

    // Decode the Sold event: topic0 match on the typed SIGNATURE_HASH, then a
    // full typed decode (Sold carries the proceeds/licenseTokenId in data, so
    // unlike the adapter's ListingCreated topic-only read we need decode_log).
    let sold = receipt
        .inner
        .logs()
        .iter()
        .find_map(|log| {
            (log.topics().first() == Some(&IMarketplace::Sold::SIGNATURE_HASH))
                .then(|| IMarketplace::Sold::decode_log(&log.inner).ok())
                .flatten()
        })
        .ok_or_else(|| {
            DashboardError::NotFound(format!(
                "transaction {tx_hash} has no Sold event (not a purchase)"
            ))
        })?;

    // Block timestamp: one extra lookup; degrade to 0 rather than failing the
    // whole receipt on a flaky (or hung — same deadline as every chain call)
    // block fetch.
    let block_time_unix = match receipt.block_hash {
        Some(bh) => with_chain_timeout(timeout, provider.get_block_by_hash(bh))
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten()
            .map(|b| b.header.timestamp as i64)
            .unwrap_or(0),
        None => 0,
    };

    let snapshot = state.marketplace_snapshot.read().await;
    Ok(Json(receipt_from_sold(
        &tx_hash,
        &sold.data,
        &snapshot,
        block_time_unix,
    )))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BOB: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn resolve_public_gateway_precedence() {
        // env override wins, trailing slash trimmed
        assert_eq!(
            resolve_public_gateway(Some("https://ipfs.me/".into()), Some("https://b.example".into())),
            "https://ipfs.me"
        );
        // blank/whitespace override is treated as unset → falls through to backend
        assert_eq!(
            resolve_public_gateway(Some("  ".into()), Some("https://b.example/".into())),
            "https://b.example"
        );
        // neither set → vendor-neutral default
        assert_eq!(resolve_public_gateway(None, None), DEFAULT_PUBLIC_GATEWAY);
        assert_eq!(
            resolve_public_gateway(Some(String::new()), Some(String::new())),
            DEFAULT_PUBLIC_GATEWAY
        );
    }

    #[test]
    fn network_out_maps_chain_id_to_slug_and_explorer() {
        // Mantle mainnet (5000) → "mantle" + mainnet explorer.
        let n = network_out(Some(5000)).expect("configured mainnet → Some");
        assert_eq!(n.chain_id, 5000);
        assert_eq!(n.network, "mantle");
        assert_eq!(n.explorer_base.as_deref(), Some("https://explorer.mantle.xyz"));

        // Mantle Sepolia (5003) → "mantle-sepolia" + testnet explorer.
        let n = network_out(Some(5003)).expect("configured testnet → Some");
        assert_eq!(n.chain_id, 5003);
        assert_eq!(n.network, "mantle-sepolia");
        assert_eq!(
            n.explorer_base.as_deref(),
            Some("https://explorer.sepolia.mantle.xyz")
        );

        // No chain configured (no XVN_CHAIN_ID / no signer) → None, so the
        // frontend falls back to its build-time default.
        assert!(network_out(None).is_none());
    }

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
            units_sold_agents: 0,
            earned_usdc: 0.0,
            return30d_pct: None,
            sharpe: None,
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
    fn tx_hash_validation() {
        assert!(is_tx_hash(&format!("0x{}", "ab".repeat(32))));
        assert!(is_tx_hash(&format!("0x{}", "AB".repeat(32)))); // any case
        assert!(!is_tx_hash("0x1234")); // too short
        assert!(!is_tx_hash(&format!("1x{}", "ab".repeat(32)))); // bad prefix
        assert!(!is_tx_hash(&format!("0x{}gg", "ab".repeat(31)))); // non-hex
    }

    #[test]
    fn receipt_join_with_and_without_indexed_listing() {
        use alloy::primitives::aliases::U96;
        let sold = IMarketplace::Sold {
            listingId: U256::from(1u64),
            agentNftId: U256::from(7u64),
            buyer: ALICE.parse().unwrap(),
            priceUSDC: U96::from(49_000_000u64),
            sellerProceeds: U96::from(46_550_000u64),
            protocolProceeds: U96::from(2_450_000u64),
            licenseTokenId: U256::from(1u64),
            payerKind: 0,
            purchasePath: 1,
        };
        let tx = format!("0x{}", "AB".repeat(32));

        // Listing indexed → metadata joined.
        let snap = snapshot(vec![listing(1, "7", BOB, false)]);
        let out = receipt_from_sold(&tx, &sold, &snap, 1_700_000_123);
        assert_eq!(out.tx_hash, tx.to_lowercase());
        assert_eq!(out.listing_id, 1);
        assert_eq!(out.agent_id, "agent-7");
        assert_eq!(out.name, "xvn strategy 7");
        assert_eq!(out.content_uri, "xvn://strategy/agent-7");
        assert_eq!(out.buyer, ALICE);
        assert_eq!(out.price_usdc, 49.0);
        assert_eq!(out.seller_proceeds_usdc, 46.55);
        assert_eq!(out.protocol_proceeds_usdc, 2.45);
        assert_eq!(out.license_token_id, "1");
        assert_eq!(out.purchase_path, 1);
        assert_eq!(out.block_time_unix, 1_700_000_123);

        // Listing not indexed → chain facts stand, metadata defaults empty.
        let out = receipt_from_sold(&tx, &sold, &snapshot(vec![]), 0);
        assert_eq!(out.listing_id, 1);
        assert_eq!(out.agent_id, "");
        assert_eq!(out.gen_art_seed, "");
        assert_eq!(out.name, "");
        assert_eq!(out.content_uri, "");
        assert_eq!(out.block_time_unix, 0);
    }

    #[test]
    fn attestation_maps_to_wire_shape() {
        let hash = alloy::primitives::keccak256(r#"{"cycles":20,"sharpe":1.5}"#.as_bytes());
        let a = IEvalAttestationRegistry::Attestation {
            evalResultHash: hash,
            evalResultURI: "xvn://eval/listing/2".to_string(),
            attester: "0xAaAaAAaaaAAAAAaAaaaAAAaaAaaaAAaaAaAaaaaA".parse().unwrap(),
            postedAt: 1_700_000_777,
            schema: B256::ZERO,
        };
        let out = attestation_out(&a);
        assert_eq!(out.attester, ALICE, "lowercase 0x address");
        assert_eq!(out.posted_at_unix, 1_700_000_777);
        assert_eq!(out.eval_result_uri, "xvn://eval/listing/2");
        assert_eq!(out.eval_result_hash, format!("0x{hash:x}"));
        assert_eq!(out.schema, format!("0x{}", "00".repeat(32)));
    }

    #[test]
    fn eth_address_validation() {
        assert!(is_eth_address(ALICE));
        assert!(is_eth_address("0xAbCdEf0123456789abcdef0123456789ABCDEF01"));
        assert!(!is_eth_address("0x123")); // too short
        assert!(!is_eth_address(&format!("1x{}", "a".repeat(40)))); // bad prefix
        assert!(!is_eth_address(&format!("0x{}g", "a".repeat(39)))); // non-hex
    }

    // -- resolve_listing (Part A: ULID-first, int-fallback) -------------------

    fn listing_with_agent(listing_id: u64, agent_id: &str) -> IndexedListing {
        IndexedListing {
            listing_id,
            agent_nft_id: listing_id.to_string(),
            agent_id: agent_id.to_string(),
            seller: ALICE.to_string(),
            content_hash: "ab".repeat(32),
            content_uri: format!("xvn://strategy/{agent_id}"),
            tier: 0,
            price_usdc: 49.0,
            transferable_license: false,
            revoked: false,
            gen_art_seed: format!("{agent_id}:{}", "ab".repeat(32)),
            name: format!("Strategy {listing_id}"),
            symmetry: "Radial".into(),
            palette: "Ember".into(),
            attestation_count: 0,
            units_sold: 0,
            units_sold_agents: 0,
            earned_usdc: 0.0,
            return30d_pct: None,
            sharpe: None,
        }
    }

    #[test]
    fn resolve_listing_by_agent_id_ulid() {
        let l = listing_with_agent(1, "01HXAGENT000000000000000000");
        let listings = vec![l];
        let found = resolve_listing(&listings, "01HXAGENT000000000000000000");
        assert!(found.is_some());
        assert_eq!(found.unwrap().listing_id, 1);
    }

    #[test]
    fn resolve_listing_by_numeric_listing_id() {
        let l = listing_with_agent(42, "01HXAGENT000000000000000000");
        let listings = vec![l];
        let found = resolve_listing(&listings, "42");
        assert!(found.is_some());
        assert_eq!(found.unwrap().listing_id, 42);
    }

    #[test]
    fn resolve_listing_ulid_wins_over_numeric_when_both_possible() {
        // Agent_id looks like a ULID; another listing has listing_id 1.
        let l1 = listing_with_agent(1, "not-this-one");
        let l2 = listing_with_agent(2, "01HXAGENT000000000000000000");
        let listings = vec![l1, l2];
        // A ULID that matches agent_id on l2 — ULID-first must win.
        let found = resolve_listing(&listings, "01HXAGENT000000000000000000");
        assert!(found.is_some());
        assert_eq!(found.unwrap().listing_id, 2);
    }

    #[test]
    fn resolve_listing_returns_none_for_unknown() {
        let l = listing_with_agent(1, "01HXAGENT000000000000000000");
        let listings = vec![l];
        assert!(resolve_listing(&listings, "99").is_none());
        assert!(resolve_listing(&listings, "no-such-ulid").is_none());
    }

    #[test]
    fn resolve_listing_empty_agent_id_numeric_fallback() {
        // Listing with empty agent_id — should still resolve by numeric id.
        let l = listing_with_agent(7, "");
        let listings = vec![l];
        let found = resolve_listing(&listings, "7");
        assert!(found.is_some());
        assert_eq!(found.unwrap().listing_id, 7);
    }
}

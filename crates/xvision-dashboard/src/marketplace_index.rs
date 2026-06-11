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
        let identity_registry: Address =
            std::env::var("XVN_IDENTITY_REGISTRY").ok()?.parse().ok()?;
        Some(Self {
            rpc_url,
            listing_registry,
            identity_registry,
        })
    }
}

// ---------------------------------------------------------------------------
// tokenURI metadata decoding (pure)
// ---------------------------------------------------------------------------

/// Fields extracted from a genart tokenURI's metadata JSON. All fields default
/// to `""` on any decode failure — the indexer never drops a listing over bad
/// metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TokenMetadata {
    pub name: String,
    pub agent_id: String,
    pub symmetry: String,
    pub palette: String,
}

/// Lenient mirror of the genart metadata JSON
/// (`generate_token_uri` output: `{name, image, agent_id, attributes}`).
#[derive(serde::Deserialize)]
struct RawMetadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    attributes: Vec<RawAttribute>,
}

#[derive(serde::Deserialize)]
struct RawAttribute {
    #[serde(default)]
    trait_type: String,
    /// String OR number on the wire (Density/Layers are numeric).
    #[serde(default)]
    value: serde_json::Value,
}

const DATA_URI_PREFIX: &str = "data:application/json;base64,";

/// Decodes a `data:application/json;base64,…` tokenURI into [`TokenMetadata`].
///
/// Total function: any failure (wrong prefix, bad base64, non-JSON payload,
/// wrong shape) returns the all-empty default. Never panics, never errors.
pub(crate) fn decode_token_metadata(token_uri: &str) -> TokenMetadata {
    let Some(b64) = token_uri.strip_prefix(DATA_URI_PREFIX) else {
        return TokenMetadata::default();
    };
    let Some(bytes) = base64_decode(b64) else {
        return TokenMetadata::default();
    };
    let Ok(raw) = serde_json::from_slice::<RawMetadata>(&bytes) else {
        return TokenMetadata::default();
    };

    let mut symmetry = String::new();
    let mut palette = String::new();
    for attr in &raw.attributes {
        let value = stringify_attribute(&attr.value);
        match attr.trait_type.as_str() {
            "Symmetry" => symmetry = value,
            "Palette" => palette = value,
            _ => {}
        }
    }

    TokenMetadata {
        name: raw.name,
        agent_id: raw.agent_id,
        symmetry,
        palette,
    }
}

/// Stringifies an attribute `value` that may be a JSON string or number.
fn stringify_attribute(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Minimal standard-alphabet base64 decoder. Lenient about padding: trailing
/// `=`/`==` is stripped and unpadded 2- or 3-char trailing groups decode
/// fine; invalid characters and impossible lengths (trailing group of 1 char)
/// are rejected. Local because the dashboard crate has no base64 dep and
/// `xvision_identity`'s encoder is private.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes = s.as_bytes();
    let body = match bytes {
        [head @ .., b'=', b'='] => head,
        [head @ .., b'='] => head,
        _ => bytes,
    };
    // 6n bits must cover whole bytes: trailing group of 1 char is impossible.
    if body.len() % 4 == 1 {
        return None;
    }

    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    for chunk in body.chunks(4) {
        let mut acc: u32 = 0;
        for &c in chunk {
            acc = (acc << 6) | val(c)?;
        }
        // Left-align the 6·len bits in a 24-bit window.
        acc <<= 24 - 6 * chunk.len();
        out.push((acc >> 16) as u8);
        if chunk.len() >= 3 {
            out.push((acc >> 8) as u8);
        }
        if chunk.len() == 4 {
            out.push(acc as u8);
        }
    }
    Some(out)
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

    // -- decode_token_metadata -------------------------------------------

    #[test]
    fn decode_round_trips_real_generate_token_uri_output() {
        let uri = xvision_identity::generate_token_uri("01HXTESTAGENT", &"ab".repeat(32))
            .expect("generate_token_uri");
        let meta = decode_token_metadata(&uri);

        assert_eq!(meta.agent_id, "01HXTESTAGENT");
        assert_eq!(meta.name, "xvn strategy 01HXTEST");
        assert!(!meta.symmetry.is_empty(), "Symmetry attribute must decode");
        assert!(!meta.palette.is_empty(), "Palette attribute must decode");
    }

    #[test]
    fn decode_empty_string_is_default() {
        assert_eq!(decode_token_metadata(""), TokenMetadata::default());
    }

    #[test]
    fn decode_non_data_uri_is_default() {
        assert_eq!(decode_token_metadata("https://x"), TokenMetadata::default());
    }

    #[test]
    fn decode_bad_base64_is_default() {
        let uri = format!("{DATA_URI_PREFIX}!!!not-base64!!!");
        assert_eq!(decode_token_metadata(&uri), TokenMetadata::default());
    }

    #[test]
    fn decode_valid_base64_non_json_is_default() {
        // "hello world" — valid base64, not JSON.
        let uri = format!("{DATA_URI_PREFIX}aGVsbG8gd29ybGQ=");
        assert_eq!(decode_token_metadata(&uri), TokenMetadata::default());
    }

    #[test]
    fn decode_numeric_attribute_values_are_stringified() {
        let json = r#"{"name":"n","agent_id":"a","attributes":[
            {"trait_type":"Symmetry","value":42},
            {"trait_type":"Palette","value":"Ember"}
        ]}"#;
        let uri = format!("{DATA_URI_PREFIX}{}", b64(json.as_bytes()));
        let meta = decode_token_metadata(&uri);
        assert_eq!(meta.symmetry, "42");
        assert_eq!(meta.palette, "Ember");
    }

    /// Test-only encoder so malformed-payload tests don't depend on the
    /// private encoder in xvision_identity.
    fn b64(data: &[u8]) -> String {
        const CHARS: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let mut acc: u32 = 0;
            for (i, &b) in chunk.iter().enumerate() {
                acc |= u32::from(b) << (16 - 8 * i);
            }
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(CHARS[((acc >> (18 - 6 * i)) & 0x3f) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    #[test]
    fn base64_decode_round_trip() {
        for input in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(
                base64_decode(&b64(input)).as_deref(),
                Some(input),
                "round trip failed for {input:?}"
            );
        }
    }

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

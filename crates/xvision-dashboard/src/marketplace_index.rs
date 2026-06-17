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
//! - a failed `getAttestationCount(id)` carries the last-known count
//!   forward (the spawn loop keeps a per-listing cache and overlays it via
//!   [`carry_forward_attestations`]) so a transient RPC failure never
//!   flickers an "attested" badge off; a listing never read successfully
//!   stays at 0;
//! - a failed poll keeps the previous snapshot's listings and surfaces the
//!   error in `last_error`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use anyhow::Context;
use sqlx::SqlitePool;

use crate::routes::publish_receipts::find_receipt;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_identity::client::IIdentityRegistry;
use xvision_identity::contracts::{IEvalAttestationRegistry, IListingRegistry, IMarketplace};
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
    /// Eval attestations posted for this listing (`0` when the
    /// `EvalAttestationRegistry` is unconfigured). A failed count call
    /// degrades to the last successfully read value for the listing (the
    /// spawn loop overlays it via [`carry_forward_attestations`]), or `0`
    /// if there has never been a successful read.
    pub attestation_count: u64,
    /// Licenses sold, accumulated by the incremental chunked `Sold` log
    /// scan (`0` when the marketplace contract is unconfigured or the
    /// scan cursor hasn't reached the sale yet).
    pub units_sold: u64,
    /// Of `units_sold`, purchases via the x402 autonomous-agent payment path.
    /// `units_sold - units_sold_agents` = direct (human) buyers.
    pub units_sold_agents: u64,
    /// Sum of `Sold.sellerProceeds` in whole USDC (`0.0` when dormant or
    /// not yet scanned).
    pub earned_usdc: f64,
    /// Best trailing-30d return % across completed eval runs for this
    /// agent (`None` when agent_id is empty or no completed runs exist).
    /// Part B: populated by `apply_perf` in the indexer spawn loop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return30d_pct: Option<f64>,
    /// Best Sharpe ratio across completed eval runs for this agent
    /// (`None` when agent_id is empty or no completed runs exist).
    /// Part B: populated by `apply_perf` in the indexer spawn loop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharpe: Option<f64>,
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
#[derive(Debug, Clone)]
pub struct IndexerCfg {
    pub rpc_url: String,
    pub listing_registry: Address,
    pub identity_registry: Address,
    /// Optional `EvalAttestationRegistry` address — enables per-listing
    /// `attestation_count`. `None` → counts stay 0.
    pub eval_attestation: Option<Address>,
    /// Optional `Marketplace` address — enables the incremental chunked
    /// `Sold` log scan that feeds `units_sold` / `earned_usdc`.
    /// `None` → both stay 0.
    pub marketplace: Option<Address>,
    /// Explicit lower bound for the `Sold` scan cursor
    /// (`XVN_MARKETPLACE_DEPLOY_BLOCK`). `None` (unset/unparseable) →
    /// the first tick starts at `latest - SOLD_SCAN_DEFAULT_LOOKBACK`,
    /// which covers recent history only; set the env for a full backfill.
    pub marketplace_deploy_block: Option<u64>,
}

/// Parses an optional env address value: unset or unparseable → `None`
/// (the enrichment it gates simply stays dormant, never an error).
fn parse_opt_addr(v: Option<String>) -> Option<Address> {
    v?.parse().ok()
}

/// Parses the deploy-block lower bound: unset or unparseable → `None`
/// (the scan then defaults to a recent-history lookback on first tick).
fn parse_deploy_block(v: Option<String>) -> Option<u64> {
    v.and_then(|s| s.parse().ok())
}

impl IndexerCfg {
    /// Reads `XVN_RPC_URL`, `XVN_LISTING_REGISTRY`, `XVN_IDENTITY_REGISTRY`.
    /// Returns `None` when any is missing or an address fails to parse —
    /// the indexer then stays dormant (mirrors `ChainEnv::from_env` in
    /// `routes/marketplace.rs`).
    ///
    /// `XVN_EVAL_ATTESTATION`, `XVN_MARKETPLACE_CONTRACT`, and
    /// `XVN_MARKETPLACE_DEPLOY_BLOCK` are OPTIONAL — their absence never
    /// turns the whole config into `None`; the trust/earnings enrichment
    /// just stays at zeros.
    pub fn from_env() -> Option<Self> {
        let rpc_url = std::env::var("XVN_RPC_URL").ok()?;
        let listing_registry: Address = std::env::var("XVN_LISTING_REGISTRY").ok()?.parse().ok()?;
        let identity_registry: Address = std::env::var("XVN_IDENTITY_REGISTRY").ok()?.parse().ok()?;
        Some(Self {
            rpc_url,
            listing_registry,
            identity_registry,
            eval_attestation: parse_opt_addr(std::env::var("XVN_EVAL_ATTESTATION").ok()),
            marketplace: parse_opt_addr(std::env::var("XVN_MARKETPLACE_CONTRACT").ok()),
            marketplace_deploy_block: parse_deploy_block(std::env::var("XVN_MARKETPLACE_DEPLOY_BLOCK").ok()),
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
// Sold-event scan: incremental chunked ledger
//
// The Mantle Sepolia RPC cannot be trusted with wide `eth_getLogs` ranges:
// very large ranges (~10M blocks) error at the gateway, and ranges above
// roughly ~100k blocks SILENTLY return an empty result set (verified
// 2026-06-11: a 216k-block range returned 0 logs even though a known Sold
// log exists inside it; 26k and 100-block ranges found it). A single-shot
// full-range scan can therefore never be trusted. Instead the indexer keeps
// a persistent in-memory ledger with a block cursor and scans forward in
// small chunks, never advancing the cursor past a failed chunk.
// ---------------------------------------------------------------------------

/// Max block span per `eth_getLogs` call — safely under the ~100k-block
/// threshold where the RPC starts silently dropping results.
pub(crate) const SOLD_SCAN_CHUNK: u64 = 9_000;

/// Max chunks scanned per tick (catch-up rate: up to 90k blocks / 30s).
pub(crate) const SOLD_SCAN_MAX_CHUNKS_PER_TICK: usize = 10;

/// First-tick lookback when `XVN_MARKETPLACE_DEPLOY_BLOCK` is unset:
/// covers recent history only. Set the env for a full backfill.
pub(crate) const SOLD_SCAN_DEFAULT_LOOKBACK: u64 = 100_000;

/// Persistent (per indexer task) accumulation of `Sold` events.
///
/// `cursor` is the next block to scan (`None` until initialized on the
/// first tick); `per_listing` holds the running totals for every Sold
/// event decoded so far. The ledger only ever grows — a chunk's logs are
/// folded in exactly once, when the cursor advances past it.
#[derive(Debug, Default)]
pub(crate) struct SalesLedger {
    pub cursor: Option<u64>,
    pub per_listing: HashMap<u64, SaleTotals>,
}

/// Initial scan cursor: the configured deploy block when set, otherwise
/// `latest - SOLD_SCAN_DEFAULT_LOOKBACK` (recent history only).
pub(crate) fn initial_cursor(deploy_block: Option<u64>, latest: u64) -> u64 {
    deploy_block.unwrap_or_else(|| latest.saturating_sub(SOLD_SCAN_DEFAULT_LOOKBACK))
}

/// Pure chunk math: inclusive `(from, to)` ranges covering
/// `cursor..=latest`, each at most `chunk` blocks wide, capped at
/// `max_chunks` ranges. Empty when `cursor > latest`.
pub(crate) fn chunk_ranges(cursor: u64, latest: u64, chunk: u64, max_chunks: usize) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let chunk = chunk.max(1);
    let mut from = cursor;
    while from <= latest && out.len() < max_chunks {
        let to = from.saturating_add(chunk - 1).min(latest);
        out.push((from, to));
        if to == u64::MAX {
            break;
        }
        from = to + 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Sold-event aggregation (pure)
// ---------------------------------------------------------------------------

/// Per-listing sale totals derived from decoded `Sold` events.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct SaleTotals {
    pub units_sold: u64,
    /// Of `units_sold`, those bought via the x402 autonomous-agent payment
    /// path (`Sold.purchasePath == 1`). `units_sold - units_sold_agents` are
    /// direct (human-wallet) purchases.
    pub units_sold_agents: u64,
    /// Sum of `sellerProceeds` in 6-decimal USDC units.
    pub seller_proceeds_usdc6: u128,
}

/// Folds decoded `Sold` events into existing per-listing totals: one unit
/// per event, seller proceeds summed (saturating — `uint96` sums cannot
/// realistically overflow u128, but never panic on hostile logs).
pub(crate) fn fold_sales<I>(totals: &mut HashMap<u64, SaleTotals>, events: I)
where
    I: IntoIterator<Item = IMarketplace::Sold>,
{
    for sold in events {
        let listing_id: u64 = sold.listingId.try_into().unwrap_or(u64::MAX);
        let t = totals.entry(listing_id).or_default();
        t.units_sold += 1;
        // purchasePath == 1 is the x402 autonomous-agent rail (0 = direct).
        if sold.purchasePath == 1 {
            t.units_sold_agents += 1;
        }
        t.seller_proceeds_usdc6 = t
            .seller_proceeds_usdc6
            .saturating_add(sold.sellerProceeds.to::<u128>());
    }
}

/// One-shot fold of decoded `Sold` events into fresh per-listing totals
/// (test convenience over [`fold_sales`]).
#[cfg(test)]
pub(crate) fn aggregate_sales<I>(events: I) -> HashMap<u64, SaleTotals>
where
    I: IntoIterator<Item = IMarketplace::Sold>,
{
    let mut totals = HashMap::new();
    fold_sales(&mut totals, events);
    totals
}

/// One bounded `eth_getLogs` scan for `Sold` events on the marketplace
/// contract (topic0 = the typed `SIGNATURE_HASH`), over the INCLUSIVE
/// `from..=to` block range. Callers must keep the range under the RPC's
/// silent-empty threshold (see [`SOLD_SCAN_CHUNK`]). Undecodable logs are
/// skipped; a failed call bubbles to the caller, which retries the same
/// chunk next tick.
async fn fetch_sold_events<P: Provider>(
    provider: &P,
    marketplace: Address,
    from: u64,
    to: u64,
) -> anyhow::Result<Vec<IMarketplace::Sold>> {
    let filter = Filter::new()
        .address(marketplace)
        .event_signature(IMarketplace::Sold::SIGNATURE_HASH)
        .from_block(from)
        .to_block(to);
    let logs = provider
        .get_logs(&filter)
        .await
        .with_context(|| format!("eth_getLogs(Sold) blocks {from}..={to}"))?;
    Ok(logs
        .iter()
        .filter_map(|log| IMarketplace::Sold::decode_log(&log.inner).ok().map(|l| l.data))
        .collect())
}

/// One tick of the incremental Sold scan: fetch the latest block, then scan
/// up to [`SOLD_SCAN_MAX_CHUNKS_PER_TICK`] chunks of [`SOLD_SCAN_CHUNK`]
/// blocks from the ledger cursor. Each successful chunk folds its logs into
/// the ledger and advances the cursor to `chunk_end + 1`; a failed chunk
/// stops the tick WITHOUT advancing, so the same range is retried next tick
/// (no data loss, no silent gaps). No-op when the marketplace contract is
/// unconfigured.
async fn advance_sales_ledger(cfg: &IndexerCfg, ledger: &mut SalesLedger) {
    let Some(marketplace) = cfg.marketplace else {
        return;
    };
    let provider = match ProviderBuilder::new().connect(cfg.rpc_url.as_str()).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "Sold scan: provider connect failed; cursor not advanced, retrying next tick");
            return;
        }
    };
    let latest = match provider.get_block_number().await {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(error = %e, "Sold scan: eth_blockNumber failed; cursor not advanced, retrying next tick");
            return;
        }
    };
    let cursor = *ledger
        .cursor
        .get_or_insert_with(|| initial_cursor(cfg.marketplace_deploy_block, latest));
    for (from, to) in chunk_ranges(cursor, latest, SOLD_SCAN_CHUNK, SOLD_SCAN_MAX_CHUNKS_PER_TICK) {
        match fetch_sold_events(&provider, marketplace, from, to).await {
            Ok(events) => {
                fold_sales(&mut ledger.per_listing, events);
                ledger.cursor = Some(to + 1);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    from_block = from,
                    to_block = to,
                    "Sold scan chunk failed; cursor stays at this chunk and retries next tick"
                );
                return;
            }
        }
    }
}

/// Overlays the ledger's accumulated sale totals onto a freshly built
/// snapshot's `units_sold` / `earned_usdc` (listings without sales keep
/// their zeros from `poll_once`).
pub(crate) fn apply_sales(snapshot: &mut MarketplaceSnapshot, ledger: &SalesLedger) {
    for listing in &mut snapshot.listings {
        if let Some(t) = ledger.per_listing.get(&listing.listing_id) {
            listing.units_sold = t.units_sold;
            listing.units_sold_agents = t.units_sold_agents;
            listing.earned_usdc = usdc6_to_f64(t.seller_proceeds_usdc6);
        }
    }
}

// ---------------------------------------------------------------------------
// Part B: real perf metrics from eval_runs
// ---------------------------------------------------------------------------

/// Raw perf row fetched from `eval_runs` for one agent.
#[derive(Debug, sqlx::FromRow)]
struct PerfRow {
    total_return_pct: Option<f64>,
    sharpe: Option<f64>,
}

/// Queries `eval_runs` for completed runs belonging to `agent_id` and
/// returns the best trailing-30d return % and best Sharpe across those runs.
///
/// Trailing-30d window: filters `started_at >= now - 30d` when any run
/// falls in that window; otherwise falls back to all completed runs so a
/// strategy with only older runs still gets a non-null value rather than
/// silently returning nothing.
///
/// Returns `None` for each metric when no completed runs exist or when the
/// DB query fails (the caller degrades gracefully to `None`). Skipped when
/// `agent_id` is empty.
pub(crate) async fn perf_for_agent(pool: &SqlitePool, agent_id: &str) -> (Option<f64>, Option<f64>) {
    if agent_id.is_empty() {
        return (None, None);
    }

    // Parse metrics_json to extract sharpe and total_return_pct.
    // We extract both fields directly via json_extract so we don't need
    // to deserialize the full MetricsSummary.
    let cutoff_30d = {
        let thirty_days_ago = chrono::Utc::now() - chrono::Duration::days(30);
        thirty_days_ago.to_rfc3339()
    };

    // Try trailing-30d window first.
    let rows_30d: Vec<PerfRow> = sqlx::query_as(
        "SELECT \
             json_extract(metrics_json, '$.total_return_pct') AS total_return_pct, \
             json_extract(metrics_json, '$.sharpe') AS sharpe \
         FROM eval_runs \
         WHERE agent_id = ? AND status = 'completed' AND started_at >= ? \
           AND metrics_json IS NOT NULL",
    )
    .bind(agent_id)
    .bind(&cutoff_30d)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Fall back to all completed runs when the 30d window is empty.
    let rows: Vec<PerfRow> = if rows_30d.is_empty() {
        sqlx::query_as(
            "SELECT \
                 json_extract(metrics_json, '$.total_return_pct') AS total_return_pct, \
                 json_extract(metrics_json, '$.sharpe') AS sharpe \
             FROM eval_runs \
             WHERE agent_id = ? AND status = 'completed' AND metrics_json IS NOT NULL",
        )
        .bind(agent_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    } else {
        rows_30d
    };

    let best_return = rows.iter().filter_map(|r| r.total_return_pct).reduce(f64::max);
    let best_sharpe = rows.iter().filter_map(|r| r.sharpe).reduce(f64::max);

    (best_return, best_sharpe)
}

/// Overlays real perf metrics onto listings that have a non-empty `agent_id`.
/// Listings with empty agent_id or no completed eval runs keep `None`.
/// DB failures per-listing degrade to `None` (never block the snapshot swap).
pub(crate) async fn apply_perf(snapshot: &mut MarketplaceSnapshot, pool: &SqlitePool) {
    for listing in &mut snapshot.listings {
        if listing.agent_id.is_empty() {
            continue;
        }
        let (return_pct, sharpe) = perf_for_agent(pool, &listing.agent_id).await;
        listing.return30d_pct = return_pct;
        listing.sharpe = sharpe;
    }
}

// ---------------------------------------------------------------------------
// Part C: resolve real display names for local listings
// ---------------------------------------------------------------------------

/// Resolves a real, human display name for each listing, keyed on the listing's
/// `agent_id` (always present from the on-chain token metadata) rather than the
/// `content_uri` scheme — so it covers BOTH open (`xvn://strategy/<ulid>`) and
/// sealed (`ipfs://CID`) listings, which previously kept their generic gen-art
/// name and rendered as "Strategy #N".
///
/// Precedence per listing:
///   1. the creator-chosen name on the local publish receipt (when non-empty);
///   2. else the local strategy manifest's `display_name` (when non-empty);
///   3. else leave the existing gen-art name (foreign / unknown strategies).
/// Map an `ipfs://CID[/path]` content URI to a full gateway URL by appending it
/// to `gateway` (a prefix the CID is joined onto, e.g.
/// `https://gateway.pinata.cloud/ipfs`). `None` for non-ipfs URIs or empty CIDs.
fn ipfs_gateway_url(gateway: &str, content_uri: &str) -> Option<String> {
    let cid = content_uri.strip_prefix("ipfs://")?.trim();
    if cid.is_empty() {
        return None;
    }
    Some(format!("{}/{}", gateway.trim_end_matches('/'), cid))
}

/// Pull a non-empty `display_name` from a fetched manifest JSON, checking both
/// the top level and a nested `manifest` object. `None` if absent/blank/unparseable.
fn parse_manifest_display_name(json: &str) -> Option<String> {
    fn pick(v: &serde_json::Value) -> Option<String> {
        v.get("display_name")
            .and_then(|d| d.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    pick(&v).or_else(|| v.get("manifest").and_then(pick))
}

/// Thin HTTP wrapper: GET the gateway URL and extract the manifest display name.
/// Best-effort — any network/status/parse failure returns `None` so the caller
/// keeps the existing gen-art name.
async fn resolve_ipfs_name(client: &reqwest::Client, url: &str) -> Option<String> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().await.ok()?;
    parse_manifest_display_name(&body)
}

pub(crate) async fn enrich_local_names(
    snapshot: &mut MarketplaceSnapshot,
    xvn_home: &PathBuf,
    pool: &SqlitePool,
) {
    let store = FilesystemStore::new(strategy_store_dir(xvn_home));
    // Optional IPFS gateway for FOREIGN sealed listings (`ipfs://CID`) that have
    // no local strategy: set `XVN_IPFS_GATEWAY` to a gateway prefix the CID is
    // appended to (e.g. `https://gateway.pinata.cloud/ipfs`). Unset → such
    // listings keep their gen-art name (prior behavior).
    let ipfs_gateway = std::env::var("XVN_IPFS_GATEWAY")
        .ok()
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty());
    let http = ipfs_gateway.as_ref().and_then(|_| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(6))
            .build()
            .ok()
    });
    // Dedupe gateway fetches across listings that share a CID within one pass.
    let mut ipfs_name_cache: HashMap<String, Option<String>> = HashMap::new();

    for listing in &mut snapshot.listings {
        let agent_id = listing.agent_id.trim().to_string();
        if agent_id.is_empty() {
            continue;
        }

        // 1. Creator-chosen listing name from the publish receipt wins. A DB
        //    error or missing receipt simply falls through to the manifest.
        if let Ok(Some(receipt)) = find_receipt(pool, &agent_id).await {
            if let Some(name) = receipt.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                listing.name = name.to_owned();
                continue;
            }
        }

        // 2. Otherwise default to the local strategy's own display name.
        let mut resolved = false;
        match store.load(&agent_id).await {
            Ok(strategy) => {
                let display_name = strategy.manifest.display_name.trim().to_string();
                if !display_name.is_empty() {
                    listing.name = display_name;
                    resolved = true;
                }
            }
            Err(e) => {
                tracing::debug!(
                    listing_id = listing.listing_id,
                    agent_id = %agent_id,
                    error = %e,
                    "enrich_local_names: strategy load failed; trying ipfs gateway"
                );
            }
        }
        if resolved {
            continue;
        }

        // 3. Foreign sealed listing (no local strategy): resolve the name from
        //    the IPFS gateway when configured. Best-effort; failure keeps the
        //    gen-art name. Cached per CID within this pass.
        if let (Some(client), Some(url)) = (
            http.as_ref(),
            ipfs_gateway
                .as_deref()
                .and_then(|gw| ipfs_gateway_url(gw, &listing.content_uri)),
        ) {
            let name = match ipfs_name_cache.get(&url) {
                Some(cached) => cached.clone(),
                None => {
                    let fetched = resolve_ipfs_name(client, &url).await;
                    ipfs_name_cache.insert(url.clone(), fetched.clone());
                    fetched
                }
            };
            if let Some(name) = name {
                listing.name = name;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Attestation-count carry-forward
// ---------------------------------------------------------------------------

/// Overlays last-known attestation counts onto listings whose
/// `getAttestationCount` call failed this poll, and refreshes the cache
/// from the listings that were read successfully.
///
/// `failed` is the per-poll list of listing ids whose count lookup
/// errored (their `attestation_count` is 0 in the fresh snapshot);
/// `last_known` is the spawn loop's persistent per-listing cache. A
/// failed listing with no cached value keeps its honest 0.
pub(crate) fn carry_forward_attestations(
    snapshot: &mut MarketplaceSnapshot,
    failed: &[u64],
    last_known: &mut HashMap<u64, u64>,
) {
    for listing in &mut snapshot.listings {
        if failed.contains(&listing.listing_id) {
            if let Some(prev) = last_known.get(&listing.listing_id) {
                listing.attestation_count = *prev;
            }
        } else {
            last_known.insert(listing.listing_id, listing.attestation_count);
        }
    }
}

// ---------------------------------------------------------------------------
// Chain reader
// ---------------------------------------------------------------------------

/// Result of one [`poll_once`] pass: the fresh snapshot plus the listing
/// ids whose `getAttestationCount` lookup failed (their counts are 0 in
/// the snapshot and get overlaid by [`carry_forward_attestations`]).
pub struct PollOutcome {
    pub snapshot: MarketplaceSnapshot,
    pub attestation_failures: Vec<u64>,
}

/// One full read pass over the marketplace contracts (listings +
/// attestations only — `units_sold` / `earned_usdc` come out as zeros and
/// are overlaid afterwards via [`apply_sales`] from the spawn loop's
/// persistent [`SalesLedger`]).
///
/// Errors only on connection / `totalListings()` failure. Per-listing
/// failures degrade: a failed `getListing` skips the id (logged), a failed
/// `tokenURI` keeps the listing with empty metadata, and a failed
/// `getAttestationCount` records the id in `attestation_failures` so the
/// spawn loop can carry the last-known count forward.
pub async fn poll_once(cfg: &IndexerCfg) -> anyhow::Result<PollOutcome> {
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

    // Attestation counts: per-listing getAttestationCount when the
    // EvalAttestationRegistry is configured. Degrades to 0 on error.
    let attestation_registry = cfg
        .eval_attestation
        .map(|addr| IEvalAttestationRegistry::new(addr, &provider));

    let mut listings = Vec::with_capacity(total as usize);
    let mut attestation_failures: Vec<u64> = Vec::new();
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
        let listing_id = u64::try_from(listing.listingId).unwrap_or(id);

        let attestation_count = match &attestation_registry {
            Some(registry) => match registry.getAttestationCount(U256::from(id)).call().await {
                Ok(count) => count.try_into().unwrap_or(u64::MAX),
                Err(e) => {
                    tracing::warn!(
                        listing_id = id,
                        error = %e,
                        "getAttestationCount failed; carrying last-known count forward"
                    );
                    attestation_failures.push(listing_id);
                    0
                }
            },
            None => 0,
        };

        listings.push(IndexedListing {
            listing_id,
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
            attestation_count,
            // Overlaid by apply_sales() in the spawn loop.
            units_sold: 0,
            units_sold_agents: 0,
            earned_usdc: 0.0,
            // Overlaid by apply_perf() in the spawn loop (Part B).
            return30d_pct: None,
            sharpe: None,
        });
    }

    Ok(PollOutcome {
        snapshot: MarketplaceSnapshot {
            listings,
            last_poll_unix: chrono::Utc::now().timestamp(),
            last_error: None,
            total_onchain: total,
        },
        attestation_failures,
    })
}

// ---------------------------------------------------------------------------
// Background task
// ---------------------------------------------------------------------------

/// Per-tick deadline applied to the entire chain work block (ledger advance +
/// poll_once + overlay/swap). Must exceed the worst case of
/// SOLD_SCAN_MAX_CHUNKS_PER_TICK getLogs calls plus a full listing
/// enumeration. Generous because this deadline only guards against HUNG
/// sockets, not slow polls.
pub(crate) const TICK_DEADLINE: Duration = Duration::from_secs(120);

/// Records a timeout error on the shared snapshot without disturbing the
/// existing listings. Extracted so it can be called from the spawn loop and
/// covered by a unit test without spinning up an async runtime.
pub(crate) fn apply_tick_timeout(snapshot: &mut MarketplaceSnapshot) {
    snapshot.last_error = Some("indexer tick timed out after 120s".to_owned());
    snapshot.last_poll_unix = chrono::Utc::now().timestamp();
}

/// Spawns the 30s polling loop. First tick fires immediately. A successful
/// poll replaces the snapshot wholesale; a failed poll keeps the previous
/// listings and records `last_error` + the attempt time.
///
/// The loop owns the persistent [`SalesLedger`] and the per-listing
/// last-known attestation-count cache: each tick first advances the
/// incremental chunked Sold scan (cursor only moves past successfully
/// scanned chunks), then overlays last-known attestation counts onto
/// listings whose count lookup failed ([`carry_forward_attestations`])
/// and the accumulated sale totals ([`apply_sales`]) before swapping the
/// fresh snapshot in.
///
/// The entire per-tick chain work is wrapped in [`TICK_DEADLINE`]: a hung
/// socket can no longer stall the loop forever. On timeout the previous
/// listings are kept and `last_error` is set.
pub fn spawn_indexer(
    snapshot: SharedSnapshot,
    cfg: IndexerCfg,
    pool: SqlitePool,
    xvn_home: PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut ledger = SalesLedger::default();
        let mut attestation_counts: HashMap<u64, u64> = HashMap::new();
        loop {
            tick.tick().await;
            let work = async {
                advance_sales_ledger(&cfg, &mut ledger).await;
                match poll_once(&cfg).await {
                    Ok(outcome) => {
                        let PollOutcome {
                            snapshot: mut fresh,
                            attestation_failures,
                        } = outcome;
                        carry_forward_attestations(
                            &mut fresh,
                            &attestation_failures,
                            &mut attestation_counts,
                        );
                        apply_sales(&mut fresh, &ledger);
                        // Part B: enrich listings with real perf metrics from
                        // local eval_runs. Degrades per-listing to None on DB
                        // failure; never blocks the snapshot swap.
                        apply_perf(&mut fresh, &pool).await;
                        // Part C: resolve real display names for local listings,
                        // keyed on agent_id so both xvn:// and sealed ipfs://
                        // listings inherit the creator-chosen name (publish
                        // receipt) or the strategy's display_name.
                        enrich_local_names(&mut fresh, &xvn_home, &pool).await;
                        *snapshot.write().await = fresh;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "marketplace indexer poll failed; keeping previous snapshot");
                        let mut guard = snapshot.write().await;
                        guard.last_error = Some(e.to_string());
                        guard.last_poll_unix = chrono::Utc::now().timestamp();
                    }
                }
            };
            if tokio::time::timeout(TICK_DEADLINE, work).await.is_err() {
                tracing::warn!("marketplace indexer tick timed out after 120s; keeping previous snapshot");
                let mut guard = snapshot.write().await;
                apply_tick_timeout(&mut guard);
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

    // -- ipfs name resolution (ctkm.9) -------------------------------------

    #[test]
    fn ipfs_gateway_url_maps_cid() {
        assert_eq!(
            ipfs_gateway_url("https://gw/ipfs", "ipfs://bafyCID"),
            Some("https://gw/ipfs/bafyCID".to_string())
        );
        // trailing slash on the gateway is normalized; sub-paths survive.
        assert_eq!(
            ipfs_gateway_url("https://gw/ipfs/", "ipfs://bafyCID/manifest.json"),
            Some("https://gw/ipfs/bafyCID/manifest.json".to_string())
        );
        // non-ipfs (open listing) and empty CID → None (skip gateway).
        assert_eq!(ipfs_gateway_url("https://gw/ipfs", "xvn://strategy/01ABC"), None);
        assert_eq!(ipfs_gateway_url("https://gw/ipfs", "ipfs://"), None);
    }

    #[test]
    fn parse_manifest_display_name_variants() {
        assert_eq!(
            parse_manifest_display_name(r#"{"display_name":"Momentum Scalper"}"#).as_deref(),
            Some("Momentum Scalper")
        );
        // nested under a `manifest` object.
        assert_eq!(
            parse_manifest_display_name(r#"{"manifest":{"display_name":"Nested Name"}}"#).as_deref(),
            Some("Nested Name")
        );
        // blank / missing / non-JSON → None (caller keeps gen-art name).
        assert_eq!(parse_manifest_display_name(r#"{"display_name":"  "}"#), None);
        assert_eq!(parse_manifest_display_name(r#"{"other":"x"}"#), None);
        assert_eq!(parse_manifest_display_name("not json at all"), None);
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

    // -- optional cfg parsing (pure — from_env's required vars are owned by
    //    indexer_cfg_missing_env_is_none under the removal-only convention,
    //    so the Some path is exercised through these value-level helpers) --

    #[test]
    fn optional_addr_parsing() {
        assert_eq!(parse_opt_addr(None), None);
        assert_eq!(parse_opt_addr(Some("nope".into())), None);
        let addr = "0x1111111111111111111111111111111111111111";
        assert_eq!(parse_opt_addr(Some(addr.into())), Some(addr.parse().unwrap()));
    }

    #[test]
    fn deploy_block_parsing_defaults_none() {
        assert_eq!(parse_deploy_block(None), None);
        assert_eq!(parse_deploy_block(Some("not-a-number".into())), None);
        assert_eq!(parse_deploy_block(Some("12345678".into())), Some(12_345_678));
    }

    // -- chunked-scan cursor math ---------------------------------------------

    #[test]
    fn initial_cursor_prefers_deploy_block() {
        assert_eq!(initial_cursor(Some(42), 1_000_000), 42);
        // Unset → recent-history lookback.
        assert_eq!(
            initial_cursor(None, 1_000_000),
            1_000_000 - SOLD_SCAN_DEFAULT_LOOKBACK
        );
        // Saturates near genesis.
        assert_eq!(initial_cursor(None, 5), 0);
    }

    #[test]
    fn chunk_ranges_empty_when_cursor_past_latest() {
        assert!(chunk_ranges(101, 100, 9_000, 10).is_empty());
    }

    #[test]
    fn chunk_ranges_single_chunk_clamps_to_latest() {
        assert_eq!(chunk_ranges(100, 150, 9_000, 10), vec![(100, 150)]);
        // Exactly one block.
        assert_eq!(chunk_ranges(100, 100, 9_000, 10), vec![(100, 100)]);
    }

    #[test]
    fn chunk_ranges_splits_and_caps_at_max_chunks() {
        // 25 blocks, chunk = 10 → 3 chunks, last clamped to latest.
        assert_eq!(chunk_ranges(0, 24, 10, 10), vec![(0, 9), (10, 19), (20, 24)]);
        // Cap at max_chunks = 2.
        assert_eq!(chunk_ranges(0, 24, 10, 2), vec![(0, 9), (10, 19)]);
        // Ranges are contiguous and inclusive (no gap, no overlap).
        let ranges = chunk_ranges(7, 100_000, 9_000, 10);
        assert_eq!(ranges.len(), 10);
        assert_eq!(ranges[0], (7, 9_006));
        for w in ranges.windows(2) {
            assert_eq!(w[1].0, w[0].1 + 1);
        }
    }

    #[test]
    fn chunk_ranges_no_overflow_at_u64_max() {
        let ranges = chunk_ranges(u64::MAX - 5, u64::MAX, 9_000, 10);
        assert_eq!(ranges, vec![(u64::MAX - 5, u64::MAX)]);
    }

    // -- apply_sales merge ----------------------------------------------------

    fn listing_with_id(listing_id: u64) -> IndexedListing {
        IndexedListing {
            listing_id,
            agent_nft_id: "7".into(),
            agent_id: String::new(),
            seller: String::new(),
            content_hash: String::new(),
            content_uri: String::new(),
            tier: 0,
            price_usdc: 0.0,
            transferable_license: false,
            revoked: false,
            gen_art_seed: String::new(),
            name: String::new(),
            symmetry: String::new(),
            palette: String::new(),
            attestation_count: 0,
            units_sold: 0,
            units_sold_agents: 0,
            earned_usdc: 0.0,
            return30d_pct: None,
            sharpe: None,
        }
    }

    #[test]
    fn apply_sales_overlays_ledger_totals() {
        let mut snapshot = MarketplaceSnapshot {
            listings: vec![listing_with_id(2), listing_with_id(5)],
            ..Default::default()
        };
        let mut ledger = SalesLedger::default();
        fold_sales(
            &mut ledger.per_listing,
            vec![sold(2, 950_000), sold(2, 950_000), sold(9, 1_000_000)],
        );
        apply_sales(&mut snapshot, &ledger);
        assert_eq!(snapshot.listings[0].units_sold, 2);
        assert_eq!(snapshot.listings[0].earned_usdc, 1.9);
        // Listing without sales keeps its zeros.
        assert_eq!(snapshot.listings[1].units_sold, 0);
        assert_eq!(snapshot.listings[1].earned_usdc, 0.0);
    }

    #[test]
    fn fold_sales_accumulates_across_chunks() {
        let mut totals = HashMap::new();
        fold_sales(&mut totals, vec![sold(3, 500_000)]);
        fold_sales(&mut totals, vec![sold(3, 500_000)]);
        assert_eq!(
            totals.get(&3).copied(),
            Some(SaleTotals {
                units_sold: 2,
                units_sold_agents: 0,
                seller_proceeds_usdc6: 1_000_000,
            })
        );
    }

    // -- attestation-count carry-forward ---------------------------------------

    fn listing_with_attestations(listing_id: u64, attestation_count: u64) -> IndexedListing {
        IndexedListing {
            attestation_count,
            ..listing_with_id(listing_id)
        }
    }

    #[test]
    fn carry_forward_overlays_failed_listing_from_cache() {
        let mut last_known: HashMap<u64, u64> = HashMap::from([(2, 3)]);
        let mut snapshot = MarketplaceSnapshot {
            // Listing 2 failed this poll (count degraded to 0); listing 5 read fine.
            listings: vec![listing_with_attestations(2, 0), listing_with_attestations(5, 1)],
            ..Default::default()
        };
        carry_forward_attestations(&mut snapshot, &[2], &mut last_known);
        // Failed listing keeps its last-known count instead of flickering to 0.
        assert_eq!(snapshot.listings[0].attestation_count, 3);
        // Successful listing keeps the fresh value…
        assert_eq!(snapshot.listings[1].attestation_count, 1);
        // …and refreshes the cache; the failed listing's cache is untouched.
        assert_eq!(last_known.get(&5), Some(&1));
        assert_eq!(last_known.get(&2), Some(&3));
    }

    #[test]
    fn carry_forward_failed_listing_without_cache_stays_zero() {
        let mut last_known: HashMap<u64, u64> = HashMap::new();
        let mut snapshot = MarketplaceSnapshot {
            listings: vec![listing_with_attestations(7, 0)],
            ..Default::default()
        };
        carry_forward_attestations(&mut snapshot, &[7], &mut last_known);
        // Never read successfully → honest 0, nothing cached.
        assert_eq!(snapshot.listings[0].attestation_count, 0);
        assert!(last_known.is_empty());
    }

    #[test]
    fn carry_forward_successful_zero_updates_cache() {
        // A GENUINE zero (successful read) must overwrite a stale cached
        // value — e.g. after a chain reorg or registry redeploy.
        let mut last_known: HashMap<u64, u64> = HashMap::from([(4, 9)]);
        let mut snapshot = MarketplaceSnapshot {
            listings: vec![listing_with_attestations(4, 0)],
            ..Default::default()
        };
        carry_forward_attestations(&mut snapshot, &[], &mut last_known);
        assert_eq!(snapshot.listings[0].attestation_count, 0);
        assert_eq!(last_known.get(&4), Some(&0));
    }

    // -- Sold aggregation ----------------------------------------------------

    fn sold(listing_id: u64, seller_proceeds_usdc6: u64) -> IMarketplace::Sold {
        sold_with_path(listing_id, seller_proceeds_usdc6, 0) // 0 = direct (human)
    }

    fn sold_with_path(listing_id: u64, seller_proceeds_usdc6: u64, purchase_path: u8) -> IMarketplace::Sold {
        use alloy::primitives::aliases::U96;
        IMarketplace::Sold {
            listingId: U256::from(listing_id),
            agentNftId: U256::from(7u64),
            buyer: Address::ZERO,
            priceUSDC: U96::from(seller_proceeds_usdc6),
            sellerProceeds: U96::from(seller_proceeds_usdc6),
            protocolProceeds: U96::from(0u64),
            licenseTokenId: U256::from(listing_id),
            payerKind: purchase_path as u16,
            purchasePath: purchase_path,
        }
    }

    #[test]
    fn aggregate_sales_counts_units_and_sums_proceeds() {
        let totals = aggregate_sales(vec![sold(2, 950_000), sold(2, 950_000), sold(5, 46_550_000)]);
        assert_eq!(
            totals.get(&2).copied(),
            Some(SaleTotals {
                units_sold: 2,
                units_sold_agents: 0,
                seller_proceeds_usdc6: 1_900_000,
            })
        );
        assert_eq!(totals.get(&5).unwrap().units_sold, 1);
        assert_eq!(usdc6_to_f64(totals.get(&2).unwrap().seller_proceeds_usdc6), 1.9);
        assert!(totals.get(&99).is_none());
    }

    #[test]
    fn fold_sales_counts_x402_purchases_as_agents() {
        // listing 7: 1 direct + 2 x402 (purchasePath==1) → 3 units, 2 agents.
        let totals = aggregate_sales(vec![
            sold(7, 100_000),
            sold_with_path(7, 100_000, 1),
            sold_with_path(7, 100_000, 1),
        ]);
        let t = totals.get(&7).copied().unwrap();
        assert_eq!(t.units_sold, 3);
        assert_eq!(t.units_sold_agents, 2);
    }

    #[test]
    fn aggregate_sales_empty_is_empty() {
        assert!(aggregate_sales(vec![]).is_empty());
    }

    // -- tick timeout helper --------------------------------------------------

    #[test]
    fn apply_tick_timeout_sets_error_keeps_listings() {
        let listing = listing_with_id(1);
        let mut snapshot = MarketplaceSnapshot {
            listings: vec![listing],
            last_error: None,
            ..Default::default()
        };
        apply_tick_timeout(&mut snapshot);
        assert_eq!(
            snapshot.last_error.as_deref(),
            Some("indexer tick timed out after 120s")
        );
        // Listings must be preserved.
        assert_eq!(snapshot.listings.len(), 1);
        assert_eq!(snapshot.listings[0].listing_id, 1);
        // Timestamp updated (non-zero is sufficient; exact value is wall-clock).
        assert!(snapshot.last_poll_unix > 0);
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

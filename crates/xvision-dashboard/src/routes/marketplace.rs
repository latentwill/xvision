//! Mutating marketplace routes.
//!
//! - `POST /api/marketplace/publish` — mint a strategy's identity NFT with
//!   its Bitfields v3 genart tokenURI, then create the marketplace listing.
//! - `POST /api/marketplace/listings/:id/revoke` — seller-initiated revoke.
//! - `POST /api/marketplace/buy` — gasless x402 purchase relay
//!   (`buyWithAuthorization` signed by the buyer, gas paid by the relayer).
//! - `POST /api/marketplace/listings/:id/import` — license-gated bundle
//!   delivery: verify the buyer holds an ERC-1155 license, fetch+verify the
//!   manifest, install it as a new local strategy.
//! - `POST /api/marketplace/listings/:id/attest` — post an eval attestation
//!   (`EvalAttestationRegistry.postAttestation`, permissionless on-chain).
//! - `POST /api/marketplace/listings/:id/update` — seller-only content
//!   refresh (`ListingRegistry.updateListing`; price is immutable on-chain —
//!   re-pricing is revoke + relist).
//!
//! Flow: parse/validate body → load strategy → hash → tokenURI →
//! chain-config gate (the startup-resolved [`MarketplaceChainConfig`] in
//! `AppState`) → construct driver → IPFS pin (when an IPFS backend — Kubo or
//! Pinata — was configured at startup; a pin failure aborts BEFORE the
//! mint) → connect + register
//! (mint) → `publish_listing`. All config errors surface before the mint so
//! no orphan NFTs are created by a missing env var.
//!
//! Chain access is config-gated: `MarketplaceChainConfig` is resolved ONCE
//! at server startup from `XVN_RPC_URL` / `XVN_CHAIN_ID` /
//! `XVN_PUBLISHER_PK` (plus registry addresses); when the needed piece is
//! absent the route returns 503 so dev boxes degrade loudly — same status
//! and messages as the old per-request env reads (xvision-df3). All pure
//! logic (tier mapping, USDC scaling) is unit-testable without a chain.
//!
//! Idempotency: enforced via the `publish_receipts` store (bead xvision-4dn).
//! The first successful publish records a receipt keyed by `agent_id` (the
//! strategy ULID / NFT token id); a re-publish of an already-published
//! agent_id short-circuits with 409 Conflict BEFORE any chain or IPFS work,
//! so a re-click / retry / refresh cannot mint a duplicate NFT + listing.
//! Residual: the receipt is inserted AFTER the on-chain mint, so two
//! genuinely-concurrent first-publishes can still both mint before either
//! receipt lands — the store collapses the dominant sequential case, not a
//! hard mutex (see `publish_receipts`).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use alloy::network::EthereumWallet;
use alloy::primitives::{keccak256, Address, B256, U256};
use alloy::providers::ProviderBuilder;

use xvision_engine::api::strategy;
use xvision_engine::api::ApiError;
use xvision_engine::autooptimizer::content_hash::canonical_json;
use xvision_identity::contracts::IListingRegistry;
use xvision_identity::{generate_token_uri, manifest_hash_hex, IdentityClient};
use xvision_marketplace::adapter::{
    AnchorDriver, AttestRequest, BuyRequest, Erc8004MantleDriver, PublishRequest, TransferAuthorization,
};
use xvision_marketplace::IpfsStore;

use crate::chain_config::{chain_call_timeout, with_chain_timeout};
use crate::error::DashboardError;
use crate::routes::marketplace_auth::{build_challenge_message, verify_address_proof};
use crate::routes::publish_receipts::{find_receipt, insert_receipt};
use crate::state::AppState;

/// Current wall-clock time as unix seconds. Single source of "now" for the
/// sealed-import challenge issue/expiry checks; the pure verifier takes the
/// clock as a parameter so tests inject a fixed/advanced time.
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Request body for `POST /api/marketplace/publish`.
#[derive(Debug, Deserialize)]
pub struct PublishBody {
    /// Strategy ULID — the pre-mint `agent_id` (becomes the NFT token id
    /// lineage anchor post-mint).
    pub strategy_id: String,
    /// `"open"` or `"sealed"`.
    pub tier: String,
    /// Listing price in whole USDC (e.g. `49.0` → 49_000_000 on-chain).
    pub price_usdc: f64,
    /// Soulbound default is `false`.
    #[serde(default)]
    pub transferable_license: bool,
}

/// Response for a successful publish.
#[derive(Debug, Serialize)]
pub struct PublishOut {
    pub agent_id: String,
    /// 64-char lowercase hex keccak256 of the canonical strategy JSON.
    pub manifest_hash: String,
    /// Minted IdentityRegistry token id (decimal string).
    pub token_id: String,
    /// Created ListingRegistry listing id (decimal string).
    pub listing_id: String,
    /// Size of the generated `data:application/json;base64,…` tokenURI.
    pub token_uri_bytes: usize,
    /// Where the canonical manifest lives: `ipfs://<cid>` when an IPFS
    /// backend (`XVN_IPFS_API_URL` or `PINATA_JWT`) was configured at
    /// publish time, else the local `xvn://strategy/<id>` fallback.
    pub content_uri: String,
}

/// Pins the canonical manifest bytes, mapping any pin failure to a 503-class
/// error. The caller invokes this BEFORE the identity mint so an IPFS-backend
/// outage can never strand an orphan NFT (DashboardError has no 502 variant;
/// `ServiceUnavailable` is the upstream-dependency class here).
async fn pin_canonical(ipfs: &impl IpfsStore, canonical: &str) -> Result<String, DashboardError> {
    pin_bytes(ipfs, canonical.as_bytes()).await
}

/// Pins arbitrary bytes (plaintext canonical manifest for open tier, or the
/// sealed ciphertext for sealed tier), mapping any pin failure to the same
/// 503-class no-orphan error as [`pin_canonical`].
async fn pin_bytes(ipfs: &impl IpfsStore, bytes: &[u8]) -> Result<String, DashboardError> {
    ipfs.put(bytes).await.map_err(|e| {
        DashboardError::ServiceUnavailable(format!(
            "IPFS pin failed (publish aborted before mint, nothing on chain): {e}"
        ))
    })
}

/// Maps the wire tier name to its on-chain code: `"open"` → 0, `"sealed"` → 1.
fn tier_code(tier: &str) -> Result<u8, DashboardError> {
    match tier {
        "open" => Ok(0),
        "sealed" => Ok(1),
        other => Err(DashboardError::Validation {
            field: "tier".into(),
            msg: format!("must be \"open\" or \"sealed\", got {other:?}"),
        }),
    }
}

/// Converts a whole-USDC price to the 6-decimal on-chain representation.
/// Rejects non-finite or negative input.
fn usdc6(price: f64) -> Result<U256, DashboardError> {
    if !price.is_finite() || price < 0.0 {
        return Err(DashboardError::Validation {
            field: "price_usdc".into(),
            msg: "must be a finite, non-negative number".into(),
        });
    }
    let scaled = (price * 1_000_000.0).round();
    if scaled > u64::MAX as f64 {
        return Err(DashboardError::Validation {
            field: "price_usdc".into(),
            msg: "price too large".into(),
        });
    }
    Ok(U256::from(scaled as u64))
}

/// `POST /api/marketplace/publish` — mint + list. Returns 201 on success,
/// 503 when the chain env is not configured.
pub async fn post_publish(
    State(state): State<AppState>,
    Json(body): Json<PublishBody>,
) -> Result<(StatusCode, Json<PublishOut>), DashboardError> {
    // Validate the cheap, pure inputs before touching the store or chain.
    // Sealed-tier publishing IS supported (server-side encrypt-at-publish +
    // ciphertext pin); the tier-specific gating (require Lit, then an IPFS pin
    // backend) happens at the content_uri step below, after the chain gate and
    // before the mint, so an encrypt/pin failure leaves nothing on chain.
    let tier = tier_code(&body.tier)?;
    let price_usdc = usdc6(body.price_usdc)?;

    // a. Load the strategy (404s via ApiError::NotFound when absent).
    let strategy = strategy::get(&state.api_context(), &body.strategy_id).await?;

    // b. Canonical JSON → manifest hash.
    let value = serde_json::to_value(&strategy)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("serialize strategy: {e}")))?;
    let canonical = canonical_json(&value);
    let manifest_hash = manifest_hash_hex(&canonical);

    // c. The strategy ULID IS the pre-mint agent id.
    let agent_id = body.strategy_id.clone();

    // c.1. Idempotency gate (bead xvision-4dn). If this agent_id was already
    //      published, short-circuit with 409 Conflict BEFORE any tokenURI gen,
    //      chain gate, IPFS pin, or mint — so a re-click / retry / refresh
    //      cannot mint a duplicate NFT + listing. This lookup is cheap and
    //      precedes ALL chain/IPFS side effects (it must stay above the chain
    //      gate at step `e`). See `publish_receipts` for the residual
    //      concurrent-first-publish race.
    if let Some(r) = find_receipt(&state.pool, &agent_id).await? {
        return Err(DashboardError::Conflict(format!(
            "agent_id {agent_id} already published (token_id {}, listing_id {}); \
             re-publishing would mint a duplicate NFT",
            r.token_id, r.listing_id
        )));
    }

    // d. Genart tokenURI (data:application/json;base64,…).
    let token_uri =
        generate_token_uri(&agent_id, &manifest_hash).map_err(|e| DashboardError::Validation {
            field: "strategy_id".into(),
            msg: format!("genart tokenURI generation failed: {e}"),
        })?;

    // e. Chain gate — degrade loudly without chain config (resolved once at
    //    server startup; see `chain_config`). All config is validated here,
    //    before any chain write, so a missing env var cannot strand an
    //    orphan mint.
    let mp = state.marketplace_chain();
    let chain = mp.and_then(|c| c.chain.as_ref()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "chain publishing not configured: set XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK".into(),
        )
    })?;
    let registry_addresses = mp.and_then(|c| c.registry_addresses.clone()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "identity registry not configured: set XVN_IDENTITY_REGISTRY (and optionally \
             XVN_REPUTATION_REGISTRY)"
                .into(),
        )
    })?;
    let marketplace_addresses = mp.and_then(|c| c.marketplace_addresses.clone()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "marketplace not configured: set XVN_LISTING_REGISTRY (and related XVN_MARKETPLACE_* \
             vars)"
                .into(),
        )
    })?;
    let driver = Erc8004MantleDriver::with_signer(
        marketplace_addresses,
        chain.rpc_url.clone(),
        chain.chain_id,
        chain.signer.clone(),
    );

    // f. Decide the content_uri. The on-chain `content_hash` is ALWAYS the
    //    plaintext manifest hash (`manifest_hash`) for both tiers, so the
    //    genart seed + tokenURI are identical regardless of tier; the tiers
    //    diverge only in WHAT gets pinned at this step.
    //
    //    - Open tier: pin the plaintext canonical manifest (as before). The
    //      IPFS backend (Kubo preferred via XVN_IPFS_API_URL, else Pinata via
    //      PINATA_JWT) is optional — without it the listing keeps the local
    //      `xvn://` ref (the bundle route resolves both).
    //    - Sealed tier: encrypt the canonical plaintext server-side (the
    //      seller's own strategy) and pin the CIPHERTEXT. Requires BOTH Lit
    //      (else 400) AND an IPFS backend (else 503) — there is no `xvn://`
    //      fallback for sealed because the plaintext must never be locally
    //      resolvable as a public bundle.
    //
    //    Either branch happens AFTER all other config validation and BEFORE
    //    the mint: a pin (or encrypt) failure aborts with 503 and leaves
    //    nothing on chain.
    let content_uri = if tier == 1 {
        // Sealed: require Lit, then require an IPFS pin backend.
        let sealed = state.sealed_crypto();
        if !sealed.is_configured() {
            return Err(DashboardError::Validation {
                field: "tier".into(),
                msg: "sealed-tier publishing requires Lit configuration (XVN_LIT_*)".into(),
            });
        }
        let ipfs = mp.and_then(|c| c.ipfs.as_ref()).ok_or_else(|| {
            DashboardError::ServiceUnavailable(
                "sealed-tier publishing requires an IPFS pin backend: set XVN_IPFS_API_URL or \
                 PINATA_JWT"
                    .into(),
            )
        })?;
        let ciphertext = sealed
            .encrypt(canonical.as_bytes())
            .await
            .map_err(|e| DashboardError::ServiceUnavailable(format!("sealed encrypt failed: {e}")))?;
        let cid = pin_bytes(ipfs, ciphertext.as_bytes()).await?;
        format!("ipfs://{cid}")
    } else {
        match mp.and_then(|c| c.ipfs.as_ref()) {
            Some(ipfs) => {
                let cid = pin_canonical(ipfs, &canonical).await?;
                format!("ipfs://{cid}")
            }
            None => {
                tracing::info!(
                    agent_id = %agent_id,
                    "no IPFS backend configured (XVN_IPFS_API_URL / PINATA_JWT unset); \
                     publishing with local xvn:// content_uri"
                );
                format!("xvn://strategy/{agent_id}")
            }
        }
    };

    // g. Mint the identity NFT with the genart tokenURI.
    //    All config has been validated above — no config error can occur
    //    after this point. Every chain interaction is deadline-bounded
    //    (xvision-4fp; timeout → 503 with an explicit message).
    let timeout = chain_call_timeout(mp);
    let identity_client = with_chain_timeout(
        timeout,
        IdentityClient::connect(&chain.rpc_url, registry_addresses, chain.chain_id),
    )
    .await?
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("identity connect: {e}")))?;
    let agent_uri = url::Url::parse(&token_uri)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("tokenURI is not a valid URL: {e}")))?;
    let token_id = with_chain_timeout(timeout, identity_client.register(&agent_uri, &chain.signer))
        .await?
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("identity register: {e}")))?;

    // h. Create the marketplace listing.
    let content_hash: B256 = manifest_hash
        .parse()
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("manifest hash is not valid B256 hex: {e}")))?;
    let listing = with_chain_timeout(
        timeout,
        driver.publish_listing(PublishRequest {
            agent_nft_id: token_id.0,
            content_hash,
            content_uri: content_uri.clone(),
            tier,
            price_usdc,
            transferable_license: body.transferable_license,
        }),
    )
    .await?
    .map_err(|e| {
        DashboardError::Internal(anyhow::anyhow!(
            "publish listing failed after minting identity NFT token_id={}: {e}",
            token_id
        ))
    })?;

    // i. Persist the publish receipt (bead xvision-4dn) so a re-publish 409s
    //    at step `c.1` above. This runs AFTER the on-chain mint+list, so a DB
    //    failure here must NOT 500 the response — the NFT already exists on
    //    chain, and surfacing an error would make the operator retry, which is
    //    the exact duplicate-mint bug being fixed. Log-only is therefore the
    //    safer no-duplicate posture: a missed receipt leaves the next publish
    //    able to duplicate (rare), but a hard-fail GUARANTEES the operator
    //    retries into a duplicate.
    if let Err(e) = insert_receipt(
        &state.pool,
        &agent_id,
        &token_id.to_string(),
        &listing.listing_id.to_string(),
        &manifest_hash,
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    {
        tracing::error!(
            error = %e,
            agent_id = %agent_id,
            token_id = %token_id,
            listing_id = %listing.listing_id,
            "publish succeeded on chain but the receipt insert failed; a future \
             re-publish will not be short-circuited by the idempotency gate"
        );
    }

    // j. 201 + receipt.
    Ok((
        StatusCode::CREATED,
        Json(PublishOut {
            agent_id,
            manifest_hash,
            token_id: token_id.to_string(),
            listing_id: listing.listing_id.to_string(),
            token_uri_bytes: token_uri.len(),
            content_uri,
        }),
    ))
}

/// Response for a successful revoke.
#[derive(Debug, Serialize)]
pub struct RevokeOut {
    pub listing_id: u64,
    pub tx_hash: String,
}

/// `POST /api/marketplace/listings/:id/revoke` — seller-initiated revoke.
///
/// Returns 200 `{"listing_id": id, "tx_hash": "0x…"}` on success.
/// 503 when chain env is not configured or the private key is invalid;
/// 400 (via `DashboardError::Validation`) for any chain error from `revoke_listing`
/// (contract reverts, RPC transport failures, etc.) — the error text is included
/// in the response message.
pub async fn post_revoke(
    Path(id): Path<u64>,
    State(state): State<AppState>,
) -> Result<Json<RevokeOut>, DashboardError> {
    // a. Chain gate — startup-resolved config validated before any chain write.
    let mp = state.marketplace_chain();
    let chain = mp.and_then(|c| c.chain.as_ref()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "chain not configured: set XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK".into(),
        )
    })?;
    let marketplace_addresses = mp.and_then(|c| c.marketplace_addresses.clone()).ok_or_else(|| {
        DashboardError::ServiceUnavailable("marketplace not configured: set XVN_LISTING_REGISTRY".into())
    })?;

    // b. Build a signing driver and call revokeListing.
    let driver = Erc8004MantleDriver::with_signer(
        marketplace_addresses,
        chain.rpc_url.clone(),
        chain.chain_id,
        chain.signer.clone(),
    );
    let tx_hash = with_chain_timeout(chain_call_timeout(mp), driver.revoke_listing(U256::from(id)))
        .await?
        .map_err(|e| {
            // All chain errors (contract reverts: NotSeller, UnknownListing, AlreadyRevoked,
            // and RPC transport failures) map to DashboardError::Validation → 400 BAD_REQUEST.
            // NOTE: maps ALL chain errors (incl. RPC transport) to 400 — acceptable for
            // testnet; split transport → 503 when this hardens.
            let msg = e.to_string();
            DashboardError::Validation {
                field: "listing_id".into(),
                msg: format!("revoke failed: {msg}"),
            }
        })?;

    Ok(Json(RevokeOut {
        listing_id: id,
        tx_hash: format!("0x{tx_hash:x}"),
    }))
}

/// Request body for an in-place reprice.
#[derive(Debug, Deserialize)]
pub struct SetPriceBody {
    /// New whole-USDC price. `0` makes the listing free (open/clone path).
    pub price_usdc: f64,
}

/// Response for a successful reprice.
#[derive(Debug, Serialize)]
pub struct SetPriceOut {
    pub listing_id: u64,
    pub price_usdc: f64,
    pub tx_hash: String,
}

/// `POST /api/marketplace/listings/:id/price` — seller-initiated in-place
/// repricing (mirrors the revoke path: server signer + on-chain seller check).
///
/// Returns 200 `{"listing_id", "price_usdc", "tx_hash"}` on success.
/// 400 for a bad price or any chain error from `updatePrice` (contract reverts:
/// NotSeller, UnknownListing, AlreadyRevoked, FreeTransferableForbidden).
/// 503 when chain env is not configured.
pub async fn post_set_price(
    Path(id): Path<u64>,
    State(state): State<AppState>,
    Json(body): Json<SetPriceBody>,
) -> Result<Json<SetPriceOut>, DashboardError> {
    // a. Validate the price before touching the chain.
    let price6 = usdc6(body.price_usdc)?;

    // b. Chain gate — startup-resolved config validated before any chain write.
    let mp = state.marketplace_chain();
    let chain = mp.and_then(|c| c.chain.as_ref()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "chain not configured: set XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK".into(),
        )
    })?;
    let marketplace_addresses = mp.and_then(|c| c.marketplace_addresses.clone()).ok_or_else(|| {
        DashboardError::ServiceUnavailable("marketplace not configured: set XVN_LISTING_REGISTRY".into())
    })?;

    // c. Build a signing driver and call updatePrice.
    let driver = Erc8004MantleDriver::with_signer(
        marketplace_addresses,
        chain.rpc_url.clone(),
        chain.chain_id,
        chain.signer.clone(),
    );
    let tx_hash =
        with_chain_timeout(chain_call_timeout(mp), driver.update_price(U256::from(id), price6))
            .await?
            .map_err(|e| {
                // Contract reverts (NotSeller, UnknownListing, AlreadyRevoked,
                // FreeTransferableForbidden) and RPC failures map to 400. Testnet
                // posture, same as revoke.
                let msg = e.to_string();
                DashboardError::Validation {
                    field: "listing_id".into(),
                    msg: format!("reprice failed: {msg}"),
                }
            })?;

    Ok(Json(SetPriceOut {
        listing_id: id,
        price_usdc: body.price_usdc,
        tx_hash: format!("0x{tx_hash:x}"),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/marketplace/buy — gasless x402 relay (buyWithAuthorization)
// ---------------------------------------------------------------------------

/// The buyer's signed EIP-3009 `TransferWithAuthorization` payload, as sent
/// by the frontend after `eth_signTypedData_v4`.
///
/// Wire encoding:
/// - `from` / `to` — `0x…` 40-hex addresses;
/// - `value` — **decimal string of 6-decimal USDC units** (e.g. `"49000000"`
///   for 49 USDC). Decimal-only by design: it is what the frontend already
///   holds as a bigint, and avoids a dual hex/decimal parse ambiguity;
/// - `valid_after` / `valid_before` — unix seconds as JSON numbers;
/// - `nonce` / `r` / `s` — `0x…` 64-hex (bytes32);
/// - `v` — JSON number (must fit u8; validated, not silently truncated).
#[derive(Debug, Deserialize)]
pub struct AuthorizationBody {
    pub from: String,
    pub to: String,
    pub value: String,
    pub valid_after: u64,
    pub valid_before: u64,
    pub nonce: String,
    pub v: u64,
    pub r: String,
    pub s: String,
}

/// Request body for `POST /api/marketplace/buy`.
#[derive(Debug, Deserialize)]
pub struct BuyBody {
    pub listing_id: u64,
    /// Wallet that receives the license token. Must equal
    /// `authorization.from` (contract guard M-2, `RecipientMustBePayer`).
    pub recipient: String,
    pub authorization: AuthorizationBody,
}

/// Response for a successful relayed purchase.
#[derive(Debug, Serialize)]
pub struct BuyOut {
    pub tx_hash: String,
    /// Decimal string; `== listing_id` by the surface-spec invariant.
    pub license_token_id: String,
}

fn parse_address(field: &str, s: &str) -> Result<Address, DashboardError> {
    s.parse().map_err(|_| DashboardError::Validation {
        field: field.into(),
        msg: "must be a 0x-prefixed 40-hex-char address".into(),
    })
}

fn parse_b256(field: &str, s: &str) -> Result<B256, DashboardError> {
    s.parse().map_err(|_| DashboardError::Validation {
        field: field.into(),
        msg: "must be a 0x-prefixed 64-hex-char value (bytes32)".into(),
    })
}

/// Parses the `value` field: a decimal string of 6-decimal USDC units.
fn parse_usdc6_decimal(field: &str, s: &str) -> Result<U256, DashboardError> {
    if s.is_empty() {
        return Err(DashboardError::Validation {
            field: field.into(),
            msg: "must be a decimal string of 6-decimal USDC units".into(),
        });
    }
    U256::from_str_radix(s, 10).map_err(|_| DashboardError::Validation {
        field: field.into(),
        msg: "must be a decimal string of 6-decimal USDC units".into(),
    })
}

/// Pure body → driver-request conversion. Validation order: all field
/// parses (400) → M-2 recipient==from (400). Env gating happens after this
/// in the handler, so a malformed body never reports a misleading 503.
fn build_buy_request(body: &BuyBody) -> Result<BuyRequest, DashboardError> {
    let recipient = parse_address("recipient", &body.recipient)?;
    let auth = &body.authorization;
    let from = parse_address("authorization.from", &auth.from)?;
    let to = parse_address("authorization.to", &auth.to)?;
    let value = parse_usdc6_decimal("authorization.value", &auth.value)?;
    let nonce = parse_b256("authorization.nonce", &auth.nonce)?;
    let r = parse_b256("authorization.r", &auth.r)?;
    let s = parse_b256("authorization.s", &auth.s)?;
    let v: u8 = auth.v.try_into().map_err(|_| DashboardError::Validation {
        field: "authorization.v".into(),
        msg: "must fit in u8 (27/28 or 0/1)".into(),
    })?;

    // Contract guard M-2: `buyWithAuthorization` reverts with
    // `RecipientMustBePayer()` when recipient != auth.from. Reject here so
    // the relay never pays gas for a guaranteed revert (the driver re-checks).
    if recipient != from {
        return Err(DashboardError::Validation {
            field: "recipient".into(),
            msg: "recipient must equal authorization.from (contract guard RecipientMustBePayer)".into(),
        });
    }

    Ok(BuyRequest {
        listing_id: U256::from(body.listing_id),
        recipient,
        authorization: Some(TransferAuthorization {
            from,
            to,
            value,
            valid_after: U256::from(auth.valid_after),
            valid_before: U256::from(auth.valid_before),
            nonce,
            v,
            r,
            s,
        }),
    })
}

/// `POST /api/marketplace/buy` — relay a buyer-signed EIP-3009 authorization
/// through `Marketplace.buyWithAuthorization`. The relayer
/// (`XVN_PUBLISHER_PK`) pays gas; the signature is the buyer's authority —
/// the server never holds buyer funds.
///
/// 400 malformed body / M-2 violation / contract revert; 503 chain env
/// unconfigured; 200 `{tx_hash, license_token_id}`.
pub async fn post_buy(
    State(state): State<AppState>,
    Json(body): Json<BuyBody>,
) -> Result<Json<BuyOut>, DashboardError> {
    // a. Parse + validate everything (incl. M-2) before any config/chain work.
    let req = build_buy_request(&body)?;

    // b. Chain gate — degrade loudly without startup-resolved config.
    let mp = state.marketplace_chain();
    let chain = mp.and_then(|c| c.chain.as_ref()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "chain relay not configured: set XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK".into(),
        )
    })?;
    let marketplace_addresses = mp.and_then(|c| c.marketplace_addresses.clone()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "marketplace not configured: set XVN_LISTING_REGISTRY, XVN_MARKETPLACE_CONTRACT, \
             XVN_MARKETPLACE_USDC"
                .into(),
        )
    })?;

    // c. Relay the purchase.
    let driver = Erc8004MantleDriver::with_signer(
        marketplace_addresses,
        chain.rpc_url.clone(),
        chain.chain_id,
        chain.signer.clone(),
    );
    let receipt = with_chain_timeout(chain_call_timeout(mp), driver.buy_listing(req))
        .await?
        .map_err(|e| {
            // Contract reverts (UnknownListing, ListingRevoked, expired/used
            // authorization, bad signature) map to 400 with the chain text.
            // NOTE: like post_revoke this also maps RPC transport errors to 400 —
            // acceptable for testnet; split transport → 503 when this hardens.
            let msg = e.to_string();
            DashboardError::Validation {
                field: "listing_id".into(),
                msg: format!("buy failed: {msg}"),
            }
        })?;

    Ok(Json(BuyOut {
        tx_hash: format!("0x{:x}", receipt.tx_hash),
        license_token_id: receipt.license_token_id.to_string(),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/marketplace/listings/:id/import — license-gated bundle delivery
// ---------------------------------------------------------------------------

/// Request body for `POST /api/marketplace/listings/:id/import`.
#[derive(Debug, Deserialize)]
pub struct ImportBody {
    /// The buyer's wallet — must hold an ERC-1155 license for the listing.
    pub address: String,
}

/// Response for a successful import: the freshly minted local strategy id.
#[derive(Debug, Serialize)]
pub struct ImportOut {
    pub agent_id: String,
}

/// `POST /api/marketplace/listings/:id/import` — install a purchased
/// strategy into the local engine.
///
/// Order: validate address (400) → listing from snapshot (404) → license
/// gate via `ILicenseToken::balanceOf(address, listing_id)` over the
/// read-only provider (503 when `XVN_LICENSE_TOKEN` / indexer chain env is
/// dormant; 403 when the balance is zero) → fetch + hash-verify the bundle
/// (shared with `GET …/:id/bundle`; 409 on integrity mismatch) →
/// `import_strategy` mints a NEW local ULID → 201 `{agent_id}`.
///
/// V1 CAVEAT: `address` is asserted by the client, not proven — there is no
/// signature challenge yet. Anyone who knows a license-holding address can
/// trigger an import of an OPEN-tier manifest, which is acceptable because
/// the open-tier manifest is already public (pinned plaintext on IPFS).
/// Signature-challenge auth arrives with the sealed tier.
pub async fn post_import(
    Path(id): Path<u64>,
    State(state): State<AppState>,
    Json(body): Json<ImportBody>,
) -> Result<(StatusCode, Json<ImportOut>), DashboardError> {
    // a. Validate the asserted wallet before any snapshot/chain work.
    let address = parse_address("address", &body.address)?;

    // b. Listing from the indexer snapshot.
    let listing = {
        let snap = state.marketplace_snapshot.read().await;
        snap.listings
            .iter()
            .find(|l| l.listing_id == id)
            .cloned()
            .ok_or_else(|| DashboardError::NotFound(format!("listing {id} not in indexed snapshot")))?
    };

    // c. License gate (shared with import-sealed): 503 when license/chain env
    //    is dormant, 403 when the balance is zero.
    license_gate(&state, address, id).await?;

    // d. Fetch + hash-verify the bundle (404/409/503 per the shared fn).
    let manifest = crate::routes::marketplace_read::fetch_verified_manifest(&state, &listing).await?;

    // e. Install as a NEW local strategy (fresh ULID; provenance stashed in
    //    mechanical_params.metadata.imported_from).
    let imported = strategy::import_strategy(&state.api_context(), manifest).await?;

    Ok((
        StatusCode::CREATED,
        Json(ImportOut {
            agent_id: imported.manifest.id,
        }),
    ))
}

/// Shared license gate for the import routes: verifies the asserted wallet
/// holds an ERC-1155 license for the listing via
/// `ILicenseToken::balanceOf(address, listing_id)` over the read-only
/// provider. All startup-resolved config is validated before the chain read so
/// a dev box degrades with 503, never a silent skip. 503 when the license
/// token / indexer chain env is dormant; 403 when the balance is zero; `Ok`
/// when the wallet holds at least one license.
async fn license_gate(state: &AppState, address: Address, id: u64) -> Result<(), DashboardError> {
    let mp = state.marketplace_chain();
    let license_token: Address = mp.and_then(|c| c.license_token).ok_or_else(|| {
        DashboardError::ServiceUnavailable("license gating not configured: set XVN_LICENSE_TOKEN".into())
    })?;
    let cfg = mp.and_then(|c| c.indexer.as_ref()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "marketplace chain access not configured: set XVN_RPC_URL, XVN_LISTING_REGISTRY, \
             XVN_IDENTITY_REGISTRY"
                .into(),
        )
    })?;
    let timeout = chain_call_timeout(mp);
    let provider = with_chain_timeout(
        timeout,
        alloy::providers::ProviderBuilder::new().connect(cfg.rpc_url.as_str()),
    )
    .await?
    .map_err(|e| DashboardError::ServiceUnavailable(format!("rpc connect failed: {e}")))?;
    let balance = with_chain_timeout(
        timeout,
        xvision_identity::contracts::ILicenseToken::new(license_token, &provider)
            .balanceOf(address, U256::from(id))
            .call(),
    )
    .await?
    .map_err(|e| DashboardError::ServiceUnavailable(format!("license balance lookup failed: {e}")))?;
    if balance.is_zero() {
        return Err(DashboardError::Forbidden(format!(
            "no license for {address:#x} on listing {id}"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// POST /api/marketplace/listings/:id/import-sealed — sealed bundle import
// ---------------------------------------------------------------------------

/// Response for `GET /api/marketplace/listings/:id/import-challenge`: a fresh,
/// single-use, time-bounded nonce plus the exact message the client must
/// `personal_sign` (byte-compatible with the Lit gate action's grammar).
#[derive(Debug, Serialize)]
pub struct ImportChallengeOut {
    /// Server-issued single-use nonce (consumed on a successful import).
    pub nonce: String,
    /// Unix-seconds expiry of both the nonce and the message the client signs.
    pub expiry_unix: u64,
    /// The exact byte string to sign — embeds the listing id, nonce, and
    /// expiry. The frontend may rebuild this itself, but signing the
    /// server-issued string guarantees the nonce binding.
    pub message: String,
}

/// `GET /api/marketplace/listings/:id/import-challenge` — issue a fresh,
/// single-use, time-bounded proof-of-address challenge for the sealed import
/// route (lane cgz). 404 if the listing is not in the indexed snapshot.
///
/// The buyer `personal_sign`s the returned `message` and POSTs the signature to
/// `import-sealed`; the server recovers the signer, requires it to equal the
/// claimed address, validates the message binding/freshness, and consumes the
/// nonce single-use (a replay or expiry → 401).
pub async fn get_import_challenge(
    Path(id): Path<u64>,
    State(state): State<AppState>,
) -> Result<Json<ImportChallengeOut>, DashboardError> {
    // Listing must exist in the indexed snapshot (404 unknown) so a challenge
    // is only ever issued for a real listing.
    {
        let snap = state.marketplace_snapshot.read().await;
        snap.listings
            .iter()
            .find(|l| l.listing_id == id)
            .ok_or_else(|| DashboardError::NotFound(format!("listing {id} not in indexed snapshot")))?;
    }
    let (nonce, expiry_unix) = state.marketplace_nonces().issue(id, now_unix());
    let message = build_challenge_message(id, &nonce, expiry_unix);
    Ok(Json(ImportChallengeOut {
        nonce,
        expiry_unix,
        message,
    }))
}

/// Request body for `POST /api/marketplace/listings/:id/import-sealed`. Unlike
/// open-tier import, the manifest is supplied by the caller: the browser
/// decrypted it client-side via Lit, so the server never sees the ciphertext
/// key. `address` is still required for the server-side license re-check, and
/// (lane cgz) the caller must prove control of it: `message` is the
/// server-issued challenge string and `signature` its EIP-191 `personal_sign`.
#[derive(Debug, Deserialize)]
pub struct ImportSealedBody {
    /// The buyer's wallet — must hold an ERC-1155 license for the listing AND
    /// be proven via the signature below.
    pub address: String,
    /// The decrypted strategy manifest (browser-side Lit decrypt output).
    pub manifest: serde_json::Value,
    /// The signed challenge message (from `import-challenge`; embeds the
    /// listing id, the server-issued nonce, and an expiry). Optional in the
    /// wire type so a missing proof yields a clean 400 (rather than a serde
    /// 422) AFTER the address/listing checks.
    #[serde(default)]
    pub message: Option<String>,
    /// EIP-191 `personal_sign` of `message` by `address` (0x-prefixed 65-byte
    /// hex). The server recovers the signer and requires it to equal `address`.
    #[serde(default)]
    pub signature: Option<String>,
}

/// Re-verifies a client-supplied manifest against a listing's on-chain
/// `content_hash` (which commits to the canonical PLAINTEXT for both tiers).
/// 409 `Conflict` on mismatch — this is the defense against a malicious
/// browser POSTing arbitrary JSON to the sealed-import route.
fn verify_manifest_matches_onchain(
    manifest: &serde_json::Value,
    onchain_content_hash: &str,
) -> Result<(), DashboardError> {
    let canonical = canonical_json(manifest);
    let manifest_hash = manifest_hash_hex(&canonical);
    let onchain_hash = onchain_content_hash.trim_start_matches("0x").to_lowercase();
    if manifest_hash != onchain_hash {
        return Err(DashboardError::Conflict(format!(
            "manifest does not match the listing's on-chain hash (on-chain {onchain_hash}, \
             supplied manifest hashes to {manifest_hash})"
        )));
    }
    Ok(())
}

/// `POST /api/marketplace/listings/:id/import-sealed` — install a sealed
/// strategy that the buyer's browser decrypted via the Lit gate action.
///
/// Order: validate address (400) → listing from snapshot (404) → PROOF OF
/// ADDRESS (lane cgz): recover the EIP-191 signer of `message` and require it to
/// equal `address` (403 on mismatch / 400 on malformed signature), validate the
/// message binding+freshness (401), then consume the server-issued nonce
/// single-use (401 on replay/expired/unknown) → license gate (503 dormant / 403
/// no license, shared with open import) → RE-VERIFY
/// `keccak256(canonical_json(manifest)) == listing.content_hash` (409 on
/// mismatch — this is the defense against a malicious browser POSTing arbitrary
/// JSON; the on-chain `content_hash` commits to the canonical plaintext for
/// both tiers) → `import_strategy` mints a NEW local ULID → 201 `{agent_id}`.
///
/// The proof check runs BEFORE the license gate so a caller who cannot prove
/// control of `address` is rejected (403/401) regardless of chain config — the
/// v1 address-assertion caveat is closed. The nonce is server-issued via
/// `GET …/import-challenge`; one challenge == one import (a fresh challenge is
/// required per attempt). The server never decrypts (keys live in Lit's TEE);
/// it trusts the client-supplied manifest ONLY after the on-chain hash re-check
/// passes.
pub async fn post_import_sealed(
    Path(id): Path<u64>,
    State(state): State<AppState>,
    Json(body): Json<ImportSealedBody>,
) -> Result<(StatusCode, Json<ImportOut>), DashboardError> {
    // a. Validate the asserted wallet before any snapshot/chain work.
    let address = parse_address("address", &body.address)?;

    // b. Listing from the indexer snapshot (404 unknown).
    let listing = {
        let snap = state.marketplace_snapshot.read().await;
        snap.listings
            .iter()
            .find(|l| l.listing_id == id)
            .cloned()
            .ok_or_else(|| DashboardError::NotFound(format!("listing {id} not in indexed snapshot")))?
    };

    // c. PROOF OF ADDRESS (lane cgz). The signed challenge + signature are
    //    required; a missing one is a clean 400 (after address/listing checks).
    let message =
        body.message
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| DashboardError::Validation {
                field: "message".into(),
                msg: "sealed import requires the signed challenge message (GET …/import-challenge)".into(),
            })?;
    let signature = body
        .signature
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| DashboardError::Validation {
            field: "signature".into(),
            msg: "sealed import requires the personal_sign signature of the challenge message".into(),
        })?;
    //    Recover the signer from the EIP-191 signature and require it to equal
    //    the claimed address, then validate the message's listing binding +
    //    freshness. Runs BEFORE the license gate so a forged/mismatched proof
    //    is rejected (403/401) even when the chain env is dormant — closing the
    //    v1 address-assertion caveat.
    let now = now_unix();
    let proof = verify_address_proof(address, message, signature, id, now)?;
    // Consume the server-issued nonce single-use: a replayed, expired, or
    // never-issued nonce → 401. This is the actual replay defense (the Lit gate
    // action is deliberately stateless).
    state
        .marketplace_nonces()
        .consume(&proof.nonce, id, now)
        .map_err(|e| {
            use crate::marketplace_nonce::NonceError;
            DashboardError::Unauthorized(match e {
                NonceError::UnknownOrConsumed => {
                    "import challenge nonce was never issued or already used".into()
                }
                NonceError::ListingMismatch => "import challenge nonce is for a different listing".into(),
                NonceError::Expired => "import challenge nonce expired".into(),
            })
        })?;

    // d. License gate (503 dormant / 403 no license).
    license_gate(&state, address, id).await?;

    // e. Re-verify the client-supplied manifest against the on-chain hash. The
    //    content_hash commits to the canonical plaintext (identical for both
    //    tiers), so a browser that swaps in arbitrary JSON is rejected here.
    verify_manifest_matches_onchain(&body.manifest, &listing.content_hash)?;

    // f. Install as a NEW local strategy (fresh ULID).
    let imported = strategy::import_strategy(&state.api_context(), body.manifest).await?;

    Ok((
        StatusCode::CREATED,
        Json(ImportOut {
            agent_id: imported.manifest.id,
        }),
    ))
}

// ---------------------------------------------------------------------------
// POST /api/marketplace/listings/:id/attest — manual eval attestation
// ---------------------------------------------------------------------------

/// Request body for `POST /api/marketplace/listings/:id/attest`.
#[derive(Debug, Deserialize)]
pub struct AttestBody {
    /// Decision cycles backing the eval result.
    pub cycles: u64,
    /// Sharpe ratio of the eval — must be finite.
    pub sharpe: f64,
}

/// Response for a successful attestation post.
#[derive(Debug, Serialize)]
pub struct AttestOut {
    pub tx_hash: String,
}

/// The attest payload convention shared with `xvn marketplace attest`
/// (crates/xvision-cli/src/commands/marketplace.rs): the on-chain
/// `evalResultHash` is the keccak256 of the compact
/// `{"cycles":N,"sharpe":F}` JSON bytes.
fn attest_payload_hash(cycles: u64, sharpe: f64) -> B256 {
    let payload = serde_json::json!({ "cycles": cycles, "sharpe": sharpe });
    keccak256(payload.to_string().as_bytes())
}

/// `POST /api/marketplace/listings/:id/attest` — post an eval attestation
/// for a listing. Permissionless on-chain (attester = the server's publisher
/// key here).
///
/// Order: validate sharpe (400) → listing from snapshot (404) → chain env
/// gate (503) → `driver.attest_eval` (chain errors → 400 with the chain
/// text, same posture as revoke/buy) → 201 `{tx_hash}`.
pub async fn post_attest(
    Path(id): Path<u64>,
    State(state): State<AppState>,
    Json(body): Json<AttestBody>,
) -> Result<(StatusCode, Json<AttestOut>), DashboardError> {
    // a. Validate the cheap, pure input first.
    if !body.sharpe.is_finite() {
        return Err(DashboardError::Validation {
            field: "sharpe".into(),
            msg: "must be a finite number".into(),
        });
    }

    // b. Listing from the indexer snapshot (404 unknown).
    {
        let snap = state.marketplace_snapshot.read().await;
        snap.listings
            .iter()
            .find(|l| l.listing_id == id)
            .ok_or_else(|| DashboardError::NotFound(format!("listing {id} not in indexed snapshot")))?;
    }

    // c. Chain gate — startup-resolved config validated before any chain write.
    let mp = state.marketplace_chain();
    let chain = mp.and_then(|c| c.chain.as_ref()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "chain not configured: set XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK".into(),
        )
    })?;
    let marketplace_addresses = mp.and_then(|c| c.marketplace_addresses.clone()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "marketplace not configured: set XVN_LISTING_REGISTRY and XVN_EVAL_ATTESTATION".into(),
        )
    })?;

    // d. Post the attestation with the CLI payload convention.
    let driver = Erc8004MantleDriver::with_signer(
        marketplace_addresses,
        chain.rpc_url.clone(),
        chain.chain_id,
        chain.signer.clone(),
    );
    let tx_hash = with_chain_timeout(
        chain_call_timeout(mp),
        driver.attest_eval(AttestRequest {
            listing_id: U256::from(id),
            eval_result_hash: attest_payload_hash(body.cycles, body.sharpe),
            eval_result_uri: format!("xvn://eval/listing/{id}"),
            schema: B256::ZERO,
        }),
    )
    .await?
    .map_err(|e| {
        // Chain errors (incl. NotConfigured zero-address and RPC
        // transport) map to 400 with the chain text — same testnet
        // posture as post_revoke / post_buy.
        let msg = e.to_string();
        DashboardError::Validation {
            field: "listing_id".into(),
            msg: format!("attest failed: {msg}"),
        }
    })?;

    Ok((
        StatusCode::CREATED,
        Json(AttestOut {
            tx_hash: format!("0x{tx_hash:x}"),
        }),
    ))
}

// ---------------------------------------------------------------------------
// POST /api/marketplace/listings/:id/update — seller content refresh
// ---------------------------------------------------------------------------

/// Response for a successful content update.
#[derive(Debug, Serialize)]
pub struct UpdateOut {
    pub listing_id: u64,
    /// 64-char lowercase hex keccak256 of the canonical strategy JSON (the
    /// new on-chain `contentHash`).
    pub content_hash: String,
    /// `ipfs://<cid>` when an IPFS backend (`XVN_IPFS_API_URL` or
    /// `PINATA_JWT`) was configured, else the local `xvn://strategy/<id>`
    /// fallback (same convention as publish).
    pub content_uri: String,
    pub tx_hash: String,
}

/// `POST /api/marketplace/listings/:id/update` — re-canonicalize the local
/// strategy behind a listing and push the new content hash/URI on-chain via
/// `ListingRegistry.updateListing`. Content-only: price is immutable
/// on-chain (re-pricing = revoke + relist).
///
/// Order: listing from snapshot (404) → local strategy by `listing.agent_id`
/// (404 with an explicit "local strategy not found") → canonical + hash →
/// chain env gate (503, all config before any pin/write) → IPFS pin when a
/// backend is configured (shared `pin_canonical`; failure aborts before the
/// chain write) → `updateListing` (reverts like NotSeller → 400 with the
/// chain text) → 200 `{listing_id, content_hash, content_uri, tx_hash}`.
pub async fn post_update(
    Path(id): Path<u64>,
    State(state): State<AppState>,
) -> Result<Json<UpdateOut>, DashboardError> {
    // a. Listing from the indexer snapshot (404 unknown).
    let listing = {
        let snap = state.marketplace_snapshot.read().await;
        snap.listings
            .iter()
            .find(|l| l.listing_id == id)
            .cloned()
            .ok_or_else(|| DashboardError::NotFound(format!("listing {id} not in indexed snapshot")))?
    };

    // b. Load the local strategy behind the listing. A missing strategy is a
    //    distinct, named 404 (the listing exists; this host can't rebuild
    //    its content).
    let strategy = strategy::get(&state.api_context(), &listing.agent_id)
        .await
        .map_err(|e| match e {
            ApiError::NotFound(_) => DashboardError::NotFound(format!(
                "local strategy not found for listing {id} (agent_id {:?})",
                listing.agent_id
            )),
            other => other.into(),
        })?;

    // c. Canonical JSON → new content hash.
    let value = serde_json::to_value(&strategy)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("serialize strategy: {e}")))?;
    let canonical = canonical_json(&value);
    let manifest_hash = manifest_hash_hex(&canonical);
    let content_hash: B256 = manifest_hash
        .parse()
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("manifest hash is not valid B256 hex: {e}")))?;

    // d. Chain gate — startup-resolved config validated before the pin or
    //    chain write.
    let mp = state.marketplace_chain();
    let chain = mp.and_then(|c| c.chain.as_ref()).ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "chain not configured: set XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK".into(),
        )
    })?;
    let marketplace_addresses = mp.and_then(|c| c.marketplace_addresses.clone()).ok_or_else(|| {
        DashboardError::ServiceUnavailable("marketplace not configured: set XVN_LISTING_REGISTRY".into())
    })?;

    // e. Pin the refreshed manifest when an IPFS backend is configured
    //    (Kubo preferred, else Pinata; failure aborts with 503 BEFORE the
    //    chain write — same no-orphan posture as publish), else keep the
    //    local xvn:// reference.
    let content_uri = match mp.and_then(|c| c.ipfs.as_ref()) {
        Some(ipfs) => {
            let cid = pin_canonical(ipfs, &canonical).await?;
            format!("ipfs://{cid}")
        }
        None => {
            tracing::info!(
                listing_id = id,
                agent_id = %listing.agent_id,
                "no IPFS backend configured (XVN_IPFS_API_URL / PINATA_JWT unset); \
                 updating listing with local xvn:// content_uri"
            );
            format!("xvn://strategy/{}", listing.agent_id)
        }
    };

    // f. updateListing via the IListingRegistry binding with the publisher
    //    signer (seller-only on-chain; NotSeller and friends → 400 below).
    let timeout = chain_call_timeout(mp);
    let wallet = EthereumWallet::from(chain.signer.clone());
    let provider = with_chain_timeout(
        timeout,
        ProviderBuilder::new()
            .wallet(wallet)
            .connect(chain.rpc_url.as_str()),
    )
    .await?
    .map_err(|e| DashboardError::ServiceUnavailable(format!("rpc connect failed: {e}")))?;
    let registry = IListingRegistry::new(marketplace_addresses.listing_registry, &provider);
    let receipt = with_chain_timeout(timeout, async {
        registry
            .updateListing(U256::from(id), content_hash, content_uri.clone())
            .send()
            .await
            .map_err(|e| update_chain_error(e.to_string()))?
            .get_receipt()
            .await
            .map_err(|e| update_chain_error(e.to_string()))
    })
    .await??;

    Ok(Json(UpdateOut {
        listing_id: id,
        content_hash: manifest_hash,
        content_uri,
        tx_hash: format!("0x{:x}", receipt.transaction_hash),
    }))
}

/// Maps `updateListing` chain failures to 400 with the chain text (contract
/// reverts: NotSeller, UnknownListing, AlreadyRevoked; plus RPC transport —
/// the same testnet posture as post_revoke).
fn update_chain_error(msg: String) -> DashboardError {
    DashboardError::Validation {
        field: "listing_id".into(),
        msg: format!("update failed: {msg}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_mapping() {
        assert_eq!(tier_code("open").unwrap(), 0);
        assert_eq!(tier_code("sealed").unwrap(), 1);
        assert!(tier_code("bogus").is_err());
    }

    #[test]
    fn price_to_usdc6() {
        assert_eq!(usdc6(49.0).unwrap().to_string(), "49000000");
        assert_eq!(usdc6(0.5).unwrap().to_string(), "500000");
        assert!(usdc6(-1.0).is_err());
        assert!(usdc6(f64::NAN).is_err());
    }

    // --- build_buy_request ---------------------------------------------------

    const PAYER: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn buy_body(recipient: &str, from: &str, v: u64) -> BuyBody {
        BuyBody {
            listing_id: 1,
            recipient: recipient.to_string(),
            authorization: AuthorizationBody {
                from: from.to_string(),
                to: "0xcccccccccccccccccccccccccccccccccccccccc".to_string(),
                value: "49000000".to_string(),
                valid_after: 0,
                valid_before: 1_893_456_000,
                nonce: format!("0x{}", "ab".repeat(32)),
                v,
                r: format!("0x{}", "cd".repeat(32)),
                s: format!("0x{}", "ef".repeat(32)),
            },
        }
    }

    #[test]
    fn buy_request_happy_path() {
        let req = build_buy_request(&buy_body(PAYER, PAYER, 27)).unwrap();
        assert_eq!(req.listing_id, U256::from(1u64));
        assert_eq!(req.recipient, PAYER.parse::<Address>().unwrap());
        let auth = req.authorization.expect("authorization present");
        assert_eq!(auth.from, req.recipient);
        assert_eq!(auth.value, U256::from(49_000_000u64));
        assert_eq!(auth.v, 27);
    }

    #[test]
    fn buy_request_bad_recipient_is_validation_error() {
        let err = build_buy_request(&buy_body("nope", PAYER, 27)).unwrap_err();
        assert!(matches!(err, DashboardError::Validation { ref field, .. } if field == "recipient"));
    }

    #[test]
    fn buy_request_v_out_of_range_is_validation_error() {
        let err = build_buy_request(&buy_body(PAYER, PAYER, 300)).unwrap_err();
        assert!(matches!(err, DashboardError::Validation { ref field, .. } if field == "authorization.v"));
    }

    #[test]
    fn buy_request_value_must_be_decimal() {
        let mut body = buy_body(PAYER, PAYER, 27);
        body.authorization.value = "0x2faf080".to_string(); // hex rejected by design
        let err = build_buy_request(&body).unwrap_err();
        assert!(
            matches!(err, DashboardError::Validation { ref field, .. } if field == "authorization.value")
        );
        let mut body = buy_body(PAYER, PAYER, 27);
        body.authorization.value = String::new();
        assert!(build_buy_request(&body).is_err());
    }

    #[test]
    fn buy_request_recipient_must_equal_payer_m2() {
        let other = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let err = build_buy_request(&buy_body(other, PAYER, 27)).unwrap_err();
        match err {
            DashboardError::Validation { msg, .. } => {
                assert!(
                    msg.contains("RecipientMustBePayer"),
                    "must name the revert: {msg}"
                );
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    // pinata / chain env-parsing tests moved to `crate::chain_config` with
    // the startup-resolved config (xvision-df3).

    #[tokio::test]
    async fn pin_canonical_maps_failure_to_service_unavailable() {
        // An empty-JWT driver fails fast (NotConfigured) without network;
        // the route maps every pin failure to the 503 upstream class.
        let ipfs = xvision_marketplace::PinataDriver::new("", "");
        let err = pin_canonical(&ipfs, "{}").await.unwrap_err();
        match err {
            DashboardError::ServiceUnavailable(msg) => {
                assert!(
                    msg.contains("before mint"),
                    "names the no-orphan guarantee: {msg}"
                );
            }
            other => panic!("expected ServiceUnavailable, got {other:?}"),
        }
    }

    // --- attest payload convention --------------------------------------------

    #[test]
    fn attest_payload_hash_matches_cli_convention() {
        // The CLI (xvn marketplace attest) hashes the compact serde_json
        // bytes of {"cycles":N,"sharpe":F}; the route must produce the
        // identical digest so on-chain hashes are comparable across surfaces.
        let expected = keccak256(r#"{"cycles":20,"sharpe":1.5}"#.as_bytes());
        assert_eq!(attest_payload_hash(20, 1.5), expected);
        // Different inputs → different digests (sanity).
        assert_ne!(attest_payload_hash(21, 1.5), expected);
    }

    #[test]
    fn verify_manifest_matches_onchain_accepts_matching_hash() {
        let manifest = serde_json::json!({ "b": 2, "a": 1 });
        let canonical = canonical_json(&manifest);
        let hash = manifest_hash_hex(&canonical);
        // Bare hash and 0x-prefixed/upper-case both accepted (normalized).
        assert!(verify_manifest_matches_onchain(&manifest, &hash).is_ok());
        assert!(verify_manifest_matches_onchain(&manifest, &format!("0x{}", hash.to_uppercase())).is_ok());
    }

    #[test]
    fn verify_manifest_matches_onchain_rejects_mismatch_as_409() {
        let manifest = serde_json::json!({ "a": 1 });
        let err = verify_manifest_matches_onchain(&manifest, &"ab".repeat(32)).unwrap_err();
        match err {
            DashboardError::Conflict(msg) => {
                assert!(msg.contains("does not match"), "{msg}");
            }
            other => panic!("expected Conflict (409), got {other:?}"),
        }
    }

    #[test]
    fn update_chain_error_is_validation_400() {
        let err = update_chain_error("execution reverted: NotSeller()".into());
        match err {
            DashboardError::Validation { field, msg } => {
                assert_eq!(field, "listing_id");
                assert!(msg.contains("NotSeller"), "keeps the chain text: {msg}");
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }
}

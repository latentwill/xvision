//! Mutating marketplace routes.
//!
//! - `POST /api/marketplace/publish` — mint a strategy's identity NFT with
//!   its Bitfields v3 genart tokenURI, then create the marketplace listing.
//! - `POST /api/marketplace/listings/:id/revoke` — seller-initiated revoke.
//! - `POST /api/marketplace/buy` — gasless x402 purchase relay
//!   (`buyWithAuthorization` signed by the buyer, gas paid by the relayer).
//!
//! Flow: parse/validate body → load strategy → hash → tokenURI →
//! `ChainEnv::from_env` → signer parse → `registry_addresses_from_env` →
//! `MarketplaceAddresses::from_env` → construct driver → connect + register
//! (mint) → `publish_listing`. All config errors surface before the mint so
//! no orphan NFTs are created by a missing env var.
//!
//! Chain access is env-gated: without `XVN_RPC_URL` / `XVN_CHAIN_ID` /
//! `XVN_PUBLISHER_PK` (plus registry addresses) the route returns 503 so dev
//! boxes degrade loudly. All pure logic (tier mapping, USDC scaling, env
//! gating) is unit-testable without a chain.
//!
//! Idempotency: deliberately none in v1 (testnet). Re-publishing the same
//! strategy mints a new NFT and listing; double-click protection is the
//! frontend's job. Revisit with a publish-receipt store before mainnet.

use std::fmt;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use alloy::primitives::{Address, B256, U256};
use alloy::signers::local::PrivateKeySigner;

use xvision_engine::api::strategy;
use xvision_engine::autooptimizer::content_hash::canonical_json;
use xvision_identity::{generate_token_uri, manifest_hash_hex, IdentityClient, RegistryAddresses};
use xvision_marketplace::adapter::{
    AnchorDriver, BuyRequest, Erc8004MantleDriver, PublishRequest, TransferAuthorization,
};
use xvision_marketplace::MarketplaceAddresses;

use crate::error::DashboardError;
use crate::state::AppState;

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
}

/// Chain connection env config. All three are required for any on-chain work.
struct ChainEnv {
    rpc_url: String,
    chain_id: u64,
    publisher_pk: String,
}

/// Manual Debug impl — redacts the private key so it cannot appear in logs.
impl fmt::Debug for ChainEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChainEnv")
            .field("rpc_url", &self.rpc_url)
            .field("chain_id", &self.chain_id)
            .field("publisher_pk", &"<redacted>")
            .finish()
    }
}

impl ChainEnv {
    /// Reads `XVN_RPC_URL`, `XVN_CHAIN_ID`, `XVN_PUBLISHER_PK`. Returns
    /// `None` when any is missing or `XVN_CHAIN_ID` is not a valid u64.
    fn from_env() -> Option<Self> {
        let rpc_url = std::env::var("XVN_RPC_URL").ok()?;
        let chain_id: u64 = std::env::var("XVN_CHAIN_ID").ok()?.parse().ok()?;
        let publisher_pk = std::env::var("XVN_PUBLISHER_PK").ok()?;
        Some(Self {
            rpc_url,
            chain_id,
            publisher_pk,
        })
    }
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

/// Reads the identity registry addresses from env: `XVN_IDENTITY_REGISTRY`
/// (required for minting) and `XVN_REPUTATION_REGISTRY` (optional here —
/// `register` doesn't touch it; defaults to the zero address).
fn registry_addresses_from_env() -> Option<RegistryAddresses> {
    let identity: Address = std::env::var("XVN_IDENTITY_REGISTRY").ok()?.parse().ok()?;
    let reputation: Address = std::env::var("XVN_REPUTATION_REGISTRY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(Address::ZERO);
    Some(RegistryAddresses::custom(identity, reputation))
}

/// `POST /api/marketplace/publish` — mint + list. Returns 201 on success,
/// 503 when the chain env is not configured.
pub async fn post_publish(
    State(state): State<AppState>,
    Json(body): Json<PublishBody>,
) -> Result<(StatusCode, Json<PublishOut>), DashboardError> {
    // Validate the cheap, pure inputs before touching the store or chain.
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

    // d. Genart tokenURI (data:application/json;base64,…).
    let token_uri =
        generate_token_uri(&agent_id, &manifest_hash).map_err(|e| DashboardError::Validation {
            field: "strategy_id".into(),
            msg: format!("genart tokenURI generation failed: {e}"),
        })?;

    // e. Chain gate — degrade loudly without env config. All config is
    //    validated here, before any chain write, so a missing env var cannot
    //    strand an orphan mint.
    let chain = ChainEnv::from_env().ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "chain publishing not configured: set XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK".into(),
        )
    })?;
    let signer: PrivateKeySigner = chain.publisher_pk.parse().map_err(|_| {
        DashboardError::ServiceUnavailable("XVN_PUBLISHER_PK is not a valid private key".into())
    })?;
    let registry_addresses = registry_addresses_from_env().ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "identity registry not configured: set XVN_IDENTITY_REGISTRY (and optionally \
             XVN_REPUTATION_REGISTRY)"
                .into(),
        )
    })?;
    let marketplace_addresses = MarketplaceAddresses::from_env().ok_or_else(|| {
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
        signer.clone(),
    );

    // f. Mint the identity NFT with the genart tokenURI.
    //    All config has been validated above — no config error can occur
    //    after this point.
    let identity_client = IdentityClient::connect(&chain.rpc_url, registry_addresses, chain.chain_id)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("identity connect: {e}")))?;
    let agent_uri = url::Url::parse(&token_uri)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("tokenURI is not a valid URL: {e}")))?;
    let token_id = identity_client
        .register(&agent_uri, &signer)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("identity register: {e}")))?;

    // g. Create the marketplace listing.
    let content_hash: B256 = manifest_hash
        .parse()
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("manifest hash is not valid B256 hex: {e}")))?;
    let listing = driver
        .publish_listing(PublishRequest {
            agent_nft_id: token_id.0,
            content_hash,
            content_uri: format!("xvn://strategy/{agent_id}"),
            tier,
            price_usdc,
            transferable_license: body.transferable_license,
        })
        .await
        .map_err(|e| {
            DashboardError::Internal(anyhow::anyhow!(
                "publish listing failed after minting identity NFT token_id={}: {e}",
                token_id
            ))
        })?;

    // h. 201 + receipt.
    Ok((
        StatusCode::CREATED,
        Json(PublishOut {
            agent_id,
            manifest_hash,
            token_id: token_id.to_string(),
            listing_id: listing.listing_id.to_string(),
            token_uri_bytes: token_uri.len(),
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
    State(_state): State<AppState>,
) -> Result<Json<RevokeOut>, DashboardError> {
    // a. Chain gate — all config validated before any chain write.
    let chain = ChainEnv::from_env().ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "chain not configured: set XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK".into(),
        )
    })?;
    let signer: PrivateKeySigner = chain.publisher_pk.parse().map_err(|_| {
        DashboardError::ServiceUnavailable("XVN_PUBLISHER_PK is not a valid private key".into())
    })?;
    let marketplace_addresses = MarketplaceAddresses::from_env().ok_or_else(|| {
        DashboardError::ServiceUnavailable("marketplace not configured: set XVN_LISTING_REGISTRY".into())
    })?;

    // b. Build a signing driver and call revokeListing.
    let driver = Erc8004MantleDriver::with_signer(
        marketplace_addresses,
        chain.rpc_url.clone(),
        chain.chain_id,
        signer,
    );
    let tx_hash = driver.revoke_listing(U256::from(id)).await.map_err(|e| {
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
    State(_state): State<AppState>,
    Json(body): Json<BuyBody>,
) -> Result<Json<BuyOut>, DashboardError> {
    // a. Parse + validate everything (incl. M-2) before any env/chain work.
    let req = build_buy_request(&body)?;

    // b. Chain gate — degrade loudly without env config.
    let chain = ChainEnv::from_env().ok_or_else(|| {
        DashboardError::ServiceUnavailable(
            "chain relay not configured: set XVN_RPC_URL, XVN_CHAIN_ID, XVN_PUBLISHER_PK".into(),
        )
    })?;
    let signer: PrivateKeySigner = chain.publisher_pk.parse().map_err(|_| {
        DashboardError::ServiceUnavailable("XVN_PUBLISHER_PK is not a valid private key".into())
    })?;
    let marketplace_addresses = MarketplaceAddresses::from_env().ok_or_else(|| {
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
        signer,
    );
    let receipt = driver.buy_listing(req).await.map_err(|e| {
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

    #[test]
    fn chain_env_missing_is_none() {
        // Single test owns these vars (no other test in this crate touches
        // them) so removal cannot race a sibling under parallel threads.
        std::env::remove_var("XVN_RPC_URL");
        std::env::remove_var("XVN_CHAIN_ID");
        std::env::remove_var("XVN_PUBLISHER_PK");
        assert!(ChainEnv::from_env().is_none());
    }
}

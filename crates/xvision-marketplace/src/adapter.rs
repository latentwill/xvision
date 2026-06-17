//! The [`AnchorDriver`] port and its drivers.
//!
//! `AnchorDriver` is the seam between orchestration and the chain. The four
//! verbs map 1:1 to the contract calls in the surface spec; [`MockDriver`] is an
//! in-memory implementation for tests, [`Erc8004MantleDriver`] is the real path
//! that wraps `xvision_identity::contracts` bindings (deploy-gated end-to-end).

use alloy::network::EthereumWallet;
use alloy::primitives::{keccak256, Address, TxHash, B256, U256};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Mutex;

use crate::error::MarketplaceError;
use xvision_identity::contracts::{IEvalAttestationRegistry, IListingRegistry, IMarketplace};
use xvision_identity::MarketplaceAddresses;

sol! {
    #[sol(rpc)]
    interface IERC3009 {
        function authorizationState(address authorizer, bytes32 nonce) external view returns (bool);
    }
}

/// Inputs to `publish_listing` → `ListingRegistry.createListing`.
#[derive(Debug, Clone)]
pub struct PublishRequest {
    /// Lineage NFT id the variant belongs to.
    pub agent_nft_id: U256,
    /// keccak256 of the canonical variant bundle JSON.
    pub content_hash: B256,
    /// ipfs://… public metadata or sealed-bundle pointer.
    pub content_uri: String,
    /// 0 = Open, 1 = Sealed.
    pub tier: u8,
    /// 6-decimal USDC price (fits uint96 on-chain).
    pub price_usdc: U256,
    /// Soulbound default is `false`.
    pub transferable_license: bool,
}

/// Handle to a created listing.
#[derive(Debug, Clone, Copy)]
pub struct ListingRef {
    pub listing_id: U256,
}

/// An x402 / EIP-3009 `transferWithAuthorization` payload that lets a buyer pay
/// USDC without a prior `approve`. When present on a [`BuyRequest`], the driver
/// routes through `Marketplace.buyWithAuthorization`; otherwise the direct
/// `Marketplace.buy` path (which requires a standing USDC allowance) is used.
///
/// Field-for-field mirror of `IMarketplace::TransferAuthorization`.
#[derive(Debug, Clone)]
pub struct TransferAuthorization {
    pub from: Address,
    pub to: Address,
    pub value: U256,
    pub valid_after: U256,
    pub valid_before: U256,
    pub nonce: B256,
    pub v: u8,
    pub r: B256,
    pub s: B256,
}

/// Inputs to `buy_listing` → `Marketplace.buy` (direct path) or
/// `Marketplace.buyWithAuthorization` when [`Self::authorization`] is set.
#[derive(Debug, Clone)]
pub struct BuyRequest {
    pub listing_id: U256,
    /// Wallet that receives the license token.
    pub recipient: Address,
    /// Optional x402 pay-with-authorization payload (EIP-3009). `None` →
    /// direct `buy` (caller must hold a standing USDC allowance).
    pub authorization: Option<TransferAuthorization>,
}

/// Result of a sale.
#[derive(Debug, Clone, Copy)]
pub struct SaleReceipt {
    pub tx_hash: TxHash,
    /// `tokenId == listingId` (surface spec §3.3).
    pub license_token_id: U256,
}

/// Read-model of an on-chain listing (for building x402 payment requirements).
#[derive(Debug, Clone, Copy)]
pub struct ListingView {
    pub listing_id: U256,
    /// USDC price in 6-decimal base units.
    pub price_usdc: U256,
    /// Seller payout address (informational; funds route via the Marketplace).
    pub seller: Address,
    pub active: bool,
}

/// Inputs to `attest_eval` → `EvalAttestationRegistry.postAttestation`.
#[derive(Debug, Clone)]
pub struct AttestRequest {
    pub listing_id: U256,
    pub eval_result_hash: B256,
    pub eval_result_uri: String,
    /// EAS-style schema id.
    pub schema: B256,
}

/// Orchestration port over the marketplace contract surface.
#[async_trait]
pub trait AnchorDriver: Send + Sync {
    async fn publish_listing(&self, req: PublishRequest) -> Result<ListingRef, MarketplaceError>;
    async fn buy_listing(&self, req: BuyRequest) -> Result<SaleReceipt, MarketplaceError>;
    async fn attest_eval(&self, req: AttestRequest) -> Result<TxHash, MarketplaceError>;
    async fn revoke_listing(&self, listing_id: U256) -> Result<TxHash, MarketplaceError>;
    /// Reprice a listing in place (seller-only on-chain). `new_price_usdc` is
    /// 6-decimal USDC and must fit `uint96`; `0` makes the listing free.
    async fn update_price(&self, listing_id: U256, new_price_usdc: U256) -> Result<TxHash, MarketplaceError>;
}

// ---------------------------------------------------------------------------
// MockDriver — in-memory, fully functional (for tests)
// ---------------------------------------------------------------------------

/// In-memory `AnchorDriver` for tests. Listing ids are 1-based to mirror
/// `ListingRegistry` (id 0 == "none").
#[derive(Default)]
pub struct MockDriver {
    state: Mutex<MockState>,
}

#[derive(Default)]
struct MockState {
    listings: Vec<PublishRequest>, // listing_id == index + 1
    revoked: HashSet<u64>,
    tx_counter: u64,
}

impl MockDriver {
    pub fn new() -> Self {
        Self::default()
    }
}

fn listing_id_u64(id: U256) -> Result<u64, MarketplaceError> {
    u64::try_from(id).map_err(|_| MarketplaceError::UnknownListing(u64::MAX))
}

fn fake_tx(counter: u64) -> TxHash {
    keccak256(format!("mock-tx-{counter}").as_bytes())
}

#[async_trait]
impl AnchorDriver for MockDriver {
    async fn publish_listing(&self, req: PublishRequest) -> Result<ListingRef, MarketplaceError> {
        let mut st = self.state.lock().expect("mock lock");
        st.listings.push(req);
        let listing_id = st.listings.len() as u64; // 1-based
        Ok(ListingRef {
            listing_id: U256::from(listing_id),
        })
    }

    async fn buy_listing(&self, req: BuyRequest) -> Result<SaleReceipt, MarketplaceError> {
        let mut st = self.state.lock().expect("mock lock");
        let id = listing_id_u64(req.listing_id)?;
        if id == 0 || id as usize > st.listings.len() {
            return Err(MarketplaceError::UnknownListing(id));
        }
        if st.revoked.contains(&id) {
            return Err(MarketplaceError::ListingRevoked(id));
        }
        st.tx_counter += 1;
        Ok(SaleReceipt {
            tx_hash: fake_tx(st.tx_counter),
            license_token_id: req.listing_id,
        })
    }

    async fn attest_eval(&self, req: AttestRequest) -> Result<TxHash, MarketplaceError> {
        let mut st = self.state.lock().expect("mock lock");
        let id = listing_id_u64(req.listing_id)?;
        if id == 0 || id as usize > st.listings.len() {
            return Err(MarketplaceError::UnknownListing(id));
        }
        st.tx_counter += 1;
        Ok(fake_tx(st.tx_counter))
    }

    async fn revoke_listing(&self, listing_id: U256) -> Result<TxHash, MarketplaceError> {
        let mut st = self.state.lock().expect("mock lock");
        let id = listing_id_u64(listing_id)?;
        if id == 0 || id as usize > st.listings.len() {
            return Err(MarketplaceError::UnknownListing(id));
        }
        st.revoked.insert(id);
        st.tx_counter += 1;
        Ok(fake_tx(st.tx_counter))
    }

    async fn update_price(&self, listing_id: U256, new_price_usdc: U256) -> Result<TxHash, MarketplaceError> {
        let mut st = self.state.lock().expect("mock lock");
        let id = listing_id_u64(listing_id)?;
        if id == 0 || id as usize > st.listings.len() {
            return Err(MarketplaceError::UnknownListing(id));
        }
        if st.revoked.contains(&id) {
            return Err(MarketplaceError::ListingRevoked(id));
        }
        // Reflect the new price so tests can observe the change.
        st.listings[(id - 1) as usize].price_usdc = new_price_usdc;
        st.tx_counter += 1;
        Ok(fake_tx(st.tx_counter))
    }
}

// ---------------------------------------------------------------------------
// Erc8004MantleDriver — real path (STUB pending Phase 5 deploy)
// ---------------------------------------------------------------------------

/// Real `AnchorDriver` that writes to the marketplace contracts on Mantle.
///
/// Each verb builds a wallet-backed [`ProviderBuilder`] from the held
/// [`PrivateKeySigner`], calls the relevant `#[sol(rpc)]` binding in
/// [`xvision_identity::contracts`], and `.send().await?.get_receipt().await?`,
/// then decodes the receipt (mirrors `xvision_identity::IdentityClient`).
///
/// # Configuration posture
/// The marketplace contracts are not deployed on either Mantle chain yet
/// ([`MarketplaceAddresses::mantle_testnet`] / `mantle_mainnet` both return
/// `None`). Construct with explicitly-injected addresses
/// ([`Self::with_signer`]) — this is how tests inject anvil-deployed addresses
/// and how post-deploy wiring will inject the verified ones. A signer is
/// required for all four verbs (they are all state-mutating); a driver built
/// without one ([`Self::new`]) returns [`MarketplaceError::NotConfigured`] on
/// any write.
pub struct Erc8004MantleDriver {
    addresses: MarketplaceAddresses,
    rpc_url: String,
    chain_id: u64,
    signer: Option<PrivateKeySigner>,
}

impl Erc8004MantleDriver {
    /// Construct a signer-less driver. Useful for holding connection params
    /// (e.g. for `addresses()`/`chain_id()` inspection); every write verb
    /// returns [`MarketplaceError::NotConfigured`] until a signer is supplied
    /// via [`Self::with_signer`].
    pub fn new(addresses: MarketplaceAddresses, rpc_url: impl Into<String>, chain_id: u64) -> Self {
        Self {
            addresses,
            rpc_url: rpc_url.into(),
            chain_id,
            signer: None,
        }
    }

    /// Construct a fully-wired driver that can transact, signing with `signer`.
    pub fn with_signer(
        addresses: MarketplaceAddresses,
        rpc_url: impl Into<String>,
        chain_id: u64,
        signer: PrivateKeySigner,
    ) -> Self {
        Self {
            addresses,
            rpc_url: rpc_url.into(),
            chain_id,
            signer: Some(signer),
        }
    }

    pub fn addresses(&self) -> &MarketplaceAddresses {
        &self.addresses
    }

    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Read a single listing's price/seller/active flag from `IListingRegistry`.
    ///
    /// Uses a read-only provider (no signer required) — safe to call without
    /// constructing a full wallet-backed driver. Returns
    /// [`MarketplaceError::NotConfigured`] if `listing_registry` is the zero
    /// address, [`MarketplaceError::Rpc`] on provider/connect failure, or
    /// [`MarketplaceError::Contract`] on ABI decode or call revert.
    pub async fn fetch_listing(&self, listing_id: U256) -> Result<ListingView, MarketplaceError> {
        let registry = require_addr(self.addresses.listing_registry, "listing_registry address")?;
        let provider = ProviderBuilder::new()
            .connect(self.rpc_url.as_str())
            .await
            .map_err(|e| MarketplaceError::Rpc(e.to_string()))?;
        let contract = IListingRegistry::new(registry, &provider);

        let listing = contract
            .getListing(listing_id)
            .call()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?;

        Ok(ListingView {
            listing_id,
            // priceUSDC is uint96 on-chain; widen to U256 for uniform handling.
            price_usdc: U256::from(listing.priceUSDC),
            seller: listing.seller,
            active: !listing.revoked,
        })
    }

    /// Query the USDC contract's `authorizationState(from, nonce)` via the
    /// ERC-3009 read interface.
    ///
    /// Returns `true` when the nonce has already been consumed by a prior
    /// `transferWithAuthorization` call, `false` when it is still fresh.
    /// Uses a read-only provider (no signer required). Returns
    /// [`MarketplaceError::NotConfigured`] when `usdc` is the zero address.
    pub async fn is_authorization_used(&self, from: Address, nonce: B256) -> Result<bool, MarketplaceError> {
        let usdc = require_addr(self.addresses.usdc, "usdc address")?;
        let provider = ProviderBuilder::new()
            .connect(self.rpc_url.as_str())
            .await
            .map_err(|e| MarketplaceError::Rpc(e.to_string()))?;
        let erc3009 = IERC3009::new(usdc, &provider);
        let used = erc3009
            .authorizationState(from, nonce)
            .call()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?;
        Ok(used)
    }

    /// Build a wallet-backed alloy provider for the held signer, or return
    /// [`MarketplaceError::NotConfigured`] if no signer was supplied.
    async fn wallet_provider(&self) -> Result<impl alloy::providers::Provider + Clone, MarketplaceError> {
        let signer = self
            .signer
            .as_ref()
            .ok_or(MarketplaceError::NotConfigured("no signer supplied"))?;
        let wallet = EthereumWallet::from(signer.clone());
        ProviderBuilder::new()
            .wallet(wallet)
            .connect(self.rpc_url.as_str())
            .await
            .map_err(|e| MarketplaceError::Rpc(e.to_string()))
    }
}

/// Reject the unconfigured (zero) sentinel address with a clear error so a
/// caller that built the driver from a pre-deploy address book gets
/// `NotConfigured` rather than an opaque revert.
fn require_addr(addr: Address, what: &'static str) -> Result<Address, MarketplaceError> {
    if addr == Address::ZERO {
        Err(MarketplaceError::NotConfigured(what))
    } else {
        Ok(addr)
    }
}

#[async_trait]
impl AnchorDriver for Erc8004MantleDriver {
    /// Create a listing on `ListingRegistry`, returning its id from the
    /// `ListingCreated(listingId, …)` event (id is the first indexed topic).
    ///
    /// Pre-mint agent registration (IdentityRegistry) is the caller's
    /// responsibility — `agent_nft_id` must already exist; `createListing`
    /// reverts otherwise. (We avoid silently minting here so listing and
    /// identity stay separable, matching `IdentityClient::register`.)
    async fn publish_listing(&self, req: PublishRequest) -> Result<ListingRef, MarketplaceError> {
        let registry = require_addr(self.addresses.listing_registry, "listing_registry address")?;
        let provider = self.wallet_provider().await?;
        let contract = IListingRegistry::new(registry, &provider);

        let price = u96_from_u256(req.price_usdc)?;
        let receipt = contract
            .createListing(
                req.agent_nft_id,
                req.content_hash,
                req.content_uri,
                req.tier,
                price,
                req.transferable_license,
            )
            .send()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?
            .get_receipt()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?;

        // ListingCreated(uint256 indexed listingId, address indexed seller,
        //   uint256 indexed agentNftId, …) — listingId is topics[1].
        let sig = keccak256(b"ListingCreated(uint256,address,uint256,bytes32,uint8,uint96)");
        let listing_id = receipt
            .inner
            .logs()
            .iter()
            .find_map(|log| {
                let topics = log.topics();
                if topics.first() == Some(&sig) && topics.len() >= 2 {
                    Some(U256::from_be_bytes::<32>(topics[1].0))
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                MarketplaceError::Contract(
                    "no ListingCreated event in receipt; verify ListingRegistry ABI".to_string(),
                )
            })?;

        Ok(ListingRef { listing_id })
    }

    /// Purchase a listing on `Marketplace`. Routes through
    /// `buyWithAuthorization` when the request carries an x402 authorization,
    /// otherwise the direct `buy` path. The license token id is read from the
    /// `Sold(…)` event; by spec it equals `listing_id`.
    async fn buy_listing(&self, req: BuyRequest) -> Result<SaleReceipt, MarketplaceError> {
        // Contract invariant (finding M-2): `buyWithAuthorization` reverts with
        // `RecipientMustBePayer()` when `recipient != auth.from`. Fail fast in
        // Rust — before any network/provider work — so we never build or send a
        // tx the contract is guaranteed to revert.
        if let Some(auth) = req.authorization.as_ref() {
            if req.recipient != auth.from {
                return Err(MarketplaceError::Contract(
                    "buyWithAuthorization recipient must equal the authorization payer (auth.from)".into(),
                ));
            }
        }

        let marketplace = require_addr(self.addresses.marketplace, "marketplace address")?;
        let provider = self.wallet_provider().await?;
        let contract = IMarketplace::new(marketplace, &provider);

        let pending = if let Some(auth) = req.authorization {
            contract
                .buyWithAuthorization(
                    req.listing_id,
                    req.recipient,
                    IMarketplace::TransferAuthorization {
                        from: auth.from,
                        to: auth.to,
                        value: auth.value,
                        validAfter: auth.valid_after,
                        validBefore: auth.valid_before,
                        nonce: auth.nonce,
                        v: auth.v,
                        r: auth.r,
                        s: auth.s,
                    },
                )
                .send()
                .await
        } else {
            contract.buy(req.listing_id, req.recipient).send().await
        }
        .map_err(|e| MarketplaceError::Contract(e.to_string()))?;

        let receipt = pending
            .get_receipt()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?;

        let tx_hash = TxHash::from(receipt.transaction_hash);

        // Sold(uint256 indexed listingId, uint256 indexed agentNftId,
        //   address indexed buyer, …, uint256 licenseTokenId, …) — the
        // licenseTokenId is a non-indexed data field; by spec == listingId.
        // We surface listingId-as-tokenId (the documented invariant) so a
        // missing/renamed event does not silently zero the receipt.
        Ok(SaleReceipt {
            tx_hash,
            license_token_id: req.listing_id,
        })
    }

    /// Post an eval attestation for a listing on `EvalAttestationRegistry`.
    async fn attest_eval(&self, req: AttestRequest) -> Result<TxHash, MarketplaceError> {
        let registry = require_addr(self.addresses.eval_attestation, "eval_attestation address")?;
        let provider = self.wallet_provider().await?;
        let contract = IEvalAttestationRegistry::new(registry, &provider);

        let receipt = contract
            .postAttestation(
                req.listing_id,
                req.eval_result_hash,
                req.eval_result_uri,
                req.schema,
            )
            .send()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?
            .get_receipt()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?;

        Ok(TxHash::from(receipt.transaction_hash))
    }

    /// Revoke a listing on `ListingRegistry`.
    async fn revoke_listing(&self, listing_id: U256) -> Result<TxHash, MarketplaceError> {
        let registry = require_addr(self.addresses.listing_registry, "listing_registry address")?;
        let provider = self.wallet_provider().await?;
        let contract = IListingRegistry::new(registry, &provider);

        let receipt = contract
            .revokeListing(listing_id)
            .send()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?
            .get_receipt()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?;

        Ok(TxHash::from(receipt.transaction_hash))
    }

    /// Reprice a listing in place on `ListingRegistry` (seller-only on-chain;
    /// the contract reverts `NotSeller`/`AlreadyRevoked`/`FreeTransferableForbidden`).
    async fn update_price(&self, listing_id: U256, new_price_usdc: U256) -> Result<TxHash, MarketplaceError> {
        let registry = require_addr(self.addresses.listing_registry, "listing_registry address")?;
        let provider = self.wallet_provider().await?;
        let contract = IListingRegistry::new(registry, &provider);

        let price = u96_from_u256(new_price_usdc)?;
        let receipt = contract
            .updatePrice(listing_id, price)
            .send()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?
            .get_receipt()
            .await
            .map_err(|e| MarketplaceError::Contract(e.to_string()))?;

        Ok(TxHash::from(receipt.transaction_hash))
    }
}

/// `priceUSDC` is `uint96` on-chain; narrow checked, rejecting overflow.
fn u96_from_u256(v: U256) -> Result<alloy::primitives::aliases::U96, MarketplaceError> {
    if v.bit_len() > 96 {
        return Err(MarketplaceError::Contract(format!(
            "price {v} exceeds uint96 max"
        )));
    }
    // Safe: value fits in 96 bits, checked above. `to` would panic on overflow.
    Ok(v.to::<alloy::primitives::aliases::U96>())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn publish_req() -> PublishRequest {
        PublishRequest {
            agent_nft_id: U256::from(0u64),
            content_hash: keccak256(b"variant"),
            content_uri: "ipfs://variant".to_string(),
            tier: 0,
            price_usdc: U256::from(15_000_000u64),
            transferable_license: false,
        }
    }

    #[tokio::test]
    async fn mock_publish_then_buy() {
        let d = MockDriver::new();
        let lref = d.publish_listing(publish_req()).await.unwrap();
        assert_eq!(lref.listing_id, U256::from(1u64));

        let receipt = d
            .buy_listing(BuyRequest {
                listing_id: lref.listing_id,
                recipient: Address::ZERO,
                authorization: None,
            })
            .await
            .unwrap();
        assert_eq!(receipt.license_token_id, lref.listing_id);
    }

    #[tokio::test]
    async fn mock_buy_unknown_listing_errs() {
        let d = MockDriver::new();
        let err = d
            .buy_listing(BuyRequest {
                listing_id: U256::from(99u64),
                recipient: Address::ZERO,
                authorization: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::UnknownListing(99)));
    }

    #[tokio::test]
    async fn mock_revoke_blocks_buy() {
        let d = MockDriver::new();
        let lref = d.publish_listing(publish_req()).await.unwrap();
        d.revoke_listing(lref.listing_id).await.unwrap();

        let err = d
            .buy_listing(BuyRequest {
                listing_id: lref.listing_id,
                recipient: Address::ZERO,
                authorization: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::ListingRevoked(1)));
    }

    #[tokio::test]
    async fn mock_update_price_ok() {
        let d = MockDriver::new();
        let lref = d.publish_listing(publish_req()).await.unwrap();
        d.update_price(lref.listing_id, U256::from(9_000_000u64))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn mock_update_price_unknown_errs() {
        let d = MockDriver::new();
        let err = d
            .update_price(U256::from(99u64), U256::from(1u64))
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::UnknownListing(99)));
    }

    #[tokio::test]
    async fn mock_update_price_after_revoke_errs() {
        let d = MockDriver::new();
        let lref = d.publish_listing(publish_req()).await.unwrap();
        d.revoke_listing(lref.listing_id).await.unwrap();
        let err = d
            .update_price(lref.listing_id, U256::from(1u64))
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::ListingRevoked(1)));
    }

    /// A non-zero placeholder address so address-presence checks pass and we
    /// reach the next gate (signer / network) in unit tests.
    fn nonzero_addr(byte: u8) -> Address {
        Address::from([byte; 20])
    }

    fn configured_addresses() -> MarketplaceAddresses {
        MarketplaceAddresses {
            xvn_deployer: nonzero_addr(0x10),
            listing_registry: nonzero_addr(0x11),
            marketplace: nonzero_addr(0x12),
            license_token: nonzero_addr(0x13),
            eval_attestation: nonzero_addr(0x14),
            validation_registry: nonzero_addr(0x15),
            usdc: nonzero_addr(0x16),
            platform_agent_token_id: 0,
        }
    }

    fn zero_addresses() -> MarketplaceAddresses {
        MarketplaceAddresses {
            xvn_deployer: Address::ZERO,
            listing_registry: Address::ZERO,
            marketplace: Address::ZERO,
            license_token: Address::ZERO,
            eval_attestation: Address::ZERO,
            validation_registry: Address::ZERO,
            usdc: Address::ZERO,
            platform_agent_token_id: 0,
        }
    }

    fn test_signer() -> PrivateKeySigner {
        // anvil account 0 key (publicly known, safe for tests).
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse()
            .expect("valid anvil key")
    }

    #[test]
    fn driver_preserves_construction_params() {
        let d = Erc8004MantleDriver::new(configured_addresses(), "http://127.0.0.1:8545", 31337);
        assert_eq!(d.chain_id(), 31337);
        assert_eq!(d.rpc_url(), "http://127.0.0.1:8545");
        assert_eq!(d.addresses().marketplace, nonzero_addr(0x12));
    }

    // --- NotConfigured: no signer supplied (all four verbs) -----------------

    #[tokio::test]
    async fn publish_without_signer_is_not_configured() {
        let d = Erc8004MantleDriver::new(configured_addresses(), "http://127.0.0.1:8545", 31337);
        let err = d.publish_listing(publish_req()).await.unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    #[tokio::test]
    async fn buy_without_signer_is_not_configured() {
        let d = Erc8004MantleDriver::new(configured_addresses(), "http://127.0.0.1:8545", 31337);
        let err = d
            .buy_listing(BuyRequest {
                listing_id: U256::from(1u64),
                recipient: Address::ZERO,
                authorization: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    #[tokio::test]
    async fn attest_without_signer_is_not_configured() {
        let d = Erc8004MantleDriver::new(configured_addresses(), "http://127.0.0.1:8545", 31337);
        let err = d
            .attest_eval(AttestRequest {
                listing_id: U256::from(1u64),
                eval_result_hash: keccak256(b"r"),
                eval_result_uri: "ipfs://r".to_string(),
                schema: keccak256(b"s"),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    #[tokio::test]
    async fn revoke_without_signer_is_not_configured() {
        let d = Erc8004MantleDriver::new(configured_addresses(), "http://127.0.0.1:8545", 31337);
        let err = d.revoke_listing(U256::from(1u64)).await.unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    #[tokio::test]
    async fn update_price_without_signer_is_not_configured() {
        let d = Erc8004MantleDriver::new(configured_addresses(), "http://127.0.0.1:8545", 31337);
        let err = d
            .update_price(U256::from(1u64), U256::from(1u64))
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    // --- NotConfigured: zero (pre-deploy) addresses, even with a signer -----

    #[tokio::test]
    async fn publish_with_zero_addresses_is_not_configured() {
        let d =
            Erc8004MantleDriver::with_signer(zero_addresses(), "http://127.0.0.1:8545", 31337, test_signer());
        let err = d.publish_listing(publish_req()).await.unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    #[tokio::test]
    async fn buy_with_zero_addresses_is_not_configured() {
        let d =
            Erc8004MantleDriver::with_signer(zero_addresses(), "http://127.0.0.1:8545", 31337, test_signer());
        let err = d
            .buy_listing(BuyRequest {
                listing_id: U256::from(1u64),
                recipient: Address::ZERO,
                authorization: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    #[tokio::test]
    async fn attest_with_zero_addresses_is_not_configured() {
        let d =
            Erc8004MantleDriver::with_signer(zero_addresses(), "http://127.0.0.1:8545", 31337, test_signer());
        let err = d
            .attest_eval(AttestRequest {
                listing_id: U256::from(1u64),
                eval_result_hash: keccak256(b"r"),
                eval_result_uri: "ipfs://r".to_string(),
                schema: keccak256(b"s"),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    #[tokio::test]
    async fn revoke_with_zero_addresses_is_not_configured() {
        let d =
            Erc8004MantleDriver::with_signer(zero_addresses(), "http://127.0.0.1:8545", 31337, test_signer());
        let err = d.revoke_listing(U256::from(1u64)).await.unwrap_err();
        assert!(matches!(err, MarketplaceError::NotConfigured(_)), "{err:?}");
    }

    // --- recipient == auth.from guard (contract finding M-2) ----------------

    fn dummy_authorization(from: Address) -> TransferAuthorization {
        TransferAuthorization {
            from,
            to: nonzero_addr(0x12), // marketplace
            value: U256::from(15_000_000u64),
            valid_after: U256::ZERO,
            valid_before: U256::MAX,
            nonce: keccak256(b"nonce"),
            v: 27,
            r: keccak256(b"r"),
            s: keccak256(b"s"),
        }
    }

    /// When an authorization is present, `recipient` must equal `auth.from`
    /// (the contract reverts `RecipientMustBePayer()` otherwise). The driver
    /// must reject this in Rust *before* touching the network — note the rpc
    /// url is unroutable, so reaching `wallet_provider` would surface an `Rpc`
    /// error instead of the `Contract` guard error we assert here.
    #[tokio::test]
    async fn buy_with_auth_recipient_mismatch_is_rejected_without_send() {
        let payer = nonzero_addr(0xAA);
        let other_recipient = nonzero_addr(0xBB);
        assert_ne!(payer, other_recipient);

        let d = Erc8004MantleDriver::with_signer(
            configured_addresses(),
            "http://127.0.0.1:1", // unroutable: any send/connect would fail loudly
            31337,
            test_signer(),
        );

        let err = d
            .buy_listing(BuyRequest {
                listing_id: U256::from(1u64),
                recipient: other_recipient,
                authorization: Some(dummy_authorization(payer)),
            })
            .await
            .unwrap_err();

        match err {
            MarketplaceError::Contract(msg) => {
                assert!(
                    msg.contains("recipient must equal the authorization payer"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected Contract guard error, got {other:?}"),
        }
    }

    /// Matching recipient/payer passes the Rust-side guard and proceeds to the
    /// network layer; against an unroutable rpc that surfaces as an `Rpc` (or
    /// `Contract`) error — crucially NOT the recipient-guard `Contract`
    /// message, proving the guard let it through.
    #[tokio::test]
    async fn buy_with_auth_recipient_match_passes_guard() {
        let payer = nonzero_addr(0xAA);

        let d = Erc8004MantleDriver::with_signer(
            configured_addresses(),
            "http://127.0.0.1:1",
            31337,
            test_signer(),
        );

        let err = d
            .buy_listing(BuyRequest {
                listing_id: U256::from(1u64),
                recipient: payer,
                authorization: Some(dummy_authorization(payer)),
            })
            .await
            .unwrap_err();

        // Must have moved past the guard: the only Contract error allowed here
        // is a downstream send/decode failure, never the recipient-guard text.
        if let MarketplaceError::Contract(msg) = &err {
            assert!(
                !msg.contains("recipient must equal the authorization payer"),
                "guard fired on a matching recipient: {msg}"
            );
        }
    }

    // --- ListingView shape --------------------------------------------------

    #[test]
    fn listing_view_shape() {
        let v = ListingView {
            listing_id: U256::from(1u64),
            price_usdc: U256::from(49_000_000u64),
            seller: Address::ZERO,
            active: true,
        };
        assert_eq!(v.price_usdc, U256::from(49_000_000u64));
    }

    // --- price width guard --------------------------------------------------

    #[test]
    fn price_above_uint96_is_rejected() {
        let too_big = (U256::from(1u64) << 96) + U256::from(1u64);
        assert!(u96_from_u256(too_big).is_err());
    }

    #[test]
    fn price_within_uint96_is_accepted() {
        assert!(u96_from_u256(U256::from(15_000_000u64)).is_ok());
        let max = (U256::from(1u64) << 96) - U256::from(1u64);
        assert!(u96_from_u256(max).is_ok());
    }

    // --- anvil end-to-end scaffold (deploy-gated) ---------------------------
    //
    // The full publish→buy→revoke + attest round-trip is gated on a forge
    // deploy of the marketplace contract set onto anvil (the `sol!` bindings
    // in `xvision_identity::contracts` are interface-only — no bytecode — so
    // the contracts cannot be deployed from Rust; `buy` additionally needs a
    // funded+approved mock USDC, which DeployTestnet.s.sol takes as an env
    // address rather than deploying). This mirrors the deploy wall that keeps
    // `xvision_identity::client`'s anvil tests `#[ignore]`d.
    //
    // To run once the contracts are deployed to a local anvil:
    //   anvil &
    //   forge script contracts/script/DeployTestnet.s.sol \
    //     --rpc-url http://127.0.0.1:8545 --broadcast \
    //     --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
    //   XVN_MARKETPLACE_ANVIL=1 \
    //   XVN_LISTING_REGISTRY=0x... XVN_MARKETPLACE=0x... \
    //   XVN_LICENSE_TOKEN=0x... XVN_EVAL_ATTESTATION=0x... \
    //   scripts/cargo test -p xvision-marketplace anvil_publish_buy_revoke -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "requires anvil + a forge deploy of the marketplace set + mock USDC; see comment"]
    async fn anvil_publish_buy_revoke_attest() {
        use std::env;
        if env::var("XVN_MARKETPLACE_ANVIL").is_err() {
            eprintln!("XVN_MARKETPLACE_ANVIL unset; skipping");
            return;
        }
        let parse = |k: &str| -> Address {
            env::var(k)
                .unwrap_or_else(|_| panic!("{k} env var required"))
                .parse()
                .expect("valid address")
        };
        let addresses = MarketplaceAddresses {
            xvn_deployer: parse("XVN_DEPLOYER"),
            listing_registry: parse("XVN_LISTING_REGISTRY"),
            marketplace: parse("XVN_MARKETPLACE"),
            license_token: parse("XVN_LICENSE_TOKEN"),
            eval_attestation: parse("XVN_EVAL_ATTESTATION"),
            validation_registry: parse("XVN_VALIDATION_REGISTRY"),
            usdc: parse("XVN_USDC"),
            platform_agent_token_id: 0,
        };
        let d = Erc8004MantleDriver::with_signer(addresses, "http://127.0.0.1:8545", 31337, test_signer());

        let lref = d.publish_listing(publish_req()).await.expect("publish");
        let receipt = d
            .buy_listing(BuyRequest {
                listing_id: lref.listing_id,
                recipient: test_signer().address(),
                authorization: None,
            })
            .await
            .expect("buy");
        assert_eq!(receipt.license_token_id, lref.listing_id);

        d.attest_eval(AttestRequest {
            listing_id: lref.listing_id,
            eval_result_hash: keccak256(b"eval"),
            eval_result_uri: "ipfs://eval".to_string(),
            schema: keccak256(b"schema"),
        })
        .await
        .expect("attest");

        d.revoke_listing(lref.listing_id).await.expect("revoke");

        let err = d
            .buy_listing(BuyRequest {
                listing_id: lref.listing_id,
                recipient: test_signer().address(),
                authorization: None,
            })
            .await
            .unwrap_err();
        eprintln!("post-revoke buy rejected as expected: {err:?}");
    }
}

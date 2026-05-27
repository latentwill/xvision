//! The [`AnchorDriver`] port and its drivers.
//!
//! `AnchorDriver` is the seam between orchestration and the chain. The four
//! verbs map 1:1 to the contract calls in the surface spec; [`MockDriver`] is an
//! in-memory implementation for tests, [`Erc8004MantleDriver`] is the (stubbed)
//! real path that wraps `xvision_identity::contracts` bindings.

use alloy::primitives::{keccak256, Address, TxHash, B256, U256};
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Mutex;

use crate::error::MarketplaceError;
use xvision_identity::MarketplaceAddresses;

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

/// Inputs to `buy_listing` → `Marketplace.buy` (direct path).
#[derive(Debug, Clone)]
pub struct BuyRequest {
    pub listing_id: U256,
    /// Wallet that receives the license token.
    pub recipient: Address,
}

/// Result of a sale.
#[derive(Debug, Clone, Copy)]
pub struct SaleReceipt {
    pub tx_hash: TxHash,
    /// `tokenId == listingId` (surface spec §3.3).
    pub license_token_id: U256,
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
}

// ---------------------------------------------------------------------------
// Erc8004MantleDriver — real path (STUB pending Phase 5 deploy)
// ---------------------------------------------------------------------------

/// Real `AnchorDriver` that writes to the marketplace contracts on Mantle.
///
/// STUB: carries the deployed addresses and connection params, but its verbs
/// return [`MarketplaceError::NotImplemented`] until the contracts are deployed
/// (Phase 3/5). The `alloy::sol!` bindings it will use already exist in
/// [`xvision_identity::contracts`].
pub struct Erc8004MantleDriver {
    addresses: MarketplaceAddresses,
    rpc_url: String,
    chain_id: u64,
}

impl Erc8004MantleDriver {
    pub fn new(addresses: MarketplaceAddresses, rpc_url: impl Into<String>, chain_id: u64) -> Self {
        Self {
            addresses,
            rpc_url: rpc_url.into(),
            chain_id,
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
}

#[async_trait]
impl AnchorDriver for Erc8004MantleDriver {
    async fn publish_listing(&self, _req: PublishRequest) -> Result<ListingRef, MarketplaceError> {
        // TODO(Phase 5): connect a wallet provider, call
        // IListingRegistry::createListing, read listing id from ListingCreated.
        Err(MarketplaceError::NotImplemented(
            "Erc8004MantleDriver::publish_listing",
        ))
    }

    async fn buy_listing(&self, _req: BuyRequest) -> Result<SaleReceipt, MarketplaceError> {
        // TODO(Phase 5): IMarketplace::buy (direct) or buyWithAuthorization (x402).
        Err(MarketplaceError::NotImplemented(
            "Erc8004MantleDriver::buy_listing",
        ))
    }

    async fn attest_eval(&self, _req: AttestRequest) -> Result<TxHash, MarketplaceError> {
        // TODO(Phase 5): IEvalAttestationRegistry::postAttestation.
        Err(MarketplaceError::NotImplemented(
            "Erc8004MantleDriver::attest_eval",
        ))
    }

    async fn revoke_listing(&self, _listing_id: U256) -> Result<TxHash, MarketplaceError> {
        // TODO(Phase 5): IListingRegistry::revokeListing.
        Err(MarketplaceError::NotImplemented(
            "Erc8004MantleDriver::revoke_listing",
        ))
    }
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
            })
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::ListingRevoked(1)));
    }

    #[tokio::test]
    async fn real_driver_is_stubbed() {
        let d = Erc8004MantleDriver::new(
            xvision_identity::MarketplaceAddresses {
                xvn_deployer: Address::ZERO,
                listing_registry: Address::ZERO,
                marketplace: Address::ZERO,
                license_token: Address::ZERO,
                eval_attestation: Address::ZERO,
                validation_registry: Address::ZERO,
                usdc: Address::ZERO,
                platform_agent_token_id: 0,
            },
            "http://127.0.0.1:8545",
            31337,
        );
        assert_eq!(d.chain_id(), 31337);
        let err = d
            .buy_listing(BuyRequest {
                listing_id: U256::from(1u64),
                recipient: Address::ZERO,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, MarketplaceError::NotImplemented(_)));
    }
}

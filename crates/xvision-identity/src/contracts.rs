//! `alloy::sol!` bindings for the full marketplace contract surface
//! (surface spec §8.2: "`xvision-identity` holds `alloy::sol!` bindings for
//! **all** on-chain contracts").
//!
//! # ABI status — STUB
//!
//! These mirror the Solidity in `contracts/src/` 1:1, written ahead of any
//! deployment. They are NOT compiled against a live ABI yet. Once the contracts
//! are deployed to Mantle Sepolia and verified, pin the verified ABI JSON under
//! `crates/xvision-identity/abi/v1/` and regenerate (surface spec §8.5). No
//! cross-chain addresses are wired (`MarketplaceAddresses::mantle_*` return
//! `None`) until Phase 3/5 deploy.
//!
//! The higher-level orchestration verbs (`publish_listing`, `buy_listing`,
//! `attest_eval`, `revoke_listing`) live in the `xvision-marketplace` crate,
//! which wraps these bindings. This module is bindings-only.

use alloy::{primitives::Address, sol};

// ---------------------------------------------------------------------------
// Solidity bindings (stub — replace with verified ABIs post-deploy)
// ---------------------------------------------------------------------------

sol! {
    /// ValidationRegistry — ERC-8004 §3.3 (per-trade proofs + attester receipts).
    #[sol(rpc)]
    interface IValidationRegistry {
        function postValidation(
            uint256 agentId,
            bytes32 resultHash,
            string  calldata resultURI,
            string  calldata tag
        ) external;

        function getValidation(uint256 agentId, uint256 index)
            external view
            returns (
                address validator,
                bytes32 resultHash,
                string  memory resultURI,
                string  memory tag,
                uint256 timestamp
            );

        function getValidationCount(uint256 agentId) external view returns (uint256);

        event ValidationPosted(
            uint256 indexed agentId,
            address indexed validator,
            bytes32 resultHash,
            string  tag
        );
    }

    /// ListingRegistry — listing CRUD (surface spec §3.1).
    #[sol(rpc)]
    interface IListingRegistry {
        struct Listing {
            uint256 listingId;
            address seller;
            uint256 agentNftId;
            bytes32 contentHash;
            string  contentURI;
            uint8   tier;
            uint96  priceUSDC;
            uint16  protocolFeeBps;
            bool    transferableLicense;
            uint64  createdAt;
            bool    revoked;
        }

        function createListing(
            uint256 agentNftId,
            bytes32 contentHash,
            string  calldata contentURI,
            uint8   tier,
            uint96  priceUSDC,
            bool    transferableLicense
        ) external returns (uint256 listingId);

        function updateListing(uint256 listingId, bytes32 contentHash, string calldata contentURI) external;
        function updatePrice(uint256 listingId, uint96 newPriceUSDC) external;
        function revokeListing(uint256 listingId) external;
        function getListing(uint256 listingId) external view returns (Listing memory);
        function totalListings() external view returns (uint256);
        function transferableForListing(uint256 listingId) external view returns (bool);
        function listingExists(uint256 listingId) external view returns (bool);

        event ListingCreated(
            uint256 indexed listingId,
            address indexed seller,
            uint256 indexed agentNftId,
            bytes32 contentHash,
            uint8   tier,
            uint96  priceUSDC
        );
        event ListingUpdated(uint256 indexed listingId, bytes32 contentHash, string contentURI);
        event ListingPriceUpdated(uint256 indexed listingId, uint96 oldPriceUSDC, uint96 newPriceUSDC);
        event ListingRevoked(uint256 indexed listingId, address indexed seller);
    }

    /// LicenseToken — ERC-1155 license (surface spec §3.3).
    #[sol(rpc)]
    interface ILicenseToken {
        function authorizedMint(address to, uint256 listingId, uint256 amount) external;
        function isAuthorized(address account) external view returns (bool);
        function setAuthorized(address account, bool allowed) external;
        function transferableForId(uint256 listingId) external view returns (bool);
        function balanceOf(address account, uint256 id) external view returns (uint256);

        event AuthorizedSet(address indexed caller, bool allowed);
    }

    /// Marketplace — sale + commission split (surface spec §3.2).
    #[sol(rpc)]
    interface IMarketplace {
        struct TransferAuthorization {
            address from;
            address to;
            uint256 value;
            uint256 validAfter;
            uint256 validBefore;
            bytes32 nonce;
            uint8   v;
            bytes32 r;
            bytes32 s;
        }

        function buy(uint256 listingId, address recipient) external returns (uint256 licenseTokenId);
        function buyWithAuthorization(
            uint256 listingId,
            address recipient,
            TransferAuthorization calldata auth
        ) external returns (uint256 licenseTokenId);
        function setProtocolFeeBps(uint16 newBps) external;
        function setFeeRecipient(address newRecipient) external;
        function protocolFeeBps() external view returns (uint16);
        function feeRecipient() external view returns (address);

        event Sold(
            uint256 indexed listingId,
            uint256 indexed agentNftId,
            address indexed buyer,
            uint96  priceUSDC,
            uint96  sellerProceeds,
            uint96  protocolProceeds,
            uint256 licenseTokenId,
            uint16  payerKind,  // v1 placeholder (mirrors purchasePath); see Marketplace.sol
            uint8   purchasePath
        );
    }

    /// EvalAttestationRegistry — eval attestations per listing (surface spec §3.4).
    #[sol(rpc)]
    interface IEvalAttestationRegistry {
        struct Attestation {
            bytes32 evalResultHash;
            string  evalResultURI;
            address attester;
            uint64  postedAt;
            bytes32 schema;
        }

        function postAttestation(
            uint256 listingId,
            bytes32 evalResultHash,
            string  calldata evalResultURI,
            bytes32 schema
        ) external;
        function getAttestations(uint256 listingId) external view returns (Attestation[] memory);
        function getAttestationCount(uint256 listingId) external view returns (uint256);

        event AttestationPosted(
            uint256 indexed listingId,
            address indexed attester,
            bytes32 evalResultHash,
            bytes32 schema
        );
    }
}

// ---------------------------------------------------------------------------
// Address book
// ---------------------------------------------------------------------------

/// On-chain addresses for the marketplace contract set + the platform agent id.
///
/// Mirrors `config/mantle*.toml`'s `[marketplace]` block (surface spec §8.4).
/// Both `mantle_*` constructors return `None` until Phase 3/5 deploy lands
/// real addresses — the same posture as [`crate::RegistryAddresses`].
#[derive(Debug, Clone)]
pub struct MarketplaceAddresses {
    pub xvn_deployer: Address,
    pub listing_registry: Address,
    pub marketplace: Address,
    pub license_token: Address,
    pub eval_attestation: Address,
    pub validation_registry: Address,
    /// USDC.e on the target chain (the only sale currency in v1).
    pub usdc: Address,
    /// IdentityRegistry NFT id of the platform agent (#0 by convention).
    pub platform_agent_token_id: u64,
}

impl MarketplaceAddresses {
    /// Not deployed on Mantle mainnet (5000) — V4-gated.
    pub fn mantle_mainnet() -> Option<Self> {
        None
    }

    /// Not deployed on Mantle Sepolia testnet (5003) yet — Phase 3/5.
    pub fn mantle_testnet() -> Option<Self> {
        None
    }

    /// Read marketplace contract addresses from environment variables.
    ///
    /// Returns `None` if `XVN_LISTING_REGISTRY` is absent or not a valid hex
    /// address (it is the one required address for `publish`). All other
    /// addresses default to [`Address::ZERO`]; the driver's `require_addr`
    /// guard rejects zero addresses per-operation with a `NotConfigured` error
    /// so callers get a clear message rather than an opaque on-chain revert.
    ///
    /// | Field                    | Env var                                  |
    /// |--------------------------|------------------------------------------|
    /// | `listing_registry`       | `XVN_LISTING_REGISTRY`  (required)       |
    /// | `marketplace`            | `XVN_MARKETPLACE_CONTRACT`               |
    /// | `license_token`          | `XVN_LICENSE_TOKEN`                      |
    /// | `eval_attestation`       | `XVN_EVAL_ATTESTATION`                   |
    /// | `validation_registry`    | `XVN_VALIDATION_REGISTRY`                |
    /// | `usdc`                   | `XVN_MARKETPLACE_USDC`                   |
    /// | `xvn_deployer`           | `XVN_MARKETPLACE_DEPLOYER`               |
    /// | `platform_agent_token_id`| `XVN_MARKETPLACE_PLATFORM_AGENT_TOKEN_ID`|
    pub fn from_env() -> Option<Self> {
        fn opt_addr(key: &str) -> Address {
            std::env::var(key)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Address::ZERO)
        }

        let listing_registry: Address = std::env::var("XVN_LISTING_REGISTRY").ok()?.parse().ok()?;

        Some(Self {
            xvn_deployer: opt_addr("XVN_MARKETPLACE_DEPLOYER"),
            listing_registry,
            marketplace: opt_addr("XVN_MARKETPLACE_CONTRACT"),
            license_token: opt_addr("XVN_LICENSE_TOKEN"),
            eval_attestation: opt_addr("XVN_EVAL_ATTESTATION"),
            validation_registry: opt_addr("XVN_VALIDATION_REGISTRY"),
            usdc: opt_addr("XVN_MARKETPLACE_USDC"),
            platform_agent_token_id: std::env::var("XVN_MARKETPLACE_PLATFORM_AGENT_TOKEN_ID")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marketplace_addresses_unset_on_both_chains() {
        assert!(MarketplaceAddresses::mantle_mainnet().is_none());
        assert!(MarketplaceAddresses::mantle_testnet().is_none());
    }

    #[test]
    fn from_env_reads_listing_registry() {
        // Run both env states sequentially in one test to avoid env-mutation races
        // with parallel test threads.
        let _ = std::env::remove_var("XVN_LISTING_REGISTRY");
        assert!(MarketplaceAddresses::from_env().is_none(), "absent → None");

        let addr = "0x1111111111111111111111111111111111111111";
        std::env::set_var("XVN_LISTING_REGISTRY", addr);
        let result = MarketplaceAddresses::from_env();
        std::env::remove_var("XVN_LISTING_REGISTRY");
        let addrs = result.expect("XVN_LISTING_REGISTRY set → Some");
        assert_eq!(addrs.listing_registry, addr.parse::<Address>().unwrap());
        assert_eq!(addrs.marketplace, Address::ZERO);
        assert_eq!(addrs.platform_agent_token_id, 0);
    }
}

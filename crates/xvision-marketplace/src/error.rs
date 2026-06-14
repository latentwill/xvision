//! Error type for marketplace orchestration.

use thiserror::Error;

/// Errors returned by [`crate::AnchorDriver`] implementations.
#[derive(Debug, Error)]
pub enum MarketplaceError {
    /// A driver method is a stub awaiting Phase 5 implementation.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    /// The driver has no deployed contract addresses for its target chain
    /// (e.g. [`xvision_identity::MarketplaceAddresses::mantle_testnet`] returns
    /// `None` pre-deploy). Inject explicit addresses to proceed.
    #[error("marketplace not configured: {0}")]
    NotConfigured(&'static str),

    /// JSON-RPC transport / connection failure (endpoint unreachable, chain-id
    /// mismatch, signer-provider construction).
    #[error("rpc: {0}")]
    Rpc(String),

    /// A contract call reverted, or its receipt could not be decoded.
    #[error("contract: {0}")]
    Contract(String),

    /// On-chain interaction failed (rpc, revert, decode).
    #[error("chain: {0}")]
    Chain(String),

    /// IPFS pin/fetch failed.
    #[error("ipfs: {0}")]
    Ipfs(String),

    /// Sealed-bundle crypto failure (Lit Action request, encrypt/decrypt, or
    /// the escrow fallback). See [`crate::sealed`].
    #[error("sealed: {0}")]
    Sealed(String),

    /// Referenced listing does not exist in the driver's view.
    #[error("unknown listing: {0}")]
    UnknownListing(u64),

    /// Listing was revoked and can no longer be sold.
    #[error("listing revoked: {0}")]
    ListingRevoked(u64),

    /// Wraps a lower-level identity-client error.
    #[error("identity: {0}")]
    Identity(#[from] xvision_identity::IdentityError),

    /// EIP-3009 signing or ecrecover failure.
    #[error("signing error: {0}")]
    Signing(String),
}

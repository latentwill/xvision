//! Error type for marketplace orchestration.

use thiserror::Error;

/// Errors returned by [`crate::AnchorDriver`] implementations.
#[derive(Debug, Error)]
pub enum MarketplaceError {
    /// A driver method is a stub awaiting Phase 5 implementation.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    /// On-chain interaction failed (rpc, revert, decode).
    #[error("chain: {0}")]
    Chain(String),

    /// IPFS pin/fetch failed.
    #[error("ipfs: {0}")]
    Ipfs(String),

    /// Referenced listing does not exist in the driver's view.
    #[error("unknown listing: {0}")]
    UnknownListing(u64),

    /// Listing was revoked and can no longer be sold.
    #[error("listing revoked: {0}")]
    ListingRevoked(u64),

    /// Wraps a lower-level identity-client error.
    #[error("identity: {0}")]
    Identity(#[from] xvision_identity::IdentityError),
}

//! The [`IpfsStore`] port for manifest / sealed-bundle storage.
//!
//! V2 ships only [`PinataDriver`] (backstop tier). The `iroh` install-mesh
//! driver lands in V3 (direction doc §8.10); keeping storage behind this trait
//! makes that swap mechanical (nav-doc open question C7).

use async_trait::async_trait;

use crate::error::MarketplaceError;

/// Content-addressed storage for listing metadata and sealed bundles.
#[async_trait]
pub trait IpfsStore: Send + Sync {
    /// Pin `bytes`, returning the content id (e.g. `bafy…`).
    async fn put(&self, bytes: &[u8]) -> Result<String, MarketplaceError>;

    /// Fetch the bytes behind `cid`.
    async fn get(&self, cid: &str) -> Result<Vec<u8>, MarketplaceError>;
}

/// Pinata-backed `IpfsStore` (V2 backstop tier). STUB — wiring the Pinata HTTP
/// API is Phase 5 work; the trait shape is what dependents code against now.
pub struct PinataDriver {
    jwt: String,
    gateway: String,
}

impl PinataDriver {
    pub fn new(jwt: impl Into<String>, gateway: impl Into<String>) -> Self {
        Self { jwt: jwt.into(), gateway: gateway.into() }
    }

    pub fn gateway(&self) -> &str {
        &self.gateway
    }

    /// Whether credentials are present (does not validate them).
    pub fn is_configured(&self) -> bool {
        !self.jwt.is_empty()
    }
}

#[async_trait]
impl IpfsStore for PinataDriver {
    async fn put(&self, _bytes: &[u8]) -> Result<String, MarketplaceError> {
        Err(MarketplaceError::NotImplemented("PinataDriver::put"))
    }

    async fn get(&self, _cid: &str) -> Result<Vec<u8>, MarketplaceError> {
        Err(MarketplaceError::NotImplemented("PinataDriver::get"))
    }
}

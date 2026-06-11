//! Marketplace chain configuration — resolved ONCE at server startup.
//!
//! Before this module existed every mutating marketplace route re-read the
//! chain env per request (`ChainEnv::from_env`, `registry_addresses_from_env`,
//! `MarketplaceAddresses::from_env`, `pinata_env`) and the read routes
//! re-read `IndexerCfg::from_env` + `XVN_LICENSE_TOKEN`. Now `server::serve`
//! calls [`MarketplaceChainConfig::from_env`] once and stores the result in
//! `AppState` as `Option<Arc<MarketplaceChainConfig>>` (xvision-df3).
//!
//! Dormancy semantics are unchanged: a route that needs a missing piece
//! returns the same 503 with the same actionable message as before. Each
//! sub-config is independently optional inside the struct so per-route
//! gating stays exact; the whole config is `None` only when every
//! chain-relevant piece is unset (Pinata alone does not activate it — the
//! pin config is only meaningful alongside a publish-capable chain config).
//!
//! One deliberate semantic note (documented, behavior class preserved): an
//! invalid `XVN_PUBLISHER_PK` used to produce a per-request 503
//! "XVN_PUBLISHER_PK is not a valid private key". It now logs a startup
//! `warn` and leaves [`MarketplaceChainConfig::chain`] unset, so the same
//! requests still 503 — with the generic "chain not configured" message.
//! The server never crashes on a bad key.

use std::fmt;

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;

use xvision_identity::RegistryAddresses;
use xvision_marketplace::MarketplaceAddresses;

use crate::marketplace_index::IndexerCfg;

/// Chain connection + publisher signer. All of `XVN_RPC_URL`,
/// `XVN_CHAIN_ID`, `XVN_PUBLISHER_PK` are required, and the key must parse.
pub struct ChainSigner {
    pub rpc_url: String,
    pub chain_id: u64,
    /// The publisher/relayer key, parsed once at startup.
    pub signer: PrivateKeySigner,
}

/// Manual Debug impl — redacts the signer so it cannot appear in logs.
impl fmt::Debug for ChainSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChainSigner")
            .field("rpc_url", &self.rpc_url)
            .field("chain_id", &self.chain_id)
            .field("signer", &"<redacted>")
            .finish()
    }
}

impl ChainSigner {
    /// Reads `XVN_RPC_URL`, `XVN_CHAIN_ID`, `XVN_PUBLISHER_PK`. Returns
    /// `None` when any is missing or `XVN_CHAIN_ID` is not a valid u64.
    /// A present-but-unparseable private key logs a warning and yields
    /// `None` (mutating routes then 503 per request) — never a crash.
    fn from_env() -> Option<Self> {
        let rpc_url = std::env::var("XVN_RPC_URL").ok()?;
        let chain_id: u64 = std::env::var("XVN_CHAIN_ID").ok()?.parse().ok()?;
        let publisher_pk = std::env::var("XVN_PUBLISHER_PK").ok()?;
        match publisher_pk.parse::<PrivateKeySigner>() {
            Ok(signer) => Some(Self {
                rpc_url,
                chain_id,
                signer,
            }),
            Err(_) => {
                tracing::warn!(
                    "XVN_PUBLISHER_PK is set but is not a valid private key; treating the chain \
                     signer as unconfigured (mutating marketplace routes will return 503)"
                );
                None
            }
        }
    }
}

/// Optional Pinata pin config: `PINATA_JWT` (required to pin) and
/// `PINATA_GATEWAY` (optional; empty → driver default).
#[derive(Debug, Clone)]
pub struct PinataCfg {
    pub jwt: String,
    pub gateway: String,
}

/// `None` when the JWT is unset or blank — publish/update then fall back to
/// the local `xvn://` content_uri.
fn pinata_from_env() -> Option<PinataCfg> {
    let jwt = std::env::var("PINATA_JWT").ok()?;
    if jwt.trim().is_empty() {
        return None;
    }
    let gateway = std::env::var("PINATA_GATEWAY").unwrap_or_default();
    Some(PinataCfg { jwt, gateway })
}

/// Reads the identity registry addresses: `XVN_IDENTITY_REGISTRY` (required
/// for minting) and `XVN_REPUTATION_REGISTRY` (optional — `register` doesn't
/// touch it; defaults to the zero address).
fn registry_addresses_from_env() -> Option<RegistryAddresses> {
    let identity: Address = std::env::var("XVN_IDENTITY_REGISTRY").ok()?.parse().ok()?;
    let reputation: Address = std::env::var("XVN_REPUTATION_REGISTRY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(Address::ZERO);
    Some(RegistryAddresses::custom(identity, reputation))
}

/// Reads the optional `XVN_LICENSE_TOKEN` address (license gate + wallet
/// license balances). Unset or unparseable → `None`.
fn license_token_from_env() -> Option<Address> {
    std::env::var("XVN_LICENSE_TOKEN").ok()?.parse().ok()
}

/// All chain-facing marketplace configuration, resolved once at server
/// startup and shared via `AppState`. Each piece is independently optional
/// so routes can keep their exact per-piece 503 messages.
#[derive(Debug)]
pub struct MarketplaceChainConfig {
    /// RPC + publisher signer — required by every mutating chain route.
    pub chain: Option<ChainSigner>,
    /// IdentityRegistry (+ optional ReputationRegistry) — publish mint path.
    pub registry_addresses: Option<RegistryAddresses>,
    /// ListingRegistry / Marketplace / USDC address book — publish, revoke,
    /// buy, attest, update.
    pub marketplace_addresses: Option<MarketplaceAddresses>,
    /// Pinata pin config — publish/update manifest pinning.
    pub pinata: Option<PinataCfg>,
    /// Read-only indexer config — also reused by the read routes
    /// (attestations, receipts, wallet) and the import license gate.
    pub indexer: Option<IndexerCfg>,
    /// ERC-1155 license token — import gate + wallet license balances.
    pub license_token: Option<Address>,
}

impl MarketplaceChainConfig {
    /// Resolves every chain-relevant env var once. Returns `None` when ALL
    /// chain pieces are unset (fully dormant — routes 503 exactly as they
    /// did when they read the env per request). Pinata alone does not
    /// activate the config.
    pub fn from_env() -> Option<Self> {
        let cfg = Self {
            chain: ChainSigner::from_env(),
            registry_addresses: registry_addresses_from_env(),
            marketplace_addresses: MarketplaceAddresses::from_env(),
            pinata: pinata_from_env(),
            indexer: IndexerCfg::from_env(),
            license_token: license_token_from_env(),
        };
        let dormant = cfg.chain.is_none()
            && cfg.registry_addresses.is_none()
            && cfg.marketplace_addresses.is_none()
            && cfg.indexer.is_none()
            && cfg.license_token.is_none();
        if dormant {
            None
        } else {
            Some(cfg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_fully_dormant_is_none() {
        // Removal-only: these tests own the marketplace chain env vars in
        // this crate's unit suite (the crate-wide env-mutation convention) —
        // no other unit test sets them, so removal cannot race a sibling.
        for var in [
            "XVN_RPC_URL",
            "XVN_CHAIN_ID",
            "XVN_PUBLISHER_PK",
            "XVN_IDENTITY_REGISTRY",
            "XVN_REPUTATION_REGISTRY",
            "XVN_LISTING_REGISTRY",
            "XVN_LICENSE_TOKEN",
        ] {
            std::env::remove_var(var);
        }
        assert!(MarketplaceChainConfig::from_env().is_none());
    }

    #[test]
    fn pinata_cfg_requires_nonblank_jwt() {
        // Single test owns PINATA_JWT / PINATA_GATEWAY in this crate's unit
        // suite (the crate-wide env-mutation convention).
        std::env::remove_var("PINATA_JWT");
        std::env::remove_var("PINATA_GATEWAY");
        assert!(pinata_from_env().is_none());

        std::env::set_var("PINATA_JWT", "   ");
        assert!(pinata_from_env().is_none(), "blank JWT is not configured");

        std::env::set_var("PINATA_JWT", "jwt-token");
        std::env::set_var("PINATA_GATEWAY", "https://gw.example");
        let cfg = pinata_from_env().expect("configured");
        assert_eq!(cfg.jwt, "jwt-token");
        assert_eq!(cfg.gateway, "https://gw.example");
        std::env::remove_var("PINATA_JWT");
        std::env::remove_var("PINATA_GATEWAY");
    }
}

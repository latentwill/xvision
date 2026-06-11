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
//! chain-relevant piece is unset (an IPFS pin backend alone does not activate
//! it — pinning is only meaningful alongside a publish-capable chain config).
//!
//! One deliberate semantic note (documented, behavior class preserved): an
//! invalid `XVN_PUBLISHER_PK` used to produce a per-request 503
//! "XVN_PUBLISHER_PK is not a valid private key". It now logs a startup
//! `warn` and leaves [`MarketplaceChainConfig::chain`] unset, so the same
//! requests still 503 — with the generic "chain not configured" message.
//! The server never crashes on a bad key.

use std::fmt;
use std::future::IntoFuture;
use std::time::Duration;

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;

use xvision_identity::RegistryAddresses;
use xvision_marketplace::{IpfsStore, KuboStore, MarketplaceAddresses, PinataDriver};

use crate::error::DashboardError;
use crate::marketplace_index::IndexerCfg;

/// Default per-call deadline for chain interactions (RPC connects, contract
/// calls, transaction sends) when `XVN_CHAIN_TIMEOUT_SECS` is unset.
pub const DEFAULT_CHAIN_TIMEOUT_SECS: u64 = 45;

/// Bounds one chain interaction with a deadline (xvision-4fp). On timeout
/// the future is dropped and the routes' upstream-error class (503
/// `ServiceUnavailable`) is returned with an explicit message — a hung RPC
/// can no longer pin a request handler forever.
///
/// Accepts `IntoFuture` so alloy's lazy call builders (`.call()`,
/// `get_block_by_hash`, …) can be passed directly.
pub async fn with_chain_timeout<F: IntoFuture>(
    timeout: Duration,
    fut: F,
) -> Result<F::Output, DashboardError> {
    tokio::time::timeout(timeout, fut.into_future())
        .await
        .map_err(|_| {
            DashboardError::ServiceUnavailable(format!("chain call timed out after {}s", timeout.as_secs()))
        })
}

/// The per-call chain deadline from the startup-resolved config. The default
/// is only reachable when no config exists at all — and then every chain
/// route 503s before its first chain call anyway.
pub fn chain_call_timeout(mp: Option<&MarketplaceChainConfig>) -> Duration {
    mp.map(|c| c.chain_timeout)
        .unwrap_or(Duration::from_secs(DEFAULT_CHAIN_TIMEOUT_SECS))
}

/// Reads `XVN_CHAIN_TIMEOUT_SECS` once at startup. Unset, unparseable, or
/// zero → [`DEFAULT_CHAIN_TIMEOUT_SECS`].
fn chain_timeout_from_env() -> Duration {
    let secs = std::env::var("XVN_CHAIN_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(DEFAULT_CHAIN_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

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

/// The startup-resolved IPFS backend: a self-hosted Kubo (go-ipfs) node
/// (preferred — no paid pinning service) or Pinata (alternative hosted
/// backend). Both implement [`IpfsStore`]; this enum delegates so routes can
/// hold one concrete type in `AppState` without a trait object.
pub enum IpfsBackend {
    /// Self-hosted Kubo node (`XVN_IPFS_API_URL` / `XVN_IPFS_GATEWAY_URL`).
    Kubo(KuboStore),
    /// Pinata hosted pinning (`PINATA_JWT` / `PINATA_GATEWAY`).
    Pinata(PinataDriver),
}

/// Manual Debug — names the backend without spilling driver internals
/// (the Pinata variant holds a JWT).
impl fmt::Debug for IpfsBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpfsBackend::Kubo(_) => f.write_str("IpfsBackend::Kubo"),
            IpfsBackend::Pinata(_) => f.write_str("IpfsBackend::Pinata"),
        }
    }
}

#[async_trait::async_trait]
impl IpfsStore for IpfsBackend {
    async fn put(&self, bytes: &[u8]) -> Result<String, xvision_marketplace::MarketplaceError> {
        match self {
            IpfsBackend::Kubo(k) => k.put(bytes).await,
            IpfsBackend::Pinata(p) => p.put(bytes).await,
        }
    }

    async fn get(&self, cid: &str) -> Result<Vec<u8>, xvision_marketplace::MarketplaceError> {
        match self {
            IpfsBackend::Kubo(k) => k.get(cid).await,
            IpfsBackend::Pinata(p) => p.get(cid).await,
        }
    }
}

/// Resolves the pin backend once at startup. Preference order:
///
/// 1. **Kubo** when `XVN_IPFS_API_URL` is set non-blank (e.g.
///    `http://127.0.0.1:5001`). Optional `XVN_IPFS_GATEWAY_URL` overrides the
///    read gateway; unset → the node's default `http://127.0.0.1:8080`.
/// 2. **Pinata** when `PINATA_JWT` is set non-blank (optional
///    `PINATA_GATEWAY`; empty → the public Pinata gateway).
/// 3. `None` — publish/update fall back to the local `xvn://` content_uri,
///    and ipfs:// reads fall back to the public gateway.
fn ipfs_from_env() -> Option<IpfsBackend> {
    if let Ok(api_url) = std::env::var("XVN_IPFS_API_URL") {
        if !api_url.trim().is_empty() {
            let gateway = std::env::var("XVN_IPFS_GATEWAY_URL").unwrap_or_default();
            return Some(IpfsBackend::Kubo(KuboStore::new(api_url, gateway)));
        }
    }
    let jwt = std::env::var("PINATA_JWT").ok()?;
    if jwt.trim().is_empty() {
        return None;
    }
    let gateway = std::env::var("PINATA_GATEWAY").unwrap_or_default();
    Some(IpfsBackend::Pinata(PinataDriver::new(jwt, gateway)))
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
    /// IPFS pin backend (Kubo preferred, Pinata fallback) —
    /// publish/update manifest pinning + ipfs:// bundle reads.
    pub ipfs: Option<IpfsBackend>,
    /// Read-only indexer config — also reused by the read routes
    /// (attestations, receipts, wallet) and the import license gate.
    pub indexer: Option<IndexerCfg>,
    /// ERC-1155 license token — import gate + wallet license balances.
    pub license_token: Option<Address>,
    /// Per-call deadline for chain interactions (xvision-4fp). Resolved at
    /// startup from `XVN_CHAIN_TIMEOUT_SECS`, default 45s.
    pub chain_timeout: Duration,
}

impl MarketplaceChainConfig {
    /// Resolves every chain-relevant env var once. Returns `None` when ALL
    /// chain pieces are unset (fully dormant — routes 503 exactly as they
    /// did when they read the env per request). An IPFS backend alone does
    /// not activate the config.
    pub fn from_env() -> Option<Self> {
        let cfg = Self {
            chain: ChainSigner::from_env(),
            registry_addresses: registry_addresses_from_env(),
            marketplace_addresses: MarketplaceAddresses::from_env(),
            ipfs: ipfs_from_env(),
            indexer: IndexerCfg::from_env(),
            license_token: license_token_from_env(),
            chain_timeout: chain_timeout_from_env(),
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
    fn ipfs_backend_prefers_kubo_then_pinata() {
        // Single test owns XVN_IPFS_API_URL / XVN_IPFS_GATEWAY_URL /
        // PINATA_JWT / PINATA_GATEWAY in this crate's unit suite (the
        // crate-wide env-mutation convention) — one test so the four vars
        // can't race a parallel sibling.
        for var in [
            "XVN_IPFS_API_URL",
            "XVN_IPFS_GATEWAY_URL",
            "PINATA_JWT",
            "PINATA_GATEWAY",
        ] {
            std::env::remove_var(var);
        }
        assert!(ipfs_from_env().is_none(), "nothing set → no backend");

        // Blank values are unset.
        std::env::set_var("XVN_IPFS_API_URL", "   ");
        std::env::set_var("PINATA_JWT", "   ");
        assert!(ipfs_from_env().is_none(), "blank values → no backend");

        // Pinata when only the JWT is set.
        std::env::set_var("PINATA_JWT", "jwt-token");
        std::env::set_var("PINATA_GATEWAY", "https://gw.example");
        std::env::remove_var("XVN_IPFS_API_URL");
        match ipfs_from_env() {
            Some(IpfsBackend::Pinata(p)) => assert_eq!(p.gateway(), "https://gw.example"),
            other => panic!("expected Pinata backend, got {other:?}"),
        }

        // Kubo wins over Pinata when both are set; unset gateway → the
        // node's default :8080 gateway.
        std::env::set_var("XVN_IPFS_API_URL", "http://127.0.0.1:5001");
        match ipfs_from_env() {
            Some(IpfsBackend::Kubo(k)) => assert_eq!(k.gateway(), "http://127.0.0.1:8080"),
            other => panic!("expected Kubo backend, got {other:?}"),
        }

        // Explicit Kubo gateway override.
        std::env::set_var("XVN_IPFS_GATEWAY_URL", "http://gw.kubo.example/");
        match ipfs_from_env() {
            Some(IpfsBackend::Kubo(k)) => assert_eq!(k.gateway(), "http://gw.kubo.example"),
            other => panic!("expected Kubo backend, got {other:?}"),
        }

        for var in [
            "XVN_IPFS_API_URL",
            "XVN_IPFS_GATEWAY_URL",
            "PINATA_JWT",
            "PINATA_GATEWAY",
        ] {
            std::env::remove_var(var);
        }
    }

    // --- with_chain_timeout (xvision-4fp) -----------------------------------

    #[tokio::test]
    async fn with_chain_timeout_passes_instant_future_through() {
        let out = with_chain_timeout(Duration::from_secs(1), async { 42u32 })
            .await
            .expect("instant future completes inside the deadline");
        assert_eq!(out, 42);
    }

    #[tokio::test]
    async fn with_chain_timeout_times_out_pending_future() {
        let err = with_chain_timeout(Duration::from_millis(5), std::future::pending::<()>())
            .await
            .expect_err("pending future must hit the deadline");
        match err {
            DashboardError::ServiceUnavailable(msg) => {
                assert!(
                    msg.contains("chain call timed out after 0s"),
                    "names the deadline: {msg}"
                );
            }
            other => panic!("expected ServiceUnavailable, got {other:?}"),
        }
    }
}

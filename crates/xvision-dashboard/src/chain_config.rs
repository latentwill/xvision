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
//! chain-relevant piece is unset (IPFS/Lit backends alone do not activate it
//! — they are only meaningful alongside publish/import-capable chain config).
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
use xvision_marketplace::{
    IpfsStore, KuboStore, LitChipotleClient, MarketplaceAddresses, NoopSealed, PinataDriver,
    SealedBundleCrypto,
};

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

impl IpfsBackend {
    /// The configured READ gateway base for this backend (never the pinning
    /// API URL or JWT). Surfaced to the public status route so the frontend
    /// can build `${gateway}/ipfs/<cid>` open-bundle links.
    pub fn gateway(&self) -> &str {
        match self {
            IpfsBackend::Kubo(k) => k.gateway(),
            IpfsBackend::Pinata(p) => p.gateway(),
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

/// Lit Protocol v3 ("Chipotle") config for sealed-tier bundle encryption.
/// All FIVE env vars are required for `Some`:
/// `XVN_LIT_API_BASE`, `XVN_LIT_API_KEY`, `XVN_LIT_PKP_ID`,
/// `XVN_LIT_GATE_ACTION_CID`, `XVN_LIT_ENCRYPT_ACTION_CID`. NOT yet wired into
/// routes (later phase) — this phase only resolves it at startup and can build
/// a [`LitChipotleClient`].
#[derive(Clone)]
pub struct LitConfig {
    /// Lit REST API base (e.g. `https://api.chipotle.litprotocol.com`).
    pub api_base: String,
    /// `X-Api-Key` value. Redacted in `Debug`.
    pub api_key: String,
    /// PKP id whose key wraps sealed payloads.
    pub pkp_id: String,
    /// IPFS CID of the immutable decrypt gate Lit Action.
    pub gate_action_cid: String,
    /// IPFS CID of the pinned ENCRYPT Lit Action run server-side at publish.
    pub encrypt_action_cid: String,
}

/// Manual Debug impl — redacts the API key so it cannot appear in logs.
impl fmt::Debug for LitConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LitConfig")
            .field("api_base", &self.api_base)
            .field("api_key", &"<redacted>")
            .field("pkp_id", &self.pkp_id)
            .field("gate_action_cid", &self.gate_action_cid)
            .field("encrypt_action_cid", &self.encrypt_action_cid)
            .finish()
    }
}

impl LitConfig {
    /// Build a [`LitChipotleClient`] from this config.
    pub fn build_client(&self) -> LitChipotleClient {
        LitChipotleClient::with_api_base(
            &self.api_base,
            &self.api_key,
            &self.pkp_id,
            &self.gate_action_cid,
            &self.encrypt_action_cid,
        )
    }
}

/// Reads the Lit Chipotle config. Returns `Some` only when ALL FIVE of
/// `XVN_LIT_API_BASE`, `XVN_LIT_API_KEY`, `XVN_LIT_PKP_ID`,
/// `XVN_LIT_GATE_ACTION_CID`, `XVN_LIT_ENCRYPT_ACTION_CID` are set (and
/// non-blank); any missing → `None`.
fn lit_from_env() -> Option<LitConfig> {
    let nonblank = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let api_base = nonblank("XVN_LIT_API_BASE")?;
    let api_key = nonblank("XVN_LIT_API_KEY")?;
    let pkp_id = nonblank("XVN_LIT_PKP_ID")?;
    let gate_action_cid = nonblank("XVN_LIT_GATE_ACTION_CID")?;
    let encrypt_action_cid = nonblank("XVN_LIT_ENCRYPT_ACTION_CID")?;
    Some(LitConfig {
        api_base,
        api_key,
        pkp_id,
        gate_action_cid,
        encrypt_action_cid,
    })
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
    /// Lit Protocol v3 ("Chipotle") config — sealed-tier bundle encryption.
    /// Resolved at startup but NOT yet wired into routes (later phase). Like
    /// Pinata, sealed crypto alone does not activate the chain config.
    pub lit: Option<LitConfig>,
    /// Per-call deadline for chain interactions (xvision-4fp). Resolved at
    /// startup from `XVN_CHAIN_TIMEOUT_SECS`, default 45s.
    pub chain_timeout: Duration,
}

impl MarketplaceChainConfig {
    /// Resolves every chain-relevant env var once. Returns `None` when ALL
    /// chain pieces are unset (fully dormant — routes 503 exactly as they
    /// did when they read the env per request). IPFS/Lit backends alone do
    /// not activate the config.
    pub fn from_env() -> Option<Self> {
        let cfg = Self {
            chain: ChainSigner::from_env(),
            registry_addresses: registry_addresses_from_env(),
            marketplace_addresses: MarketplaceAddresses::from_env(),
            ipfs: ipfs_from_env(),
            indexer: IndexerCfg::from_env(),
            license_token: license_token_from_env(),
            lit: lit_from_env(),
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

    /// Resolve the sealed-bundle crypto backend from the `lit` config: a
    /// configured [`LitChipotleClient`] when `lit` is `Some`, else
    /// [`NoopSealed`] (whose `encrypt` always errors "not configured"). Mirrors
    /// how `ipfs` resolves a [`crate::routes::marketplace`] IPFS driver — the
    /// route holds a `Box<dyn SealedBundleCrypto>` and never binds to a
    /// concrete backend.
    pub fn resolve_sealed_crypto(&self) -> Box<dyn SealedBundleCrypto> {
        match &self.lit {
            Some(lit) => Box::new(lit.build_client()),
            None => Box::new(NoopSealed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_marketplace::SealedBundleCrypto;

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

    #[test]
    fn lit_cfg_requires_all_five_env_vars() {
        // Single test owns the XVN_LIT_* vars in this crate's unit suite
        // (the crate-wide env-mutation convention).
        let vars = [
            "XVN_LIT_API_BASE",
            "XVN_LIT_API_KEY",
            "XVN_LIT_PKP_ID",
            "XVN_LIT_GATE_ACTION_CID",
            "XVN_LIT_ENCRYPT_ACTION_CID",
        ];
        let set_all = || {
            std::env::set_var("XVN_LIT_API_BASE", "https://api.chipotle.litprotocol.com");
            std::env::set_var("XVN_LIT_API_KEY", "secret-key");
            std::env::set_var("XVN_LIT_PKP_ID", "pkp-123");
            std::env::set_var("XVN_LIT_GATE_ACTION_CID", "bafygatecid");
            std::env::set_var("XVN_LIT_ENCRYPT_ACTION_CID", "bafyencryptcid");
        };
        for v in vars {
            std::env::remove_var(v);
        }
        assert!(lit_from_env().is_none(), "all unset → None");

        // Set all five → Some.
        set_all();
        let cfg = lit_from_env().expect("all five set → Some");
        assert_eq!(cfg.api_base, "https://api.chipotle.litprotocol.com");
        assert_eq!(cfg.pkp_id, "pkp-123");
        assert_eq!(cfg.gate_action_cid, "bafygatecid");
        assert_eq!(cfg.encrypt_action_cid, "bafyencryptcid");

        // Debug redacts the api key.
        let dbg = format!("{cfg:?}");
        assert!(dbg.contains("<redacted>"), "{dbg}");
        assert!(!dbg.contains("secret-key"), "{dbg}");

        // build_client carries the gate CID through.
        assert_eq!(cfg.build_client().gate_action_cid(), "bafygatecid");

        // Any one missing → None.
        for missing in vars {
            set_all();
            std::env::remove_var(missing);
            assert!(lit_from_env().is_none(), "{missing} missing → None");
        }

        for v in vars {
            std::env::remove_var(v);
        }
    }

    #[test]
    fn resolve_sealed_crypto_uses_lit_when_configured() {
        let cfg = MarketplaceChainConfig {
            chain: None,
            registry_addresses: None,
            marketplace_addresses: None,
            ipfs: None,
            indexer: None,
            license_token: None,
            lit: Some(LitConfig {
                api_base: "https://api.chipotle.litprotocol.com".into(),
                api_key: "key".into(),
                pkp_id: "pkp-123".into(),
                gate_action_cid: "bafygatecid".into(),
                encrypt_action_cid: "bafyencryptcid".into(),
            }),
            chain_timeout: Duration::from_secs(45),
        };
        let crypto = cfg.resolve_sealed_crypto();
        // A Lit-backed client is configured and carries the gate CID through.
        assert!(crypto.is_configured());
        assert_eq!(crypto.gate_action_cid(), "bafygatecid");
    }

    #[test]
    fn resolve_sealed_crypto_falls_back_to_noop_when_lit_unset() {
        let cfg = MarketplaceChainConfig {
            chain: None,
            registry_addresses: None,
            marketplace_addresses: None,
            ipfs: None,
            indexer: None,
            license_token: None,
            lit: None,
            chain_timeout: Duration::from_secs(45),
        };
        let crypto = cfg.resolve_sealed_crypto();
        // NoopSealed: not configured, no gate.
        assert!(!crypto.is_configured());
        assert_eq!(crypto.gate_action_cid(), "");
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

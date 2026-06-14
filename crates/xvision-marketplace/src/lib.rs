//! xvision-marketplace — higher-level orchestration over the on-chain
//! marketplace contract surface (surface spec §8.2).
//!
//! This crate is the shim between the engine/CLI and the chain. It exposes the
//! four orchestration verbs — `publish_listing`, `buy_listing`, `attest_eval`,
//! `revoke_listing` — behind the [`AnchorDriver`] port so call sites never touch
//! `alloy` directly and can be tested with [`MockDriver`] (no Mantle needed).
//! Storage of manifests / sealed bundles goes through the separate [`IpfsStore`]
//! port; V2 ships a Pinata-backed driver, `iroh` install-mesh is V3 (direction
//! doc §8.10) — the trait makes that swap mechanical.
//!
//! The low-level `alloy::sol!` bindings live in
//! [`xvision_identity::contracts`]; this crate wraps them.
//!
//! # Status
//!
//! [`MockDriver`] is fully functional (in-memory) so dependents and tests can
//! exercise the verbs today. [`Erc8004MantleDriver`] is wired to the real
//! `alloy` bindings — each verb builds a signer-backed provider and transacts —
//! but the marketplace contracts are not deployed on either Mantle chain yet
//! ([`xvision_identity::MarketplaceAddresses::mantle_testnet`] returns `None`),
//! so it must be constructed with explicitly-injected addresses + a signer
//! ([`Erc8004MantleDriver::with_signer`]); pre-deploy / sentinel-zero addresses
//! yield [`MarketplaceError::NotConfigured`]. The end-to-end anvil round-trip
//! is deploy-gated (the bindings are interface-only — no bytecode — so the
//! contracts can't be deployed from Rust); see the `#[ignore]`d scaffold in
//! `adapter.rs`. [`PinataDriver`] performs real HTTP pins/fetches.
//!
//! ## Dependency rule (plugin spec §3.1)
//! `marketplace` may import from engine/eval/strategy crates; the reverse is
//! forbidden — the trading core never `use`s this crate. Keep it that way.

pub mod adapter;
pub mod error;
pub mod ipfs;
pub mod sealed;
pub mod x402;

pub use adapter::{
    AnchorDriver, AttestRequest, BuyRequest, Erc8004MantleDriver, ListingRef, MockDriver, PublishRequest,
    SaleReceipt, TransferAuthorization,
};
pub use error::MarketplaceError;
pub use ipfs::{IpfsStore, KuboStore, PinataDriver};
pub use sealed::{EscrowSealed, LitChipotleClient, NoopSealed, SealedBundleCrypto};
pub use xvision_identity::MarketplaceAddresses;

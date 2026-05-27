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
//! # Status — SKELETON
//!
//! [`MockDriver`] is fully functional (in-memory) so dependents and tests can
//! exercise the verbs today. [`Erc8004MantleDriver`] is a stub: it carries the
//! deployed [`xvision_identity::MarketplaceAddresses`] and documents the wiring,
//! but its methods return [`MarketplaceError::NotImplemented`] until the
//! contracts are deployed to Mantle Sepolia (Phase 3/5). No other crate depends
//! on this one yet.
//!
//! ## Dependency rule (plugin spec §3.1)
//! `marketplace` may import from engine/eval/strategy crates; the reverse is
//! forbidden — the trading core never `use`s this crate. Keep it that way.

pub mod adapter;
pub mod error;
pub mod ipfs;

pub use adapter::{
    AnchorDriver, AttestRequest, BuyRequest, Erc8004MantleDriver, ListingRef, MockDriver, PublishRequest,
    SaleReceipt,
};
pub use error::MarketplaceError;
pub use ipfs::{IpfsStore, PinataDriver};

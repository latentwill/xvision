// The `sol!` macro generates a `giveFeedback` wrapper with 8 parameters
// (matching the ERC-8004 draft interface exactly).  Suppressed here because
// the argument count is dictated by the on-chain ABI, not our design.
#![allow(clippy::too_many_arguments)]

//! xvision-identity — Phase 6.5 ERC-8004 identity registration client.
//!
//! Provides [`IdentityClient`] for minting `agentURI` NFTs and posting
//! reputation updates on Mantle. One manifest per strategy arm
//! (`identity/<arm_name>.agent.json`) — post-CV-extraction (ADR 0011)
//! the per-arm split is no longer "vectors-on / vectors-off" but the
//! deployed strategy name (e.g. `trader_arm`, `buy_and_hold`).
//!
//! # Optional dependency
//! This crate is **opt-in** at the workspace level.  It is excluded from the
//! root `default-members` array so bare `cargo build` / `cargo test` skip it
//! (and the heavy `alloy v2` transitive compile).  Use `cargo build -p
//! xvision-identity` or `cargo test --workspace` to include it explicitly.
//! No other crate in the workspace depends on this one; the trading pipeline
//! is functional end-to-end without minting any NFTs.  Future harness wiring
//! (FOLLOWUPS.md F4) gates the on-chain flow behind a runtime config flag so
//! `cargo run` of the harness without Mantle credentials still works.
//!
//! # ERC-8004 status
//! The standard is **Draft** as of 2025-05.  No production deployment on
//! Mantle mainnet or Mantle Sepolia testnet exists at build time.  This crate
//! ships a minimal stub interface that matches the plan's
//! `register / postReputation / read_reputation` semantics; production
//! deployment is a separate operator step (see `decisions/0008-erc8004-deployment.md`).
//!
//! # Mantle addresses
//! Neither registry is deployed at build time.
//! [`RegistryAddresses::mantle_mainnet`] and [`RegistryAddresses::mantle_testnet`]
//! both return `None`.  Integration tests run against a local `anvil` instance
//! only (`#[ignore]`d; see `client::tests`).

pub mod attestation;
pub mod client;
pub mod contracts;
pub mod genart;
pub mod manifest;

pub use attestation::{
    build_attestation_outcome, decide_submission, AttestationDecision, TAG1_TRADING_YIELD, TAG2_MONTH,
};
pub use client::{IdentityClient, IdentityError, RegistryAddresses, TokenId, TxHash};
pub use contracts::MarketplaceAddresses;
pub use genart::{
    derive_traits, generate_svg, generate_token_uri, manifest_hash_hex, GenartError, Symmetry,
    Traits,
};
pub use manifest::{AgentManifest, ReputationEntry, StrategyConfigSummary, TradeOutcome};

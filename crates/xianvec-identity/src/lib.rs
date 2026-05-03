// The `sol!` macro generates a `giveFeedback` wrapper with 8 parameters
// (matching the ERC-8004 draft interface exactly).  Suppressed here because
// the argument count is dictated by the on-chain ABI, not our design.
#![allow(clippy::too_many_arguments)]

//! xianvec-identity — Phase 6.5 ERC-8004 identity registration client.
//!
//! Provides [`IdentityClient`] for minting `agentURI` NFTs and posting
//! reputation updates on Mantle.  Two experimental arms are supported:
//! - **vectors-OFF** (`identity/vectors_off.agent.json`)
//! - **vectors-ON**  (`identity/vectors_on.agent.json`)
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

pub mod client;
pub mod manifest;

pub use client::{IdentityClient, IdentityError, RegistryAddresses, TokenId, TxHash};
pub use manifest::{AgentManifest, ReputationEntry, TradeOutcome, VectorConfigSummary};

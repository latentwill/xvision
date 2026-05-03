//! ERC-8004 IdentityRegistry + ReputationRegistry client for Mantle.
//!
//! # ABI status — STUB
//!
//! ERC-8004 is **Draft** (EIP repo, 2025-05).  No canonical ABI has been
//! published and no registry contracts are deployed on Mantle mainnet (5000)
//! or Mantle Sepolia testnet (5003) at build time.
//!
//! The `sol!` definitions below match the ERC-8004 draft §3 interface as
//! understood from the EIP text:
//!
//! ```text
//! // IdentityRegistry (ERC-721 + URIStorage)
//! function register(string calldata agentURI) external returns (uint256 agentId)
//! function tokenURI(uint256 tokenId) external view returns (string memory)
//!
//! // ReputationRegistry (ERC-8004 draft §3.2)
//! function giveFeedback(
//!     uint256 agentId, int128 value, uint8 valueDecimals,
//!     string calldata tag1, string calldata tag2,
//!     string calldata endpoint, string calldata feedbackURI, bytes32 feedbackHash
//! ) external
//! function getFeedback(uint256 agentId, uint256 index) external view returns (...)
//! function getFeedbackCount(uint256 agentId) external view returns (uint256)
//! ```
//!
//! The `post_reputation` helper encodes a [`TradeOutcome`] as a JSON string
//! inline in `feedbackURI` and its keccak256 as `feedbackHash`.
//! `value` is `realized_pnl_usd * 1e6` (6 decimal places, i128).
//!
//! Production deployment steps are documented in
//! `decisions/0008-erc8004-deployment.md`.
//!
//! # Mantle chain IDs
//! - Mainnet: 5000 (`https://rpc.mantle.xyz`)
//! - Sepolia testnet: 5003 (`https://rpc.sepolia.mantle.xyz`)
//!
//! # Key management
//! This crate does **not** load private keys.  Callers supply a
//! [`PrivateKeySigner`] (loaded via `op` / 1Password or equivalent).

use alloy::{
    network::EthereumWallet,
    primitives::{keccak256, Address, B256, U256},
    providers::{DynProvider, Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};
use thiserror::Error;
use tracing::{debug, info};
use uuid::Uuid;

use crate::manifest::{ReputationEntry, TradeOutcome};

// ---------------------------------------------------------------------------
// Solidity interface bindings (stub — replace with verified ABIs post-deploy)
// ---------------------------------------------------------------------------

sol! {
    /// IdentityRegistry stub — ERC-8004 draft §3.1 minimal interface.
    ///
    /// `#[sol(rpc)]` generates `IIdentityRegistry::new(address, provider)`
    /// and typed call builders for each function.
    ///
    /// Replace with the deployed contract's verified ABI once live on Mantle.
    #[sol(rpc)]
    interface IIdentityRegistry {
        function register(string calldata agentURI) external returns (uint256 agentId);
        function tokenURI(uint256 tokenId) external view returns (string memory);
        function ownerOf(uint256 tokenId) external view returns (address);
    }

    /// ReputationRegistry stub — ERC-8004 draft §3.2 minimal interface.
    ///
    /// `giveFeedback` is the canonical ERC-8004 name for what the project plan
    /// calls `postReputation`.
    #[sol(rpc)]
    interface IReputationRegistry {
        function giveFeedback(
            uint256 agentId,
            int128  value,
            uint8   valueDecimals,
            string  calldata tag1,
            string  calldata tag2,
            string  calldata endpoint,
            string  calldata feedbackURI,
            bytes32 feedbackHash
        ) external;

        function getFeedback(uint256 agentId, uint256 index)
            external view
            returns (
                address rater,
                int128  value,
                uint8   valueDecimals,
                string  memory tag1,
                string  memory tag2,
                string  memory endpoint,
                string  memory feedbackURI,
                bytes32 feedbackHash,
                uint256 timestamp
            );

        function getFeedbackCount(uint256 agentId) external view returns (uint256);
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// On-chain addresses for the two ERC-8004 registries.
#[derive(Debug, Clone)]
pub struct RegistryAddresses {
    /// Address of the IdentityRegistry (ERC-721 + URIStorage).
    pub identity_registry: Address,
    /// Address of the ReputationRegistry.
    pub reputation_registry: Address,
}

impl RegistryAddresses {
    /// Returns `None` — no registry is deployed on Mantle mainnet (5000) yet.
    ///
    /// Update once the operator runs the deployment steps in
    /// `decisions/0008-erc8004-deployment.md`.
    pub fn mantle_mainnet() -> Option<Self> {
        None
    }

    /// Returns `None` — no registry is deployed on Mantle Sepolia testnet
    /// (5003) yet.
    ///
    /// Update after testnet deployment (see `decisions/0008-erc8004-deployment.md`).
    pub fn mantle_testnet() -> Option<Self> {
        None
    }

    /// Construct addresses for a local `anvil` instance or custom deployment.
    pub fn custom(identity_registry: Address, reputation_registry: Address) -> Self {
        Self { identity_registry, reputation_registry }
    }
}

/// A minted agent token ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TokenId(pub U256);

impl std::fmt::Display for TokenId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Transaction hash from a state-mutating call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxHash(pub B256);

impl std::fmt::Display for TxHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // B256 Display already writes "0x<64 hex chars>"
        write!(f, "{}", self.0)
    }
}

/// Errors returned by [`IdentityClient`].
#[derive(Debug, Error)]
pub enum IdentityError {
    /// JSON-RPC transport error.
    #[error("rpc: {0}")]
    Rpc(String),

    /// On-chain contract call failed or returned unexpected data.
    #[error("contract: {0}")]
    Contract(String),

    /// Signer operation failed.
    #[error("signer: {0}")]
    Signer(String),

    /// Manifest serialisation / deserialisation failed.
    #[error("manifest: {0}")]
    Manifest(String),

    /// The requested registry is not deployed on this chain.
    #[error("registry not deployed on chain {chain_id}: {hint}")]
    RegistryUnavailable { chain_id: u64, hint: String },

    /// ABI encoding / decoding failed.
    #[error("encode: {0}")]
    Encode(String),
}

// ---------------------------------------------------------------------------
// IdentityClient
// ---------------------------------------------------------------------------

/// ERC-8004 client — connect, mint agentURI NFTs, post / read reputation.
///
/// All state-mutating calls require the caller to supply a
/// [`PrivateKeySigner`].  This crate never stores or loads private keys.
///
/// # Chain safety
/// [`connect`][Self::connect] verifies that the `chain_id` argument matches
/// the chain reported by the RPC endpoint.  Tests must use anvil (31337) or
/// Mantle Sepolia (5003); never mainnet (5000) from automated tests.
// DynProvider doesn't implement Debug, so we implement it manually.
pub struct IdentityClient {
    provider: DynProvider,
    addresses: RegistryAddresses,
    chain_id: u64,
    /// Stored so that wallet-backed providers can reconnect without the caller
    /// passing the URL a second time.
    rpc_url: String,
}

impl std::fmt::Debug for IdentityClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityClient")
            .field("rpc_url", &self.rpc_url)
            .field("chain_id", &self.chain_id)
            .field("identity_registry", &self.addresses.identity_registry)
            .field("reputation_registry", &self.addresses.reputation_registry)
            .finish()
    }
}

impl IdentityClient {
    /// Connect to a JSON-RPC endpoint (Mantle mainnet, testnet, or anvil).
    ///
    /// Validates that the detected chain ID matches `chain_id`.
    ///
    /// # Errors
    /// - [`IdentityError::Rpc`] if the endpoint is unreachable or the chain
    ///   ID doesn't match.
    pub async fn connect(
        rpc_url: &str,
        addresses: RegistryAddresses,
        chain_id: u64,
    ) -> Result<Self, IdentityError> {
        if chain_id == 5000 {
            tracing::warn!(
                "Connecting to Mantle mainnet (chain 5000). \
                 Ensure this is intentional — automated tests must NOT use mainnet."
            );
        }

        let provider = ProviderBuilder::new()
            .connect(rpc_url)
            .await
            .map_err(|e| IdentityError::Rpc(e.to_string()))?;

        let detected = provider
            .get_chain_id()
            .await
            .map_err(|e| IdentityError::Rpc(e.to_string()))?;

        if detected != chain_id {
            return Err(IdentityError::Rpc(format!(
                "chain_id mismatch: caller provided {chain_id} but RPC reports {detected}"
            )));
        }

        Ok(Self {
            provider: DynProvider::new(provider),
            addresses,
            chain_id,
            rpc_url: rpc_url.to_owned(),
        })
    }

    /// Mint an `agentURI` NFT for one experimental arm.
    ///
    /// Returns the minted [`TokenId`].
    ///
    /// Token ID is extracted from the ERC-721 `Transfer(address(0), to, tokenId)`
    /// event in the receipt.  ERC-8004 §3.1 mandates ERC-721, so this event
    /// must be present in any compliant deployment.
    pub async fn register(
        &self,
        agent_uri: &url::Url,
        signer: &PrivateKeySigner,
    ) -> Result<TokenId, IdentityError> {
        info!(
            chain_id = self.chain_id,
            agent_uri = %agent_uri,
            "registering agent identity"
        );

        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect(self.rpc_url.as_str())
            .await
            .map_err(|e| IdentityError::Rpc(e.to_string()))?;

        let contract = IIdentityRegistry::new(self.addresses.identity_registry, &provider);

        let receipt = contract
            .register(agent_uri.to_string())
            .send()
            .await
            .map_err(|e| IdentityError::Contract(e.to_string()))?
            .get_receipt()
            .await
            .map_err(|e| IdentityError::Contract(e.to_string()))?;

        // ERC-721 Transfer: keccak256("Transfer(address,address,uint256)")
        // Topics layout: [event_sig, from (indexed), to (indexed), tokenId (indexed)]
        // A mint has from == address(0).
        let erc721_transfer_sig = keccak256(b"Transfer(address,address,uint256)");

        let token_id = receipt
            .inner
            .logs()
            .iter()
            .find_map(|log| {
                let topics = log.topics();
                if topics.len() == 4
                    && topics[0] == erc721_transfer_sig
                    && topics[1] == B256::ZERO
                {
                    Some(U256::from_be_bytes::<32>(topics[3].0))
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                IdentityError::Contract(
                    "no ERC-721 Transfer(mint) event in receipt; \
                     verify the deployed contract emits Transfer(from=0x0, to, tokenId)"
                        .to_string(),
                )
            })?;

        debug!(%token_id, "minted identity token");
        Ok(TokenId(token_id))
    }

    /// Post a reputation update keyed by `setup_id`.
    ///
    /// Encodes `outcome` as JSON, computes keccak256 of that JSON, and calls
    /// `giveFeedback` on the ReputationRegistry.  The P&L value is encoded as
    /// `realized_pnl_usd * 1e6` (6 decimal places, i128).
    pub async fn post_reputation(
        &self,
        agent: TokenId,
        setup_id: Uuid,
        outcome: TradeOutcome,
        signer: &PrivateKeySigner,
    ) -> Result<TxHash, IdentityError> {
        info!(
            chain_id = self.chain_id,
            %agent,
            %setup_id,
            "posting reputation update"
        );

        let wallet = EthereumWallet::from(signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect(self.rpc_url.as_str())
            .await
            .map_err(|e| IdentityError::Rpc(e.to_string()))?;

        let feedback_json = serde_json::to_string(&outcome)
            .map_err(|e| IdentityError::Encode(e.to_string()))?;
        let feedback_hash = B256::from(keccak256(feedback_json.as_bytes()).0);
        let value_raw = (outcome.realized_pnl_usd * 1_000_000.0) as i128;

        let contract = IReputationRegistry::new(self.addresses.reputation_registry, &provider);

        let receipt = contract
            .giveFeedback(
                agent.0,
                value_raw,
                6u8,
                "xianvec".to_string(),
                setup_id.to_string(),
                String::new(),
                feedback_json,
                feedback_hash,
            )
            .send()
            .await
            .map_err(|e| IdentityError::Contract(e.to_string()))?
            .get_receipt()
            .await
            .map_err(|e| IdentityError::Contract(e.to_string()))?;

        let tx_hash = TxHash(receipt.transaction_hash);
        debug!(%tx_hash, "reputation posted");
        Ok(tx_hash)
    }

    /// Read all reputation entries for an agent from the ReputationRegistry.
    ///
    /// `feedbackURI` is expected to contain the inline JSON serialisation of
    /// [`TradeOutcome`] as written by [`Self::post_reputation`].
    pub async fn read_reputation(
        &self,
        agent: TokenId,
    ) -> Result<Vec<ReputationEntry>, IdentityError> {
        let contract =
            IReputationRegistry::new(self.addresses.reputation_registry, &self.provider);

        let count_u256 = contract
            .getFeedbackCount(agent.0)
            .call()
            .await
            .map_err(|e| IdentityError::Contract(e.to_string()))?;

        let count: u64 = count_u256.try_into().unwrap_or(u64::MAX);

        let mut entries = Vec::with_capacity(count as usize);
        for i in 0..count {
            let fb = contract
                .getFeedback(agent.0, U256::from(i))
                .call()
                .await
                .map_err(|e| IdentityError::Contract(format!("getFeedback[{i}]: {e}")))?;

            let outcome: TradeOutcome = serde_json::from_str(&fb.feedbackURI).map_err(|e| {
                IdentityError::Manifest(format!("bad feedback JSON at index {i}: {e}"))
            })?;

            entries.push(ReputationEntry {
                setup_id: outcome.setup_id,
                // The on-chain getFeedback does not return the original tx hash;
                // timestamp is the best available block-level anchor until a
                // subgraph or event-scan layer is added in a later phase.
                tx_hash: format!("block:{}", fb.timestamp),
                block_number: fb.timestamp.try_into().unwrap_or(0),
                outcome,
            });
        }

        Ok(entries)
    }

    /// Chain ID this client is connected to.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::Address;

    // -----------------------------------------------------------------------
    // Unit tests — no network required
    // -----------------------------------------------------------------------

    #[test]
    fn registry_addresses_mainnet_returns_none() {
        assert!(RegistryAddresses::mantle_mainnet().is_none());
    }

    #[test]
    fn registry_addresses_testnet_returns_none() {
        assert!(RegistryAddresses::mantle_testnet().is_none());
    }

    #[test]
    fn token_id_display() {
        let t = TokenId(U256::from(42u64));
        assert_eq!(t.to_string(), "42");
    }

    #[test]
    fn tx_hash_display() {
        let h = TxHash(B256::ZERO);
        let s = h.to_string();
        assert!(s.starts_with("0x"), "should start with 0x: {s}");
        assert_eq!(s.len(), 66, "expected 0x + 64 hex chars, got: {s}");
    }

    #[test]
    fn identity_error_messages() {
        let e = IdentityError::Rpc("timeout".to_string());
        assert!(e.to_string().contains("timeout"));

        let e = IdentityError::RegistryUnavailable {
            chain_id: 5000,
            hint: "not deployed yet".to_string(),
        };
        let msg = e.to_string();
        assert!(msg.contains("5000"), "chain_id in message: {msg}");
        assert!(msg.contains("not deployed yet"), "hint in message: {msg}");
    }

    #[test]
    fn token_id_equality() {
        let a = TokenId(U256::from(1u64));
        let b = TokenId(U256::from(1u64));
        let c = TokenId(U256::from(2u64));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn custom_addresses_roundtrip() {
        let id = Address::from([0xABu8; 20]);
        let rep = Address::from([0xCDu8; 20]);
        let addrs = RegistryAddresses::custom(id, rep);
        assert_eq!(addrs.identity_registry, id);
        assert_eq!(addrs.reputation_registry, rep);
    }

    // -----------------------------------------------------------------------
    // Integration tests — require a running `anvil` instance.
    //
    // Run manually:
    //   anvil &
    //   cargo test -p xianvec-identity -- --ignored --nocapture
    // -----------------------------------------------------------------------

    /// Smoke test: connect to anvil and verify chain ID round-trip.
    ///
    /// To run:
    /// ```sh
    /// anvil &
    /// cargo test -p xianvec-identity anvil_connect_chain_id -- --ignored --nocapture
    /// ```
    #[ignore = "requires local anvil (chain 31337); run: `anvil &` then `cargo test -p xianvec-identity -- --ignored`"]
    #[tokio::test]
    async fn anvil_connect_chain_id() {
        let addrs =
            RegistryAddresses::custom(Address::from([0x11u8; 20]), Address::from([0x22u8; 20]));

        let client = IdentityClient::connect("http://127.0.0.1:8545", addrs, 31337)
            .await
            .expect("anvil should be running on 8545");

        assert_eq!(client.chain_id(), 31337);
    }

    /// End-to-end mint + reputation round-trip against a local anvil instance.
    ///
    /// Requires stub contracts deployed at the addresses below.
    /// See `decisions/0008-erc8004-deployment.md` for the Forge deployment script.
    ///
    /// To run:
    /// ```sh
    /// # Deploy contracts first (see decisions/0008-erc8004-deployment.md)
    /// anvil &
    /// cargo test -p xianvec-identity anvil_mint_and_reputation -- --ignored --nocapture
    /// ```
    #[ignore = "requires anvil + stub contracts deployed; see decisions/0008-erc8004-deployment.md"]
    #[tokio::test]
    async fn anvil_mint_and_reputation() {
        use chrono::Utc;

        // anvil default account 0 private key (publicly known, safe for tests)
        let signer: PrivateKeySigner =
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .parse()
                .expect("valid anvil key");

        let addrs =
            RegistryAddresses::custom(Address::from([0x11u8; 20]), Address::from([0x22u8; 20]));

        let client = IdentityClient::connect("http://127.0.0.1:8545", addrs, 31337)
            .await
            .expect("anvil running");

        let uri: url::Url = "https://example.com/agent.json".parse().unwrap();
        let token_id = client.register(&uri, &signer).await.expect("register");

        let outcome = TradeOutcome {
            setup_id: Uuid::new_v4(),
            realized_pnl_usd: 12.34,
            action: "close".to_string(),
            closed_at: Utc::now(),
        };
        let setup_id = outcome.setup_id;
        let tx_hash = client
            .post_reputation(token_id.clone(), setup_id, outcome.clone(), &signer)
            .await
            .expect("post_reputation");

        assert!(!tx_hash.to_string().is_empty());

        let entries = client.read_reputation(token_id).await.expect("read_reputation");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].setup_id, setup_id);
        assert_eq!(entries[0].outcome.realized_pnl_usd, outcome.realized_pnl_usd);
    }

    /// Verify that connecting with a wrong chain_id is rejected before any tx.
    ///
    /// To run (anvil must be running on :8545):
    /// ```sh
    /// anvil &
    /// cargo test -p xianvec-identity anvil_chain_id_mismatch_caught -- --ignored --nocapture
    /// ```
    #[ignore = "requires local anvil; run: `anvil &` then `cargo test -p xianvec-identity -- --ignored`"]
    #[tokio::test]
    async fn anvil_chain_id_mismatch_caught() {
        let addrs = RegistryAddresses::custom(Address::ZERO, Address::ZERO);
        // anvil is chain 31337; deliberately pass 9999
        let result = IdentityClient::connect("http://127.0.0.1:8545", addrs, 9999).await;
        assert!(
            matches!(result, Err(IdentityError::Rpc(_))),
            "expected Rpc error for chain_id mismatch, got: {result:?}"
        );
    }
}

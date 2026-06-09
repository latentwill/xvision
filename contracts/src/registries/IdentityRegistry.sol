// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721URIStorage} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721URIStorage.sol";

/// @title IdentityRegistry — ERC-8004 §3.1 minimal Identity NFT registry.
/// @notice One ERC-721 token per **agent = strategy = listing** (AM3 resolution,
///         `docs/superpowers/specs/2026-06-08-live-trading-marketplace-spec.md`).
///         `register()` is called once per listed strategy: the minted
///         `agentId` IS the ERC-8004 agent id and corresponds 1:1 to the
///         marketplace listing / strategy (`agentId ↔ agent_id` ULID). The
///         token's `agentURI` resolves to that strategy's manifest. A strategy
///         that forks another may record its parent in the manifest, but on
///         chain each `register()` is an independent agent; there is NO
///         lineage-grouping enforced here.
///
///         (Supersedes the earlier "one NFT per lineage" framing in
///         `2026-05-08-smart-contract-surface-design.md` §3.1.1 — see AM3.)
///
/// @dev Immutable by design — no proxy, no admin surface (blockchain nav doc §3:
///      "these are immutable by design"). The first mint is token id `0`, which
///      the platform self-registration script claims as agent #0 (surface spec
///      §3.5). Token ids increase monotonically from there. The registry imposes
///      no lineage/parent constraint — minting is a plain monotonic ERC-721
///      mint, so the agent-per-strategy model is purely a naming/semantics
///      resolution, not a storage-layout change.
///
///      This is a minimal stub matching the ERC-8004 *draft* §3 interface as
///      consumed by `crates/xvision-identity`. If/when ERC-8004 finalises and a
///      reference registry is deployed on Mantle, this contract is replaced and
///      the Rust `sol!` bindings re-pinned (see ADR 0008 "ABI migration note").
contract IdentityRegistry is ERC721URIStorage {
    /// @dev Next token id to mint. Starts at 0 so agent #0 == the platform.
    uint256 private _nextTokenId;

    /// @notice Emitted on every `register`. Mirrors the indexer-friendly event
    ///         schema in surface spec §6.1 (`tokenId`, `owner` indexed). ERC-721
    ///         also emits `Transfer(address(0), owner, tokenId)` on mint; the
    ///         Rust client reads the token id from that, subgraphs read this.
    event AgentRegistered(uint256 indexed tokenId, address indexed owner, string agentURI);

    constructor() ERC721("XvisionAgent", "XVNA") {}

    /// @notice Mint a new agent identity NFT (one per listed strategy) pointing
    ///         at `agentURI`. Called once per strategy/listing (AM3), not per
    ///         lineage.
    /// @param agentURI Resolvable URI (ipfs://… or https://…) of the strategy
    ///        manifest JSON.
    /// @return agentId The minted token id (also the ERC-8004 agent id, 1:1 with
    ///        the strategy/listing).
    function register(string calldata agentURI) external returns (uint256 agentId) {
        agentId = _nextTokenId++;
        _safeMint(msg.sender, agentId);
        _setTokenURI(agentId, agentURI);
        emit AgentRegistered(agentId, msg.sender, agentURI);
    }

    /// @notice Total number of identity NFTs minted (also the next token id).
    function totalMinted() external view returns (uint256) {
        return _nextTokenId;
    }
}

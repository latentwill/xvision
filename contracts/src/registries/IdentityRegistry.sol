// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721URIStorage} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721URIStorage.sol";

/// @title IdentityRegistry — ERC-8004 §3.1 minimal Identity NFT registry.
/// @notice One ERC-721 token per **lineage** (an evolving strategy line), per
///         the lineage terminology lock in
///         `docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`
///         §3.1.1. The token's `agentURI` resolves to the lineage manifest
///         (which itself carries `parent_lineage_id` for forks); per-variant
///         identity is a content hash recorded off-chain, NOT a separate mint.
///
/// @dev Immutable by design — no proxy, no admin surface (blockchain nav doc §3:
///      "these are immutable by design"). The first mint is token id `0`, which
///      the platform self-registration script claims as agent #0 (surface spec
///      §3.5). Token ids increase monotonically from there.
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

    /// @notice Mint a new lineage identity NFT pointing at `agentURI`.
    /// @param agentURI Resolvable URI (ipfs://… or https://…) of the lineage
    ///        manifest JSON.
    /// @return agentId The minted token id (also the ERC-8004 agent id).
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

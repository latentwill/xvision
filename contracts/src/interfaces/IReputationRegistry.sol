// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title IReputationRegistry — the slice of the ERC-8004 ReputationRegistry
///        that the marketplace contracts depend on for the §3.6 license gate.
/// @notice `ListingRegistry.createListing` calls {setListingForAgent} so the
///         per-agent feedback gate is wired ATOMICALLY at listing creation
///         (agent = strategy = listing, AM3). Without this, a freshly listed
///         agent would stay ungated until a separate manual owner action.
interface IReputationRegistry {
    function setListingForAgent(uint256 agentId, uint256 listingId) external;

    function listingForAgent(uint256 agentId) external view returns (uint256);
}

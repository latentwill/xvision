// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title IIdentityRegistry — the slice of the ERC-8004 IdentityRegistry that
///        the marketplace contracts depend on.
/// @notice `ListingRegistry.createListing` calls `ownerOf` to enforce that the
///         lister owns the lineage NFT they are listing variants of.
interface IIdentityRegistry {
    function register(string calldata agentURI) external returns (uint256 agentId);

    function ownerOf(uint256 tokenId) external view returns (address);

    function tokenURI(uint256 tokenId) external view returns (string memory);
}

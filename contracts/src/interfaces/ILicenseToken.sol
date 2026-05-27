// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title ILicenseToken — ERC-1155 license surface (surface spec §3.3).
/// @notice `tokenId == listingId`. Soulbound by default; per-listing transfer
///         is opted in at listing time and mirrored from `ListingRegistry`.
interface ILicenseToken {
    function authorizedMint(address to, uint256 listingId, uint256 amount) external;

    function isAuthorized(address account) external view returns (bool);

    function setAuthorized(address account, bool allowed) external;

    function transferableForId(uint256 listingId) external view returns (bool);

    function balanceOf(address account, uint256 id) external view returns (uint256);

    event AuthorizedSet(address indexed caller, bool allowed);
}

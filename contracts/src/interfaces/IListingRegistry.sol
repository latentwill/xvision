// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title IListingRegistry — listing CRUD surface (surface spec §3.1).
interface IListingRegistry {
    /// @notice On-chain listing record. `agentNftId` is the LINEAGE NFT;
    ///         `contentHash` identifies the VARIANT within that lineage
    ///         (terminology lock, surface spec §3.1.1).
    struct Listing {
        uint256 listingId; // monotonically increasing
        address seller;
        uint256 agentNftId; // ERC-8004 lineage NFT
        bytes32 contentHash; // keccak256 of canonical variant bundle JSON
        string contentURI; // ipfs://… public metadata or sealed-bundle pointer
        uint8 tier; // 0 = Open, 1 = Sealed
        uint96 priceUSDC; // 6-decimal USDC
        uint16 protocolFeeBps; // snapshotted at create time (rug-resistant)
        bool transferableLicense; // soulbound default = false
        uint64 createdAt;
        bool revoked;
    }

    function createListing(
        uint256 agentNftId,
        bytes32 contentHash,
        string calldata contentURI,
        uint8 tier,
        uint96 priceUSDC,
        bool transferableLicense
    ) external returns (uint256 listingId);

    function updateListing(uint256 listingId, bytes32 contentHash, string calldata contentURI) external;

    /// @notice Seller-only in-place repricing. `newPriceUSDC == 0` makes the
    ///         listing free (open/clone path); everything else (tier, fee
    ///         snapshot, content, transferable flag) is unchanged.
    function updatePrice(uint256 listingId, uint96 newPriceUSDC) external;

    function revokeListing(uint256 listingId) external;

    function getListing(uint256 listingId) external view returns (Listing memory);

    /// @notice Convenience accessor used by `LicenseToken` to enforce the
    ///         soulbound default without decoding the full struct.
    function transferableForListing(uint256 listingId) external view returns (bool);

    /// @notice True once a listing id has been created (even if later revoked).
    function listingExists(uint256 listingId) external view returns (bool);

    event ListingCreated(
        uint256 indexed listingId,
        address indexed seller,
        uint256 indexed agentNftId,
        bytes32 contentHash,
        uint8 tier,
        uint96 priceUSDC
    );
    event ListingUpdated(uint256 indexed listingId, bytes32 contentHash, string contentURI);
    event ListingPriceUpdated(uint256 indexed listingId, uint96 oldPriceUSDC, uint96 newPriceUSDC);
    event ListingRevoked(uint256 indexed listingId, address indexed seller);
}

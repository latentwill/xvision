// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";

import {IListingRegistry} from "./interfaces/IListingRegistry.sol";
import {IIdentityRegistry} from "./interfaces/IIdentityRegistry.sol";
import {IMarketplace} from "./interfaces/IMarketplace.sol";

/// @title ListingRegistry — on-chain listing CRUD (surface spec §3.1).
/// @notice Listings are variant-scoped (`contentHash`) and owned by
///         lineage-scoped NFTs (`agentNftId`). A creator can have multiple
///         active listings under one lineage NFT — one per variant they
///         monetise (surface spec §3.1.1).
///
/// @dev UUPS proxy + operator-EOA admin for V2 testnet. The admin can upgrade
///      the implementation but CANNOT rewrite existing listings or change a
///      listing's snapshotted `protocolFeeBps` (surface spec §7.3).
contract ListingRegistry is Initializable, OwnableUpgradeable, UUPSUpgradeable, IListingRegistry {
    /// @dev ERC-8004 IdentityRegistry — used to enforce lineage ownership.
    IIdentityRegistry private _identityRegistry;

    /// @dev Marketplace — read at create time to snapshot `protocolFeeBps`.
    ///      Set once post-deploy (ListingRegistry deploys before Marketplace in
    ///      the §8.3 sequence).
    address private _marketplace;

    /// @dev Next listing id. Starts at 1 so id 0 is an unambiguous "none".
    uint256 private _nextListingId;

    /// @dev listingId => listing record.
    mapping(uint256 => Listing) private _listings;

    /// @dev Storage gap (surface spec §7.5). Four slots used above.
    uint256[46] private __gap;

    error NotLineageOwner(uint256 agentNftId, address caller);
    error InvalidTier(uint8 tier);
    error MarketplaceNotWired();
    error MarketplaceAlreadySet();
    error UnknownListing(uint256 listingId);
    error NotSeller(uint256 listingId, address caller);
    error AlreadyRevoked(uint256 listingId);
    error ZeroAddress();

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    /// @param admin Operator EOA that owns the proxy.
    /// @param identityRegistry_ Deployed ERC-8004 IdentityRegistry.
    function initialize(address admin, address identityRegistry_) external initializer {
        if (admin == address(0) || identityRegistry_ == address(0)) revert ZeroAddress();
        __Ownable_init(admin);
        __UUPSUpgradeable_init();
        _identityRegistry = IIdentityRegistry(identityRegistry_);
        _nextListingId = 1;
    }

    // -----------------------------------------------------------------------
    // Wiring (one-shot, admin)
    // -----------------------------------------------------------------------

    /// @notice Wire the Marketplace. Callable exactly once by the admin.
    function setMarketplace(address marketplace_) external onlyOwner {
        if (marketplace_ == address(0)) revert ZeroAddress();
        if (_marketplace != address(0)) revert MarketplaceAlreadySet();
        _marketplace = marketplace_;
    }

    function marketplace() external view returns (address) {
        return _marketplace;
    }

    function identityRegistry() external view returns (address) {
        return address(_identityRegistry);
    }

    // -----------------------------------------------------------------------
    // CRUD
    // -----------------------------------------------------------------------

    /// @inheritdoc IListingRegistry
    function createListing(
        uint256 agentNftId,
        bytes32 contentHash,
        string calldata contentURI,
        uint8 tier,
        uint96 priceUSDC,
        bool transferableLicense
    ) external override returns (uint256 listingId) {
        // Lineage ownership: reverts inside ownerOf if the NFT does not exist.
        if (_identityRegistry.ownerOf(agentNftId) != msg.sender) {
            revert NotLineageOwner(agentNftId, msg.sender);
        }
        if (tier > 1) revert InvalidTier(tier);
        if (_marketplace == address(0)) revert MarketplaceNotWired();

        // Snapshot the protocol fee at create time so a later fee bump cannot
        // retroactively rug this listing (surface spec §3.1, §5.1).
        uint16 feeBps = IMarketplace(_marketplace).protocolFeeBps();

        listingId = _nextListingId++;
        _listings[listingId] = Listing({
            listingId: listingId,
            seller: msg.sender,
            agentNftId: agentNftId,
            contentHash: contentHash,
            contentURI: contentURI,
            tier: tier,
            priceUSDC: priceUSDC,
            protocolFeeBps: feeBps,
            transferableLicense: transferableLicense,
            createdAt: uint64(block.timestamp),
            revoked: false
        });

        emit ListingCreated(listingId, msg.sender, agentNftId, contentHash, tier, priceUSDC);
    }

    /// @inheritdoc IListingRegistry
    /// @dev Content rotation only — price, tier, fee snapshot, and the
    ///      transferable flag are immutable after creation.
    function updateListing(uint256 listingId, bytes32 contentHash, string calldata contentURI)
        external
        override
    {
        Listing storage l = _listings[listingId];
        if (l.seller == address(0)) revert UnknownListing(listingId);
        if (l.seller != msg.sender) revert NotSeller(listingId, msg.sender);

        l.contentHash = contentHash;
        l.contentURI = contentURI;
        emit ListingUpdated(listingId, contentHash, contentURI);
    }

    /// @inheritdoc IListingRegistry
    function revokeListing(uint256 listingId) external override {
        Listing storage l = _listings[listingId];
        if (l.seller == address(0)) revert UnknownListing(listingId);
        if (l.seller != msg.sender) revert NotSeller(listingId, msg.sender);
        if (l.revoked) revert AlreadyRevoked(listingId);

        l.revoked = true;
        emit ListingRevoked(listingId, msg.sender);
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// @inheritdoc IListingRegistry
    function getListing(uint256 listingId) external view override returns (Listing memory) {
        Listing memory l = _listings[listingId];
        if (l.seller == address(0)) revert UnknownListing(listingId);
        return l;
    }

    /// @inheritdoc IListingRegistry
    function transferableForListing(uint256 listingId) external view override returns (bool) {
        return _listings[listingId].transferableLicense;
    }

    /// @inheritdoc IListingRegistry
    function listingExists(uint256 listingId) external view override returns (bool) {
        return _listings[listingId].seller != address(0);
    }

    /// @notice Total listings ever created (next id minus the id-1 offset).
    function totalListings() external view returns (uint256) {
        return _nextListingId - 1;
    }

    // -----------------------------------------------------------------------
    // UUPS
    // -----------------------------------------------------------------------

    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}
}

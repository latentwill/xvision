// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";

import {IListingRegistry} from "./interfaces/IListingRegistry.sol";
import {IIdentityRegistry} from "./interfaces/IIdentityRegistry.sol";
import {IMarketplace} from "./interfaces/IMarketplace.sol";
import {IReputationRegistry} from "./interfaces/IReputationRegistry.sol";

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

    /// @dev ReputationRegistry — the §3.6 license-gate store. Wired once
    ///      post-deploy (Finding 1). When set, {createListing} calls
    ///      `setListingForAgent(agentNftId, listingId)` so the feedback gate is
    ///      active ATOMICALLY at listing creation rather than depending on a
    ///      separate manual owner action. Optional: if unset, {createListing}
    ///      simply skips the call (the listing is still created).
    IReputationRegistry private _reputationRegistry;

    /// @dev Storage gap (surface spec §7.5). FIVE sequential slots are used
    ///      above now (`_identityRegistry`, `_marketplace`, `_nextListingId`,
    ///      `_listings`, `_reputationRegistry`), occupying slots 0..4. The gap
    ///      shrinks from [46] to [45] so the total reserved stays at 50 slots
    ///      (5 used + 45 gap). Pre-deploy layout change — no live proxy exists
    ///      yet, so reordering is safe; the invariant kept is the fixed
    ///      50-slot reservation.
    uint256[45] private __gap;

    error NotLineageOwner(uint256 agentNftId, address caller);
    error InvalidTier(uint8 tier);
    error MarketplaceNotWired();
    error MarketplaceAlreadySet();
    error UnknownListing(uint256 listingId);
    error NotSeller(uint256 listingId, address caller);
    error AlreadyRevoked(uint256 listingId);
    error ZeroAddress();
    error ReputationRegistryAlreadySet();
    /// @dev Finding 3: a free (priceUSDC == 0) AND transferable listing is
    ///      nonsensical — the L-1 per-recipient cap reads live balance, so the
    ///      recipient could mint, transfer the token away, and mint again
    ///      without bound. Forbid the combination at creation.
    error FreeTransferableForbidden();

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    /// @param admin Operator EOA that owns the proxy.
    /// @param identityRegistry_ Deployed ERC-8004 IdentityRegistry.
    function initialize(address admin, address identityRegistry_) external initializer {
        if (admin == address(0) || identityRegistry_ == address(0)) revert ZeroAddress();
        __Ownable_init(admin);
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

    /// @notice Wire the ReputationRegistry. Callable exactly once by the admin.
    ///         Once set, {createListing} auto-wires the §3.6 feedback gate by
    ///         calling `setListingForAgent` on it (Finding 1). For that call to
    ///         succeed, this ListingRegistry must also be set as the registrar
    ///         on the ReputationRegistry (`setListingRegistrar`).
    function setReputationRegistry(address reputationRegistry_) external onlyOwner {
        if (reputationRegistry_ == address(0)) revert ZeroAddress();
        if (address(_reputationRegistry) != address(0)) revert ReputationRegistryAlreadySet();
        _reputationRegistry = IReputationRegistry(reputationRegistry_);
    }

    function marketplace() external view returns (address) {
        return _marketplace;
    }

    function reputationRegistry() external view returns (address) {
        return address(_reputationRegistry);
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
        // Finding 3: a free + transferable license would let the recipient mint,
        // transfer the token away, and mint again indefinitely (the L-1 cap reads
        // live balance). The combination is nonsensical — forbid it.
        if (priceUSDC == 0 && transferableLicense) revert FreeTransferableForbidden();
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

        // Finding 1: wire the §3.6 license gate ATOMICALLY at listing creation
        // (agent = strategy = listing, AM3 1:1). Without this, a freshly listed
        // agent stays UNGATED — anyone could post feedback without holding a
        // license — until a separate manual owner call. This ListingRegistry is
        // the authorized registrar on the ReputationRegistry, so the call sets
        // `_listingForAgent[agentNftId] = listingId` and the gate is live the
        // instant `createListing` returns. The ReputationRegistry is a contract
        // we deploy and own; `setListingForAgent` performs no external call back
        // into this contract and mutates only its own storage, so there is no
        // reentrancy surface (it also runs after all of this function's effects,
        // satisfying checks-effects-interactions). If unset, skip (the listing
        // is still created), but the wired deploy path always sets it.
        if (address(_reputationRegistry) != address(0)) {
            _reputationRegistry.setListingForAgent(agentNftId, listingId);
        }
    }

    /// @inheritdoc IListingRegistry
    /// @dev Content rotation only — price, tier, fee snapshot, and the
    ///      transferable flag are immutable after creation.
    function updateListing(uint256 listingId, bytes32 contentHash, string calldata contentURI) external override {
        Listing storage l = _listings[listingId];
        if (l.seller == address(0)) revert UnknownListing(listingId);
        if (l.seller != msg.sender) revert NotSeller(listingId, msg.sender);

        l.contentHash = contentHash;
        l.contentURI = contentURI;
        emit ListingUpdated(listingId, contentHash, contentURI);
    }

    /// @inheritdoc IListingRegistry
    /// @dev Price-only mutation (tier, fee snapshot, content, transferable flag
    ///      stay fixed). Preserves the create-time `FreeTransferableForbidden`
    ///      invariant: a transferable listing can never be repriced to free.
    function updatePrice(uint256 listingId, uint96 newPriceUSDC) external override {
        Listing storage l = _listings[listingId];
        if (l.seller == address(0)) revert UnknownListing(listingId);
        if (l.seller != msg.sender) revert NotSeller(listingId, msg.sender);
        if (l.revoked) revert AlreadyRevoked(listingId);
        if (newPriceUSDC == 0 && l.transferableLicense) revert FreeTransferableForbidden();

        uint96 oldPriceUSDC = l.priceUSDC;
        l.priceUSDC = newPriceUSDC;
        emit ListingPriceUpdated(listingId, oldPriceUSDC, newPriceUSDC);
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

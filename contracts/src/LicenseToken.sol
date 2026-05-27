// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {ERC1155Upgradeable} from "@openzeppelin/contracts-upgradeable/token/ERC1155/ERC1155Upgradeable.sol";

import {ILicenseToken} from "./interfaces/ILicenseToken.sol";
import {IListingRegistry} from "./interfaces/IListingRegistry.sol";

/// @title LicenseToken — ERC-1155 license token (surface spec §3.3).
/// @notice One token id per listing (`tokenId == listingId`); one unit minted
///         per purchase. Soulbound by default — a license is bound to the
///         buyer's wallet so the wallet that holds the token IS the licensee
///         (this is what lets a resource server verify access with a single
///         `balanceOf` read, surface spec §4.4). A listing can opt into a
///         transferable license at listing time; that flag is read live from
///         `ListingRegistry` and is immutable per listing.
///
/// @dev UUPS proxy + operator-EOA admin for V2 testnet (surface spec §7.1–§7.2).
///      Authorised minters in v1: `Marketplace` only. The authorized-minter
///      pattern lets future settlement contracts (subscription, pay-per-fire)
///      mint without redeploying this contract (surface spec §3.3).
contract LicenseToken is Initializable, ERC1155Upgradeable, OwnableUpgradeable, UUPSUpgradeable, ILicenseToken {
    /// @dev Addresses allowed to call {authorizedMint} (v1: the Marketplace).
    mapping(address => bool) private _authorized;

    /// @dev Source of truth for per-listing transferability. Set once,
    ///      post-deploy, by the deploy script (LicenseToken deploys before
    ///      ListingRegistry in the surface-spec §8.3 sequence). Making it
    ///      one-shot is what guarantees "admin cannot change a listing's
    ///      transferable flag" (surface spec §7.3) — the admin wires the
    ///      registry exactly once, then per-listing flags come from the
    ///      registry, where they are immutable after creation.
    IListingRegistry private _listingRegistry;

    /// @dev Storage gap for safe UUPS upgrades (surface spec §7.5). Two slots
    ///      used above, so reserve 48 to keep 50 total reserved.
    uint256[48] private __gap;

    error NotAuthorizedMinter(address caller);
    error Soulbound(uint256 listingId);
    error ListingRegistryAlreadySet();
    error ZeroAddress();

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    /// @param admin Operator EOA that owns the proxy (upgrade + minter-set authority).
    /// @param uri_ ERC-1155 metadata URI template (e.g. an `{id}`-templated gateway URL).
    function initialize(address admin, string calldata uri_) external initializer {
        if (admin == address(0)) revert ZeroAddress();
        __ERC1155_init(uri_);
        __Ownable_init(admin);
    }

    // -----------------------------------------------------------------------
    // Wiring (one-shot, admin)
    // -----------------------------------------------------------------------

    /// @notice Wire the ListingRegistry. Callable exactly once by the admin.
    function setListingRegistry(address registry) external onlyOwner {
        if (registry == address(0)) revert ZeroAddress();
        if (address(_listingRegistry) != address(0)) revert ListingRegistryAlreadySet();
        _listingRegistry = IListingRegistry(registry);
    }

    function listingRegistry() external view returns (address) {
        return address(_listingRegistry);
    }

    // -----------------------------------------------------------------------
    // Minting + authorisation
    // -----------------------------------------------------------------------

    /// @inheritdoc ILicenseToken
    function authorizedMint(address to, uint256 listingId, uint256 amount) external override {
        if (!_authorized[msg.sender]) revert NotAuthorizedMinter(msg.sender);
        _mint(to, listingId, amount, "");
    }

    /// @inheritdoc ILicenseToken
    function setAuthorized(address account, bool allowed) external override onlyOwner {
        if (account == address(0)) revert ZeroAddress();
        _authorized[account] = allowed;
        emit AuthorizedSet(account, allowed);
    }

    /// @inheritdoc ILicenseToken
    function isAuthorized(address account) external view override returns (bool) {
        return _authorized[account];
    }

    /// @inheritdoc ILicenseToken
    /// @dev Resolves the `balanceOf` collision between `ERC1155Upgradeable`
    ///      (concrete) and `ILicenseToken` (declared) — the surface a resource
    ///      server reads to verify a license (§4.4).
    function balanceOf(address account, uint256 id)
        public
        view
        override(ERC1155Upgradeable, ILicenseToken)
        returns (uint256)
    {
        return super.balanceOf(account, id);
    }

    // -----------------------------------------------------------------------
    // Transferability / soulbound enforcement
    // -----------------------------------------------------------------------

    /// @inheritdoc ILicenseToken
    /// @dev Reads live from ListingRegistry. The per-listing transferable flag
    ///      is immutable post-create (updateListing only rotates content), so
    ///      the live read equals the value at mint time. Defaults to `false`
    ///      (soulbound) until the registry is wired.
    function transferableForId(uint256 listingId) public view override returns (bool) {
        if (address(_listingRegistry) == address(0)) return false;
        return _listingRegistry.transferableForListing(listingId);
    }

    /// @dev OZ v5 routes mint/burn/transfer through `_update` (the v4
    ///      `_beforeTokenTransfer` referenced in the spec). A real transfer has
    ///      both `from` and `to` non-zero; mint (`from==0`) and burn (`to==0`)
    ///      are always allowed.
    function _update(address from, address to, uint256[] memory ids, uint256[] memory values) internal override {
        if (from != address(0) && to != address(0)) {
            uint256 len = ids.length;
            for (uint256 i = 0; i < len; ++i) {
                if (!transferableForId(ids[i])) revert Soulbound(ids[i]);
            }
        }
        super._update(from, to, ids, values);
    }

    // -----------------------------------------------------------------------
    // UUPS
    // -----------------------------------------------------------------------

    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}
}

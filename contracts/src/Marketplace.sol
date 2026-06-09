// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {PausableUpgradeable} from "@openzeppelin/contracts-upgradeable/utils/PausableUpgradeable.sol";
import {ReentrancyGuardUpgradeable} from "@openzeppelin/contracts-upgradeable/utils/ReentrancyGuardUpgradeable.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import {IMarketplace} from "./interfaces/IMarketplace.sol";
import {IListingRegistry} from "./interfaces/IListingRegistry.sol";
import {ILicenseToken} from "./interfaces/ILicenseToken.sol";
import {IERC3009} from "./interfaces/IERC3009.sol";
import {Splits} from "./libraries/Splits.sol";

/// @title Marketplace — sale + commission split (surface spec §3.2, §4, §5).
/// @notice Two settlement paths into one contract: a direct `buy` (human with a
///         wallet: approve + buy, two txs) and an x402 `buyWithAuthorization`
///         (agent: sign an EIP-3009 authorization off-chain, a facilitator
///         submits one tx). Both pull USDC, split 95% seller / 5% protocol
///         (configurable, snapshotted per listing), and mint one license.
///
/// @dev UUPS proxy + operator-EOA admin for V2 testnet. USDC.e on Mantle is the
///      only sale currency. `buy`/`buyWithAuthorization` are `nonReentrant`;
///      funds move before the license mint, and the contract holds no state an
///      ERC-1155 receive hook could exploit.
contract Marketplace is
    Initializable,
    OwnableUpgradeable,
    PausableUpgradeable,
    ReentrancyGuardUpgradeable,
    UUPSUpgradeable,
    IMarketplace
{
    using SafeERC20 for IERC20;

    /// @notice Hard ceiling on the protocol fee. Raising it requires a contract
    ///         upgrade — an explicit "we changed the deal" signal (spec §3.2).
    uint16 public constant MAX_PROTOCOL_FEE_BPS = 1000; // 10%

    IListingRegistry private _listingRegistry;
    ILicenseToken private _licenseToken;
    address private _usdc;
    address private _feeRecipient;
    uint16 private _protocolFeeBps;

    /// @dev Storage gap (surface spec §7.5). This contract reserves 50 total
    ///      sequential storage slots (slots 0..49). Only Marketplace's OWN vars
    ///      sit in sequential storage now: `_listingRegistry`, `_licenseToken`,
    ///      `_usdc`, and `_feeRecipient`+`_protocolFeeBps` (which pack into one
    ///      slot) — 4 slots, occupying slots 0..3. The inherited OZ upgradeable
    ///      bases (Ownable, Pausable, ReentrancyGuard, UUPS) all use ERC-7201
    ///      namespaced storage and consume ZERO sequential slots. (Previously
    ///      the NON-upgradeable ReentrancyGuard reserved a sequential `_status`
    ///      slot at slot 0, so the gap was [45] after 5 used slots; switching to
    ///      ReentrancyGuardUpgradeable frees that slot, so the gap grows to [46]
    ///      to keep the total reserved at 50.)
    uint256[46] private __gap;

    error ListingRevoked(uint256 listingId);
    error FeeTooHigh(uint16 bps);
    error ZeroAddress();
    error BadAuthorizationTarget(address to);
    error BadAuthorizationValue(uint256 value, uint96 price);
    /// @dev M-2: on the x402 path the license recipient must be the EIP-3009
    ///      payer (`auth.from`), so a facilitator cannot redirect the soulbound
    ///      license away from the account that paid.
    error RecipientMustBePayer();
    /// @dev L-1: a free (priceUSDC == 0) listing mints at most one license per
    ///      recipient.
    error AlreadyOwnsFreeLicense();

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address admin,
        address listingRegistry_,
        address licenseToken_,
        address usdc_,
        address feeRecipient_,
        uint16 initialFeeBps
    ) external initializer {
        if (
            admin == address(0) || listingRegistry_ == address(0) || licenseToken_ == address(0) || usdc_ == address(0)
                || feeRecipient_ == address(0)
        ) revert ZeroAddress();
        if (initialFeeBps > MAX_PROTOCOL_FEE_BPS) revert FeeTooHigh(initialFeeBps);

        __Ownable_init(admin);
        __Pausable_init();
        __ReentrancyGuard_init();

        _listingRegistry = IListingRegistry(listingRegistry_);
        _licenseToken = ILicenseToken(licenseToken_);
        _usdc = usdc_;
        _feeRecipient = feeRecipient_;
        _protocolFeeBps = initialFeeBps;
    }

    // -----------------------------------------------------------------------
    // Buy paths
    // -----------------------------------------------------------------------

    /// @inheritdoc IMarketplace
    /// @dev Direct path. Buyer must `USDC.approve(this, price)` first. Pulls the
    ///      full price into the contract, then splits it out.
    function buy(uint256 listingId, address recipient)
        external
        override
        nonReentrant
        whenNotPaused
        returns (uint256 licenseTokenId)
    {
        IListingRegistry.Listing memory l = _loadActive(listingId);

        if (l.priceUSDC > 0) {
            IERC20(_usdc).safeTransferFrom(msg.sender, address(this), l.priceUSDC);
        }

        // purchasePath = 0 (direct). payerKind derivation is a v1 placeholder
        // (see _finalize).
        return _finalize(l, recipient, 0);
    }

    /// @inheritdoc IMarketplace
    /// @dev x402 path. The buyer's signed EIP-3009 authorization must target
    ///      THIS contract (`auth.to == address(this)`) for exactly the listing
    ///      price. `msg.sender` is the facilitator submitting it. One tx settles
    ///      USDC and mints the license.
    function buyWithAuthorization(uint256 listingId, address recipient, TransferAuthorization calldata auth)
        external
        override
        nonReentrant
        whenNotPaused
        returns (uint256 licenseTokenId)
    {
        // M-2: bind the license to the payer. The EIP-3009 authorization is
        // signed over (from, to, value, validAfter, validBefore, nonce) — the
        // license `recipient` is NOT part of that signed message, so a
        // facilitator/front-runner could otherwise submit the buyer's signed
        // auth while redirecting the soulbound license to themselves. Require
        // the recipient to be the payer (`auth.from`).
        if (recipient != auth.from) revert RecipientMustBePayer();

        IListingRegistry.Listing memory l = _loadActive(listingId);

        if (auth.value != l.priceUSDC) revert BadAuthorizationValue(auth.value, l.priceUSDC);

        if (l.priceUSDC > 0) {
            if (auth.to != address(this)) revert BadAuthorizationTarget(auth.to);
            // Pulls `auth.value` from `auth.from` into this contract atomically;
            // reverts (consuming the single-use nonce) on a bad signature or
            // insufficient balance.
            IERC3009(_usdc)
                .transferWithAuthorization(
                    auth.from,
                    auth.to,
                    auth.value,
                    auth.validAfter,
                    auth.validBefore,
                    auth.nonce,
                    auth.v,
                    auth.r,
                    auth.s
                );
        }

        // purchasePath = 1 (x402).
        return _finalize(l, recipient, 1);
    }

    /// @dev Shared settlement: split the price already held by this contract,
    ///      pay out, mint one license, emit `Sold`.
    function _finalize(IListingRegistry.Listing memory l, address recipient, uint8 purchasePath)
        private
        returns (uint256 licenseTokenId)
    {
        (uint96 sellerProceeds, uint96 protocolProceeds) = Splits.computeSplit(l.priceUSDC, l.protocolFeeBps);

        if (l.priceUSDC > 0) {
            if (sellerProceeds > 0) IERC20(_usdc).safeTransfer(l.seller, sellerProceeds);
            if (protocolProceeds > 0) IERC20(_usdc).safeTransfer(_feeRecipient, protocolProceeds);
        }

        // tokenId == listingId (surface spec §3.3). Mint last.
        licenseTokenId = l.listingId;

        // L-1: free listings (priceUSDC == 0) are capped at one license per
        // recipient — without a payment to throttle them, anyone could mint
        // unlimited units. Paid listings are unaffected (re-purchase allowed).
        if (l.priceUSDC == 0 && _licenseToken.balanceOf(recipient, licenseTokenId) != 0) {
            revert AlreadyOwnsFreeLicense();
        }

        _licenseToken.authorizedMint(recipient, licenseTokenId, 1);

        // payerKind: v1 PLACEHOLDER — mirrors purchasePath. Exact derivation
        // deferred (spec §3.2 "locked in Phase 1/5"; refinement ticket in the
        // program plan §7.1). Typed uint16 (wider than purchasePath) so a future
        // impl can encode richer payer identity without an ABI change. Indexers
        // MUST NOT derive analytics from this in v1.
        uint16 payerKind = purchasePath;

        emit Sold(
            l.listingId,
            l.agentNftId,
            recipient,
            l.priceUSDC,
            sellerProceeds,
            protocolProceeds,
            licenseTokenId,
            payerKind,
            purchasePath
        );
    }

    /// @dev Load a listing and require it to be sellable. `getListing` reverts
    ///      with `UnknownListing` if the id was never created.
    function _loadActive(uint256 listingId) private view returns (IListingRegistry.Listing memory l) {
        l = _listingRegistry.getListing(listingId);
        if (l.revoked) revert ListingRevoked(listingId);
    }

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    /// @inheritdoc IMarketplace
    function setProtocolFeeBps(uint16 newBps) external override onlyOwner {
        if (newBps > MAX_PROTOCOL_FEE_BPS) revert FeeTooHigh(newBps);
        uint16 old = _protocolFeeBps;
        _protocolFeeBps = newBps;
        emit ProtocolFeeBpsChanged(old, newBps);
    }

    /// @inheritdoc IMarketplace
    function setFeeRecipient(address newRecipient) external override onlyOwner {
        if (newRecipient == address(0)) revert ZeroAddress();
        address old = _feeRecipient;
        _feeRecipient = newRecipient;
        emit FeeRecipientChanged(old, newRecipient);
    }

    /// @notice Emergency stop for both buy paths. V2: operator EOA; V4: governed.
    ///         Pause cannot mint, burn, transfer, or change fees.
    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// @inheritdoc IMarketplace
    function protocolFeeBps() external view override returns (uint16) {
        return _protocolFeeBps;
    }

    /// @inheritdoc IMarketplace
    function feeRecipient() external view override returns (address) {
        return _feeRecipient;
    }

    function listingRegistry() external view returns (address) {
        return address(_listingRegistry);
    }

    function licenseToken() external view returns (address) {
        return address(_licenseToken);
    }

    function usdc() external view returns (address) {
        return _usdc;
    }

    // -----------------------------------------------------------------------
    // UUPS
    // -----------------------------------------------------------------------

    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}
}

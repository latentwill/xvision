// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {Marketplace} from "../../src/Marketplace.sol";
import {IMarketplace} from "../../src/interfaces/IMarketplace.sol";
import {ReentrantReceiver} from "../mocks/ReentrantReceiver.sol";
import {PausableUpgradeable} from "@openzeppelin/contracts-upgradeable/utils/PausableUpgradeable.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

contract MarketplaceTest is BaseTest {
    address seller = makeAddr("seller");
    address buyer = makeAddr("buyer");
    address facilitator = makeAddr("facilitator");

    uint96 constant PRICE = 15_000_000; // 15 USDC

    function _listed(uint96 price, bool transferable) internal returns (uint256 lid) {
        uint256 lineage = _mintLineage(seller);
        lid = _createListing(seller, lineage, price, transferable);
    }

    // ---- direct buy ----------------------------------------------------

    function test_buy_direct_splitsAndMints() public {
        uint256 lid = _listed(PRICE, false);
        _fundAndApprove(buyer, PRICE);

        vm.prank(buyer);
        uint256 tokenId = market.buy(lid, buyer);

        uint96 fee = uint96((uint256(PRICE) * INITIAL_FEE_BPS) / 10_000); // 750_000
        assertEq(tokenId, lid);
        assertEq(license.balanceOf(buyer, lid), 1);
        assertEq(usdc.balanceOf(seller), PRICE - fee, "seller gets 95%");
        assertEq(usdc.balanceOf(feeRecipient), fee, "protocol gets 5%");
        assertEq(usdc.balanceOf(address(market)), 0, "no funds stuck");
    }

    function test_buy_emitsSold() public {
        uint256 lineage;
        {
            lineage = _mintLineage(seller);
        }
        uint256 lid = _createListing(seller, lineage, PRICE, false);
        _fundAndApprove(buyer, PRICE);

        uint96 fee = uint96((uint256(PRICE) * INITIAL_FEE_BPS) / 10_000);
        vm.expectEmit(true, true, true, true, address(market));
        emit IMarketplace.Sold(lid, lineage, buyer, PRICE, PRICE - fee, fee, lid, 0, 0);
        vm.prank(buyer);
        market.buy(lid, buyer);
    }

    function test_buy_revert_revoked() public {
        uint256 lid = _listed(PRICE, false);
        vm.prank(seller);
        listings.revokeListing(lid);
        _fundAndApprove(buyer, PRICE);

        vm.prank(buyer);
        vm.expectRevert(abi.encodeWithSelector(Marketplace.ListingRevoked.selector, lid));
        market.buy(lid, buyer);
    }

    function test_buy_revert_whenPaused() public {
        uint256 lid = _listed(PRICE, false);
        _fundAndApprove(buyer, PRICE);
        market.pause();

        vm.prank(buyer);
        vm.expectRevert(PausableUpgradeable.EnforcedPause.selector);
        market.buy(lid, buyer);
    }

    /// @dev Buy uses the listing's snapshotted fee, not the marketplace's
    ///      current fee.
    function test_buy_usesSnapshottedFee() public {
        uint256 lid = _listed(PRICE, false); // snapshot 5%
        market.setProtocolFeeBps(1000); // bump current fee to 10%
        _fundAndApprove(buyer, PRICE);

        vm.prank(buyer);
        market.buy(lid, buyer);

        uint96 feeAt5pct = uint96((uint256(PRICE) * 500) / 10_000);
        assertEq(usdc.balanceOf(feeRecipient), feeAt5pct, "charged the 5% snapshot, not 10%");
    }

    function test_buy_freeListing_mintsNoTransfer() public {
        uint256 lid = _listed(0, false);
        // buyer needs no funds / approval
        vm.prank(buyer);
        market.buy(lid, buyer);

        assertEq(license.balanceOf(buyer, lid), 1);
        assertEq(usdc.balanceOf(feeRecipient), 0);
        assertEq(usdc.balanceOf(seller), 0);
    }

    function test_buy_recipientDiffersFromPayer() public {
        uint256 lid = _listed(PRICE, false);
        address recipient = makeAddr("recipient");
        _fundAndApprove(buyer, PRICE);

        vm.prank(buyer);
        market.buy(lid, recipient);
        assertEq(license.balanceOf(recipient, lid), 1);
        assertEq(license.balanceOf(buyer, lid), 0);
    }

    function test_buy_maxPrice_noOverflow() public {
        uint96 max = type(uint96).max;
        uint256 lid = _listed(max, false);
        _fundAndApprove(buyer, max);

        vm.prank(buyer);
        market.buy(lid, buyer);

        uint96 fee = uint96((uint256(max) * INITIAL_FEE_BPS) / 10_000);
        assertEq(uint256(usdc.balanceOf(seller)) + usdc.balanceOf(feeRecipient), max, "split sums to price");
        assertEq(usdc.balanceOf(feeRecipient), fee);
    }

    function test_buy_reentrancy_blocked() public {
        uint256 lid = _listed(PRICE, false);
        ReentrantReceiver recv = new ReentrantReceiver(IMarketplace(address(market)), lid);
        _fundAndApprove(buyer, PRICE);

        // Minting to the reentrant receiver triggers onERC1155Received, which
        // re-enters buy(); the nonReentrant guard reverts and bubbles up.
        vm.prank(buyer);
        vm.expectRevert(ReentrancyGuard.ReentrancyGuardReentrantCall.selector);
        market.buy(lid, address(recv));
    }

    // ---- x402 buyWithAuthorization -------------------------------------

    function _auth(address from, uint96 value, bytes32 nonce)
        internal
        view
        returns (IMarketplace.TransferAuthorization memory)
    {
        return IMarketplace.TransferAuthorization({
            from: from,
            to: address(market),
            value: value,
            validAfter: 0,
            validBefore: type(uint256).max,
            nonce: nonce,
            v: 0,
            r: bytes32(0),
            s: bytes32(0)
        });
    }

    function test_buyWithAuthorization_x402_happy() public {
        uint256 lid = _listed(PRICE, false);
        usdc.mint(buyer, PRICE); // no approval needed on the x402 path

        IMarketplace.TransferAuthorization memory auth = _auth(buyer, PRICE, keccak256("n1"));
        vm.prank(facilitator);
        uint256 tokenId = market.buyWithAuthorization(lid, buyer, auth);

        uint96 fee = uint96((uint256(PRICE) * INITIAL_FEE_BPS) / 10_000);
        assertEq(tokenId, lid);
        assertEq(license.balanceOf(buyer, lid), 1);
        assertEq(usdc.balanceOf(seller), PRICE - fee);
        assertEq(usdc.balanceOf(feeRecipient), fee);
        assertEq(usdc.balanceOf(address(market)), 0);
    }

    function test_buyWithAuthorization_emitsX402Path() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, PRICE, false);
        usdc.mint(buyer, PRICE);

        uint96 fee = uint96((uint256(PRICE) * INITIAL_FEE_BPS) / 10_000);
        IMarketplace.TransferAuthorization memory auth = _auth(buyer, PRICE, keccak256("n1"));
        // payerKind=1, purchasePath=1 on the x402 path
        vm.expectEmit(true, true, true, true, address(market));
        emit IMarketplace.Sold(lid, lineage, buyer, PRICE, PRICE - fee, fee, lid, 1, 1);
        vm.prank(facilitator);
        market.buyWithAuthorization(lid, buyer, auth);
    }

    function test_buyWithAuthorization_revert_badTarget() public {
        uint256 lid = _listed(PRICE, false);
        usdc.mint(buyer, PRICE);
        IMarketplace.TransferAuthorization memory auth = _auth(buyer, PRICE, keccak256("n1"));
        auth.to = makeAddr("notMarket");

        vm.prank(facilitator);
        vm.expectRevert(abi.encodeWithSelector(Marketplace.BadAuthorizationTarget.selector, auth.to));
        market.buyWithAuthorization(lid, buyer, auth);
    }

    function test_buyWithAuthorization_revert_badValue() public {
        uint256 lid = _listed(PRICE, false);
        usdc.mint(buyer, PRICE);
        IMarketplace.TransferAuthorization memory auth = _auth(buyer, PRICE - 1, keccak256("n1"));

        vm.prank(facilitator);
        vm.expectRevert(abi.encodeWithSelector(Marketplace.BadAuthorizationValue.selector, PRICE - 1, PRICE));
        market.buyWithAuthorization(lid, buyer, auth);
    }

    function test_buyWithAuthorization_revert_nonceReplay() public {
        uint256 lid = _listed(PRICE, false);
        usdc.mint(buyer, 2 * uint256(PRICE));

        IMarketplace.TransferAuthorization memory auth = _auth(buyer, PRICE, keccak256("dup"));
        vm.prank(facilitator);
        market.buyWithAuthorization(lid, buyer, auth);

        // Same nonce again — MockUSDC rejects the reused authorization.
        vm.prank(facilitator);
        vm.expectRevert(bytes("MockUSDC: auth nonce used"));
        market.buyWithAuthorization(lid, buyer, auth);
    }

    /// @dev M-2: the license must go to the payer. A facilitator/front-runner
    ///      submitting the buyer's signed auth with a different `recipient`
    ///      would make the buyer pay while someone else receives the soulbound
    ///      license. Enforce `recipient == auth.from`.
    function test_buyWithAuthorization_revert_recipientNotPayer() public {
        uint256 lid = _listed(PRICE, false);
        usdc.mint(buyer, PRICE);
        address attacker = makeAddr("attacker");

        // Auth is signed by `buyer` (auth.from == buyer) but the facilitator
        // tries to route the license to `attacker`.
        IMarketplace.TransferAuthorization memory auth = _auth(buyer, PRICE, keccak256("n1"));
        vm.prank(facilitator);
        vm.expectRevert(Marketplace.RecipientMustBePayer.selector);
        market.buyWithAuthorization(lid, attacker, auth);

        // No funds moved, no license minted.
        assertEq(usdc.balanceOf(buyer), PRICE, "buyer not charged");
        assertEq(license.balanceOf(attacker, lid), 0, "attacker got no license");
    }

    /// @dev M-2: recipient == auth.from is the only accepted recipient, and it
    ///      still settles normally (the happy path is unaffected).
    function test_buyWithAuthorization_recipientEqualsPayer_succeeds() public {
        uint256 lid = _listed(PRICE, false);
        usdc.mint(buyer, PRICE);

        IMarketplace.TransferAuthorization memory auth = _auth(buyer, PRICE, keccak256("n1"));
        vm.prank(facilitator);
        uint256 tokenId = market.buyWithAuthorization(lid, buyer, auth);

        assertEq(tokenId, lid);
        assertEq(license.balanceOf(buyer, lid), 1, "payer receives the license");
    }

    // ---- L-1: free-listing one-per-recipient cap -----------------------

    /// @dev L-1: a free (priceUSDC == 0) listing may mint at most one license
    ///      per recipient. The first direct buy succeeds; a second to the same
    ///      recipient reverts.
    function test_buy_freeListing_secondMintReverts() public {
        uint256 lid = _listed(0, false);

        vm.prank(buyer);
        market.buy(lid, buyer);
        assertEq(license.balanceOf(buyer, lid), 1);

        vm.prank(buyer);
        vm.expectRevert(Marketplace.AlreadyOwnsFreeLicense.selector);
        market.buy(lid, buyer);

        // Still exactly one.
        assertEq(license.balanceOf(buyer, lid), 1, "free mint stays capped at one");
    }

    /// @dev L-1: the cap is per recipient — a different recipient can still
    ///      claim their own first free license.
    function test_buy_freeListing_perRecipientCap() public {
        uint256 lid = _listed(0, false);
        address other = makeAddr("other");

        vm.prank(buyer);
        market.buy(lid, buyer);

        vm.prank(other);
        market.buy(lid, other);

        assertEq(license.balanceOf(buyer, lid), 1);
        assertEq(license.balanceOf(other, lid), 1);
    }

    /// @dev L-1: the cap also applies on the x402 free-listing path.
    function test_buyWithAuthorization_freeListing_secondMintReverts() public {
        uint256 lid = _listed(0, false);

        IMarketplace.TransferAuthorization memory auth = _auth(buyer, 0, keccak256("free1"));
        vm.prank(facilitator);
        market.buyWithAuthorization(lid, buyer, auth);
        assertEq(license.balanceOf(buyer, lid), 1);

        IMarketplace.TransferAuthorization memory auth2 = _auth(buyer, 0, keccak256("free2"));
        vm.prank(facilitator);
        vm.expectRevert(Marketplace.AlreadyOwnsFreeLicense.selector);
        market.buyWithAuthorization(lid, buyer, auth2);
    }

    /// @dev L-1: paid listings are unaffected — a buyer may purchase the same
    ///      paid listing more than once (the cap is free-only).
    function test_buy_paidListing_secondMintAllowed() public {
        uint256 lid = _listed(PRICE, false);
        _fundAndApprove(buyer, 2 * uint256(PRICE));

        vm.prank(buyer);
        market.buy(lid, buyer);
        vm.prank(buyer);
        market.buy(lid, buyer);

        assertEq(license.balanceOf(buyer, lid), 2, "paid re-purchase still allowed");
    }

    // ---- L-2: fee-recipient guard --------------------------------------

    /// @dev L-2: setFeeRecipient must reject the zero address.
    function test_setFeeRecipient_revert_zeroAddress() public {
        vm.expectRevert(Marketplace.ZeroAddress.selector);
        market.setFeeRecipient(address(0));
    }

    // ---- admin ---------------------------------------------------------

    function test_setProtocolFeeBps_capped() public {
        vm.expectRevert(abi.encodeWithSelector(Marketplace.FeeTooHigh.selector, uint16(1001)));
        market.setProtocolFeeBps(1001);

        market.setProtocolFeeBps(1000); // exactly MAX is fine
        assertEq(market.protocolFeeBps(), 1000);
    }

    function test_admin_onlyOwner() public {
        vm.prank(makeAddr("intruder"));
        vm.expectRevert(); // OwnableUnauthorizedAccount
        market.setProtocolFeeBps(100);
    }

    function test_setFeeRecipient() public {
        address nr = makeAddr("newFee");
        market.setFeeRecipient(nr);
        assertEq(market.feeRecipient(), nr);
    }

    // ---- setUsdc (UUPS re-point of the sale currency) -------------------

    function test_setUsdc() public {
        address oldToken = market.usdc();
        address newToken = makeAddr("newUsdc");

        vm.expectEmit(true, true, false, false, address(market));
        emit IMarketplace.UsdcChanged(oldToken, newToken);
        market.setUsdc(newToken);

        assertEq(market.usdc(), newToken);
    }

    function test_setUsdc_revert_zeroAddress() public {
        vm.expectRevert(Marketplace.ZeroAddress.selector);
        market.setUsdc(address(0));
    }

    function test_setUsdc_onlyOwner() public {
        vm.prank(makeAddr("intruder"));
        vm.expectRevert(); // OwnableUnauthorizedAccount
        market.setUsdc(makeAddr("newUsdc"));
    }
}

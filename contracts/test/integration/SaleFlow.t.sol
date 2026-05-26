// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {Marketplace} from "../../src/Marketplace.sol";
import {IMarketplace} from "../../src/interfaces/IMarketplace.sol";

/// @notice End-to-end sale flows against the in-memory deployment (§9.2).
contract SaleFlowTest is BaseTest {
    address creator = makeAddr("creator");
    address humanBuyer = makeAddr("humanBuyer");
    address agentWallet = makeAddr("agentWallet");
    address facilitator = makeAddr("facilitator");
    bytes32 constant SCHEMA = keccak256("xvn.eval.v1");

    /// @dev mint lineage → list → attest → approve → buy → verify license.
    function test_endToEnd_directBuy_withAttestation() public {
        uint96 price = 20_000_000;
        uint256 lineage = _mintLineage(creator);
        uint256 lid = _createListing(creator, lineage, price, false);

        // publish-time attestation by the seller
        vm.prank(creator);
        attest.postAttestation(lid, keccak256("eval"), "ipfs://eval", SCHEMA);
        assertEq(attest.getAttestationCount(lid), 1);

        _fundAndApprove(humanBuyer, price);
        vm.prank(humanBuyer);
        uint256 tokenId = market.buy(lid, humanBuyer);

        // The resource server's access check: a single chain read (§4.4).
        assertGe(license.balanceOf(humanBuyer, tokenId), 1);

        uint96 fee = uint96((uint256(price) * INITIAL_FEE_BPS) / 10_000);
        assertEq(usdc.balanceOf(creator), price - fee);
        assertEq(usdc.balanceOf(feeRecipient), fee);
    }

    /// @dev Full x402 path: agent wallet funds, facilitator submits the auth.
    function test_endToEnd_x402Buy() public {
        uint96 price = 5_000_000;
        uint256 lineage = _mintLineage(creator);
        uint256 lid = _createListing(creator, lineage, price, false);
        usdc.mint(agentWallet, price);

        IMarketplace.TransferAuthorization memory auth = IMarketplace.TransferAuthorization({
            from: agentWallet,
            to: address(market),
            value: price,
            validAfter: 0,
            validBefore: type(uint256).max,
            nonce: keccak256("x402-1"),
            v: 0,
            r: bytes32(0),
            s: bytes32(0)
        });

        vm.prank(facilitator);
        uint256 tokenId = market.buyWithAuthorization(lid, agentWallet, auth);

        assertGe(license.balanceOf(agentWallet, tokenId), 1);
        assertEq(usdc.balanceOf(agentWallet), 0);
    }

    /// @dev §4.5: a listing revoked between the 402 issue and settlement makes
    ///      `buyWithAuthorization` revert cleanly. NOTE: because we check
    ///      `revoked` BEFORE pulling funds (checks-effects), the whole tx
    ///      reverts and the EIP-3009 nonce is NOT consumed — slightly safer
    ///      than the spec §4.5 prose ("nonce is consumed but USDC isn't moved").
    function test_revokedBetween402AndSettlement_revertsCleanly() public {
        uint96 price = 5_000_000;
        uint256 lineage = _mintLineage(creator);
        uint256 lid = _createListing(creator, lineage, price, false);
        usdc.mint(agentWallet, price);

        IMarketplace.TransferAuthorization memory auth = IMarketplace.TransferAuthorization({
            from: agentWallet,
            to: address(market),
            value: price,
            validAfter: 0,
            validBefore: type(uint256).max,
            nonce: keccak256("x402-2"),
            v: 0,
            r: bytes32(0),
            s: bytes32(0)
        });

        // 402 issued; seller revokes before the facilitator settles.
        vm.prank(creator);
        listings.revokeListing(lid);

        vm.prank(facilitator);
        vm.expectRevert(abi.encodeWithSelector(Marketplace.ListingRevoked.selector, lid));
        market.buyWithAuthorization(lid, agentWallet, auth);

        assertFalse(usdc.authorizationState(agentWallet, auth.nonce), "nonce untouched on clean revert");
        assertEq(usdc.balanceOf(agentWallet), price, "no funds moved");
    }

    /// @dev A creator can monetise multiple variants under one lineage NFT.
    function test_multipleVariantListings_underOneLineage() public {
        uint256 lineage = _mintLineage(creator);
        vm.startPrank(creator);
        uint256 a = listings.createListing(lineage, keccak256("v1"), "ipfs://v1", 0, 1_000_000, false);
        uint256 b = listings.createListing(lineage, keccak256("v2"), "ipfs://v2", 0, 2_000_000, false);
        vm.stopPrank();

        assertEq(listings.getListing(a).agentNftId, lineage);
        assertEq(listings.getListing(b).agentNftId, lineage);
        assertTrue(listings.getListing(a).contentHash != listings.getListing(b).contentHash);
    }
}

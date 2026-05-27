// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {LicenseToken} from "../../src/LicenseToken.sol";

contract LicenseTokenTest is BaseTest {
    address seller = makeAddr("seller");
    address buyer = makeAddr("buyer");
    address other = makeAddr("other");

    function _buy(uint96 price, bool transferable) internal returns (uint256 lid) {
        uint256 lineage = _mintLineage(seller);
        lid = _createListing(seller, lineage, price, transferable);
        if (price > 0) _fundAndApprove(buyer, price);
        vm.prank(buyer);
        market.buy(lid, buyer);
    }

    function test_authorizedMint_revert_unauthorizedCaller() public {
        address rando = makeAddr("rando");
        vm.prank(rando);
        vm.expectRevert(abi.encodeWithSelector(LicenseToken.NotAuthorizedMinter.selector, rando));
        license.authorizedMint(buyer, 1, 1);
    }

    function test_marketplace_isAuthorizedMinter() public view {
        assertTrue(license.isAuthorized(address(market)));
    }

    function test_setAuthorized_onlyOwner() public {
        vm.prank(makeAddr("intruder"));
        vm.expectRevert(); // OwnableUnauthorizedAccount
        license.setAuthorized(makeAddr("x"), true);
    }

    function test_soulbound_default_blocksTransfer() public {
        uint256 lid = _buy(15_000_000, false);
        assertEq(license.balanceOf(buyer, lid), 1);
        assertFalse(license.transferableForId(lid));

        vm.prank(buyer);
        vm.expectRevert(abi.encodeWithSelector(LicenseToken.Soulbound.selector, lid));
        license.safeTransferFrom(buyer, other, lid, 1, "");
    }

    function test_transferableListing_allowsTransfer() public {
        uint256 lid = _buy(15_000_000, true);
        assertTrue(license.transferableForId(lid));

        vm.prank(buyer);
        license.safeTransferFrom(buyer, other, lid, 1, "");
        assertEq(license.balanceOf(other, lid), 1);
        assertEq(license.balanceOf(buyer, lid), 0);
    }

    function test_transferableForId_falseForUnknownListing() public view {
        // No such listing => registry returns false => soulbound-safe default.
        assertFalse(license.transferableForId(123_456));
    }

    function test_setListingRegistry_oneShot() public {
        vm.expectRevert(LicenseToken.ListingRegistryAlreadySet.selector);
        license.setListingRegistry(makeAddr("otherRegistry"));
    }
}

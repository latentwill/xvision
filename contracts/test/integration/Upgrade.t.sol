// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {MarketplaceV2} from "../mocks/MarketplaceV2.sol";

/// @notice UUPS upgrade-safety: state survives an implementation swap, and the
///         upgrade is owner-gated. (Local equivalent of the §9.3 fork test; the
///         mainnet-state replay version lives in test/fork/.)
contract UpgradeTest is BaseTest {
    address seller = makeAddr("seller");
    address buyer = makeAddr("buyer");

    function test_upgrade_preservesStateAndAddsBehavior() public {
        // Seed live state: a listing and a sold license.
        uint96 price = 15_000_000;
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, price, false);
        _fundAndApprove(buyer, price);
        vm.prank(buyer);
        market.buy(lid, buyer);
        assertEq(license.balanceOf(buyer, lid), 1);

        // Upgrade the Marketplace implementation.
        MarketplaceV2 v2 = new MarketplaceV2();
        market.upgradeToAndCall(address(v2), "");

        // Pre-upgrade state preserved through the proxy.
        assertEq(market.protocolFeeBps(), INITIAL_FEE_BPS);
        assertEq(market.feeRecipient(), feeRecipient);
        assertEq(market.listingRegistry(), address(listings));
        assertEq(license.balanceOf(buyer, lid), 1);

        // New behavior available at the same address.
        assertEq(MarketplaceV2(address(market)).version(), "v2");
    }

    function test_upgrade_onlyOwner() public {
        MarketplaceV2 v2 = new MarketplaceV2();
        vm.prank(makeAddr("intruder"));
        vm.expectRevert(); // OwnableUnauthorizedAccount
        market.upgradeToAndCall(address(v2), "");
    }
}

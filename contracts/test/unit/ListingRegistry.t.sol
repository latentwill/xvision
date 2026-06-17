// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {ListingRegistry} from "../../src/ListingRegistry.sol";
import {IListingRegistry} from "../../src/interfaces/IListingRegistry.sol";

contract ListingRegistryTest is BaseTest {
    address seller = makeAddr("seller");
    address other = makeAddr("other");

    function test_createListing_happy() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 15_000_000, false);

        IListingRegistry.Listing memory l = listings.getListing(lid);
        assertEq(l.listingId, lid);
        assertEq(l.seller, seller);
        assertEq(l.agentNftId, lineage);
        assertEq(l.priceUSDC, 15_000_000);
        assertEq(l.protocolFeeBps, INITIAL_FEE_BPS, "fee snapshot");
        assertEq(l.tier, 0);
        assertFalse(l.transferableLicense);
        assertFalse(l.revoked);
        assertEq(listings.totalListings(), 1);
        assertTrue(listings.listingExists(lid));
    }

    function test_createListing_idsStartAtOne_andIncrement() public {
        uint256 lineage = _mintLineage(seller);
        uint256 a = _createListing(seller, lineage, 1, false);
        uint256 b = _createListing(seller, lineage, 1, false);
        assertEq(a, 1);
        assertEq(b, 2);
    }

    function test_createListing_revert_notLineageOwner() public {
        uint256 lineage = _mintLineage(seller);
        vm.prank(other);
        vm.expectRevert(abi.encodeWithSelector(ListingRegistry.NotLineageOwner.selector, lineage, other));
        listings.createListing(lineage, keccak256("v"), "ipfs://v", 0, 1, false);
    }

    function test_createListing_revert_invalidTier() public {
        uint256 lineage = _mintLineage(seller);
        vm.prank(seller);
        vm.expectRevert(abi.encodeWithSelector(ListingRegistry.InvalidTier.selector, uint8(2)));
        listings.createListing(lineage, keccak256("v"), "ipfs://v", 2, 1, false);
    }

    /// @dev A later fee bump applies only to NEW listings, never retroactively.
    function test_feeSnapshot_isImmutablePerListing() public {
        uint256 lineage = _mintLineage(seller);
        uint256 first = _createListing(seller, lineage, 1, false);
        assertEq(listings.getListing(first).protocolFeeBps, 500);

        market.setProtocolFeeBps(800);
        uint256 second = _createListing(seller, lineage, 1, false);

        assertEq(listings.getListing(first).protocolFeeBps, 500, "old listing unchanged");
        assertEq(listings.getListing(second).protocolFeeBps, 800, "new listing picks up bump");
    }

    function test_updateListing_rotatesContentOnly() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 15_000_000, true);

        vm.prank(seller);
        listings.updateListing(lid, keccak256("v2"), "ipfs://v2");

        IListingRegistry.Listing memory l = listings.getListing(lid);
        assertEq(l.contentHash, keccak256("v2"));
        assertEq(l.contentURI, "ipfs://v2");
        // price, tier, fee, transferable untouched
        assertEq(l.priceUSDC, 15_000_000);
        assertTrue(l.transferableLicense);
        assertEq(l.protocolFeeBps, 500);
    }

    function test_updateListing_revert_notSeller() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 1, false);
        vm.prank(other);
        vm.expectRevert(abi.encodeWithSelector(ListingRegistry.NotSeller.selector, lid, other));
        listings.updateListing(lid, keccak256("v2"), "ipfs://v2");
    }

    function test_revokeListing_blocksAndIsSellerOnly() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 1, false);

        vm.prank(other);
        vm.expectRevert(abi.encodeWithSelector(ListingRegistry.NotSeller.selector, lid, other));
        listings.revokeListing(lid);

        vm.prank(seller);
        listings.revokeListing(lid);
        assertTrue(listings.getListing(lid).revoked);

        vm.prank(seller);
        vm.expectRevert(abi.encodeWithSelector(ListingRegistry.AlreadyRevoked.selector, lid));
        listings.revokeListing(lid);
    }

    function test_getListing_revert_unknown() public {
        vm.expectRevert(abi.encodeWithSelector(ListingRegistry.UnknownListing.selector, uint256(999)));
        listings.getListing(999);
    }

    function test_setMarketplace_isOneShot() public {
        vm.expectRevert(ListingRegistry.MarketplaceAlreadySet.selector);
        listings.setMarketplace(makeAddr("anotherMarket"));
    }

    // ---- Finding 3: free + transferable is forbidden --------------------

    /// A free (price == 0) AND transferable listing is nonsensical (the L-1
    /// per-recipient cap reads live balance, so the recipient could mint,
    /// transfer the token away, and mint again unboundedly). Forbid the combo
    /// at creation with a typed error.
    function test_createListing_revert_freeAndTransferable() public {
        uint256 lineage = _mintLineage(seller);
        vm.prank(seller);
        vm.expectRevert(ListingRegistry.FreeTransferableForbidden.selector);
        listings.createListing(lineage, keccak256("v"), "ipfs://v", 0, 0, true);
    }

    /// Free + soulbound (transferable == false) still works — the one-per-
    /// recipient cap holds because the token cannot move.
    function test_createListing_freeAndSoulbound_ok() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 0, false);
        IListingRegistry.Listing memory l = listings.getListing(lid);
        assertEq(l.priceUSDC, 0);
        assertFalse(l.transferableLicense);
    }

    /// Paid + transferable still works — re-purchase is allowed, no free-mint
    /// bypass exists when a payment throttles each unit.
    function test_createListing_paidAndTransferable_ok() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 1, true);
        IListingRegistry.Listing memory l = listings.getListing(lid);
        assertEq(l.priceUSDC, 1);
        assertTrue(l.transferableLicense);
    }

    function test_createListing_emitsEvent() public {
        uint256 lineage = _mintLineage(seller);
        vm.expectEmit(true, true, true, true, address(listings));
        emit IListingRegistry.ListingCreated(1, seller, lineage, keccak256("variant-bundle"), 0, 1);
        _createListing(seller, lineage, 1, false);
    }

    // ---- updatePrice: in-place repricing (seller-only) ------------------

    /// The seller can change a listing's price in place; everything else
    /// (tier, fee snapshot, content, transferable) is untouched.
    function test_updatePrice_changesPriceInPlace() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 15_000_000, false);

        vm.prank(seller);
        listings.updatePrice(lid, 9_000_000);

        IListingRegistry.Listing memory l = listings.getListing(lid);
        assertEq(l.priceUSDC, 9_000_000, "price updated");
        assertEq(l.tier, 0, "tier untouched");
        assertEq(l.protocolFeeBps, 500, "fee snapshot untouched");
        assertFalse(l.revoked);
    }

    function test_updatePrice_emitsEvent() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 15_000_000, false);
        vm.expectEmit(true, false, false, true, address(listings));
        emit IListingRegistry.ListingPriceUpdated(lid, 15_000_000, 9_000_000);
        vm.prank(seller);
        listings.updatePrice(lid, 9_000_000);
    }

    /// Repricing a soulbound listing to 0 makes it free (open/clone path).
    function test_updatePrice_toZero_freeSoulbound_ok() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 15_000_000, false);
        vm.prank(seller);
        listings.updatePrice(lid, 0);
        assertEq(listings.getListing(lid).priceUSDC, 0);
    }

    function test_updatePrice_revert_notSeller() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 1, false);
        vm.prank(other);
        vm.expectRevert(abi.encodeWithSelector(ListingRegistry.NotSeller.selector, lid, other));
        listings.updatePrice(lid, 2);
    }

    function test_updatePrice_revert_unknown() public {
        vm.prank(seller);
        vm.expectRevert(abi.encodeWithSelector(ListingRegistry.UnknownListing.selector, uint256(999)));
        listings.updatePrice(999, 1);
    }

    function test_updatePrice_revert_revoked() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 1, false);
        vm.prank(seller);
        listings.revokeListing(lid);
        vm.prank(seller);
        vm.expectRevert(abi.encodeWithSelector(ListingRegistry.AlreadyRevoked.selector, lid));
        listings.updatePrice(lid, 2);
    }

    /// A paid + transferable listing cannot be repriced to free (0): that would
    /// recreate the forbidden free+transferable combo (Finding 3), so the
    /// create-time invariant is preserved on reprice.
    function test_updatePrice_revert_freeAndTransferable() public {
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, 5_000_000, true);
        vm.prank(seller);
        vm.expectRevert(ListingRegistry.FreeTransferableForbidden.selector);
        listings.updatePrice(lid, 0);
    }
}

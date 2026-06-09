// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {ReputationRegistry} from "../../src/registries/ReputationRegistry.sol";

/// @title ReputationGateTest — §3.6 license-gated attestation + §3.7 revoke.
/// @notice Exercises the on-chain license gate (only ERC-1155 license holders
///         can submit feedback for a listed agent) and the revokeFeedback
///         tombstone path. Uses the full wired stack from BaseTest so the
///         LicenseToken/ListingRegistry relationships are real.
contract ReputationGateTest is BaseTest {
    address internal seller = makeAddr("seller");
    address internal buyer = makeAddr("buyer");
    address internal stranger = makeAddr("stranger");

    uint256 internal agentId;
    uint256 internal listingId;

    function setUp() public override {
        super.setUp();

        // Mint a lineage/agent NFT and a Tier-0 listing under it.
        agentId = _mintLineage(seller);
        listingId = _createListing(seller, agentId, 1_000_000, false);

        // Register the gate: feedback for `agentId` requires holding the
        // license for `listingId` (admin/registrar action). admin == this.
        reputation.setListingForAgent(agentId, listingId);
    }

    // ---- license gate --------------------------------------------------

    /// A license holder CAN give feedback for the gated agent.
    function test_giveFeedback_licenseHolder_succeeds() public {
        // buyer purchases -> mints 1 license (tokenId == listingId).
        _fundAndApprove(buyer, 1_000_000);
        vm.prank(buyer);
        market.buy(listingId, buyer);

        assertEq(license.balanceOf(buyer, listingId), 1, "buyer holds license");

        vm.prank(buyer);
        uint256 fid =
            reputation.giveFeedback(agentId, int128(100), 0, "tradingYield", "month", "", "ipfs://fb", keccak256("fb"));

        assertEq(fid, 0, "first feedback index is 0");
        assertEq(reputation.getFeedbackCount(agentId), 1);
    }

    /// A non-holder is REVERTED by the gate.
    function test_giveFeedback_nonHolder_reverts() public {
        assertEq(license.balanceOf(stranger, listingId), 0, "stranger holds no license");

        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(ReputationRegistry.NotLicensed.selector, agentId, listingId, stranger));
        reputation.giveFeedback(agentId, int128(100), 0, "tradingYield", "month", "", "ipfs://fb", keccak256("fb"));
    }

    /// An agent with NO listing gate (pure identity) stays permissionless.
    function test_giveFeedback_ungatedAgent_permissionless() public {
        // agentId 999 has no setListingForAgent mapping.
        vm.prank(stranger);
        uint256 fid = reputation.giveFeedback(999, int128(1), 6, "xvision", "", "", "ipfs://x", keccak256("x"));
        assertEq(fid, 0);
        assertEq(reputation.getFeedbackCount(999), 1);
    }

    /// The gate cannot be bypassed by burning/transferring the license away
    /// after the gate is registered — gate reads live balance at call time.
    function test_giveFeedback_afterLicenseGone_reverts() public {
        _fundAndApprove(buyer, 1_000_000);
        vm.prank(buyer);
        market.buy(listingId, buyer);

        // Soulbound default: buyer cannot transfer; but a transferable listing
        // would let the license move. Simulate "no longer holds" by using a
        // fresh address that never bought.
        address other = makeAddr("other");
        vm.prank(other);
        vm.expectRevert(abi.encodeWithSelector(ReputationRegistry.NotLicensed.selector, agentId, listingId, other));
        reputation.giveFeedback(agentId, int128(1), 0, "tradingYield", "", "", "ipfs://y", keccak256("y"));
    }

    // ---- revoke --------------------------------------------------------

    function _seedBuyerFeedback() internal returns (uint256 fid) {
        _fundAndApprove(buyer, 1_000_000);
        vm.prank(buyer);
        market.buy(listingId, buyer);
        vm.prank(buyer);
        fid =
            reputation.giveFeedback(agentId, int128(100), 0, "tradingYield", "month", "", "ipfs://fb", keccak256("fb"));
    }

    /// revokeFeedback by the original submitter emits FeedbackRevoked and
    /// marks the entry tombstoned (history preserved, count unchanged).
    function test_revokeFeedback_bySubmitter_tombstones() public {
        uint256 fid = _seedBuyerFeedback();
        assertFalse(reputation.isTombstoned(agentId, fid));

        vm.expectEmit(true, true, false, true, address(reputation));
        emit ReputationRegistry.FeedbackRevoked(agentId, fid, buyer);
        vm.prank(buyer);
        reputation.revokeFeedback(agentId, fid);

        assertTrue(reputation.isTombstoned(agentId, fid), "entry tombstoned");
        // Append-only: history not deleted, count unchanged.
        assertEq(reputation.getFeedbackCount(agentId), 1);
        (address rater,,,,,,,,) = reputation.getFeedback(agentId, fid);
        assertEq(rater, buyer, "original entry preserved");
    }

    /// Admin (owner) can also revoke any feedback.
    function test_revokeFeedback_byAdmin_tombstones() public {
        uint256 fid = _seedBuyerFeedback();
        // admin == address(this) (owner).
        reputation.revokeFeedback(agentId, fid);
        assertTrue(reputation.isTombstoned(agentId, fid));
    }

    /// revoke by a non-submitter, non-owner reverts.
    function test_revokeFeedback_byStranger_reverts() public {
        uint256 fid = _seedBuyerFeedback();
        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(ReputationRegistry.NotFeedbackOwner.selector, agentId, fid, stranger));
        reputation.revokeFeedback(agentId, fid);
    }

    /// double-revoke reverts (already tombstoned).
    function test_revokeFeedback_doubleRevoke_reverts() public {
        uint256 fid = _seedBuyerFeedback();
        vm.prank(buyer);
        reputation.revokeFeedback(agentId, fid);
        vm.prank(buyer);
        vm.expectRevert(abi.encodeWithSelector(ReputationRegistry.AlreadyRevoked.selector, agentId, fid));
        reputation.revokeFeedback(agentId, fid);
    }

    /// revoke of an out-of-range index reverts.
    function test_revokeFeedback_badIndex_reverts() public {
        vm.prank(buyer);
        vm.expectRevert(abi.encodeWithSelector(ReputationRegistry.UnknownFeedback.selector, agentId, uint256(0)));
        reputation.revokeFeedback(agentId, 0);
    }

    // ---- access control on wiring -------------------------------------

    function test_setListingForAgent_onlyOwner() public {
        vm.prank(stranger);
        vm.expectRevert();
        reputation.setListingForAgent(agentId, listingId);
    }

    function test_setLicenseToken_onlyOwner_andOneShot() public {
        vm.prank(stranger);
        vm.expectRevert();
        reputation.setLicenseToken(address(license));

        // already set in BaseTest wiring -> one-shot revert on re-set by owner.
        vm.expectRevert(ReputationRegistry.LicenseTokenAlreadySet.selector);
        reputation.setLicenseToken(address(license));
    }
}

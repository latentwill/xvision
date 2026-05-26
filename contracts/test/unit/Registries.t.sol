// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Test} from "forge-std/Test.sol";
import {IdentityRegistry} from "../../src/registries/IdentityRegistry.sol";
import {ReputationRegistry} from "../../src/registries/ReputationRegistry.sol";
import {ValidationRegistry} from "../../src/registries/ValidationRegistry.sol";

contract RegistriesTest is Test {
    IdentityRegistry identity;
    ReputationRegistry reputation;
    ValidationRegistry validation;

    address alice = makeAddr("alice");
    address bob = makeAddr("bob");

    function setUp() public {
        identity = new IdentityRegistry();
        reputation = new ReputationRegistry();
        validation = new ValidationRegistry();
    }

    // ---- IdentityRegistry ----------------------------------------------

    function test_register_firstTokenIsZero() public {
        vm.prank(alice);
        uint256 id = identity.register("ipfs://lineage-a");
        assertEq(id, 0, "agent #0 is the first mint");
        assertEq(identity.ownerOf(0), alice);
        assertEq(identity.tokenURI(0), "ipfs://lineage-a");
        assertEq(identity.totalMinted(), 1);
    }

    function test_register_incrementsMonotonically() public {
        vm.prank(alice);
        assertEq(identity.register("ipfs://a"), 0);
        vm.prank(bob);
        assertEq(identity.register("ipfs://b"), 1);
        assertEq(identity.ownerOf(1), bob);
        assertEq(identity.totalMinted(), 2);
    }

    function test_register_emitsAgentRegistered() public {
        vm.expectEmit(true, true, false, true, address(identity));
        emit IdentityRegistry.AgentRegistered(0, alice, "ipfs://a");
        vm.prank(alice);
        identity.register("ipfs://a");
    }

    // ---- ReputationRegistry --------------------------------------------

    function test_giveFeedback_storesAndReads() public {
        vm.prank(bob);
        reputation.giveFeedback(
            0, int128(12_340_000), 6, "xvision", "cycle-1", "", "ipfs://fb", keccak256("fb")
        );

        assertEq(reputation.getFeedbackCount(0), 1);
        (
            address rater,
            int128 value,
            uint8 valueDecimals,
            string memory tag1,
            string memory tag2,
            string memory endpoint,
            string memory feedbackURI,
            bytes32 feedbackHash,
            uint256 timestamp
        ) = reputation.getFeedback(0, 0);

        assertEq(rater, bob);
        assertEq(value, int128(12_340_000));
        assertEq(valueDecimals, 6);
        assertEq(tag1, "xvision");
        assertEq(tag2, "cycle-1");
        assertEq(endpoint, "");
        assertEq(feedbackURI, "ipfs://fb");
        assertEq(feedbackHash, keccak256("fb"));
        assertGt(timestamp, 0);
    }

    function test_giveFeedback_negativeValue() public {
        vm.prank(bob);
        reputation.giveFeedback(3, int128(-5_000_000), 6, "xvision", "", "", "ipfs://l", bytes32(0));
        (, int128 value,,,,,,,) = reputation.getFeedback(3, 0);
        assertEq(value, int128(-5_000_000));
    }

    function test_giveFeedback_emits() public {
        vm.expectEmit(true, true, false, true, address(reputation));
        emit ReputationRegistry.FeedbackPosted(9, bob, int128(1), keccak256("h"), "xvision");
        vm.prank(bob);
        reputation.giveFeedback(9, int128(1), 6, "xvision", "", "", "ipfs://x", keccak256("h"));
    }

    // ---- ValidationRegistry --------------------------------------------

    function test_postValidation_storesAndReads() public {
        vm.prank(alice);
        validation.postValidation(0, keccak256("proof"), "ipfs://proof", "trade-proof");

        assertEq(validation.getValidationCount(0), 1);
        (address validator, bytes32 resultHash, string memory resultURI, string memory tag, uint256 ts)
        = validation.getValidation(0, 0);
        assertEq(validator, alice);
        assertEq(resultHash, keccak256("proof"));
        assertEq(resultURI, "ipfs://proof");
        assertEq(tag, "trade-proof");
        assertGt(ts, 0);
    }

    function test_postValidation_emits() public {
        vm.expectEmit(true, true, false, true, address(validation));
        emit ValidationRegistry.ValidationPosted(2, alice, keccak256("r"), "regime-verifier");
        vm.prank(alice);
        validation.postValidation(2, keccak256("r"), "ipfs://r", "regime-verifier");
    }
}

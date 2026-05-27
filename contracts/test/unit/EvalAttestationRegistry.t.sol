// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {IEvalAttestationRegistry} from "../../src/interfaces/IEvalAttestationRegistry.sol";

contract EvalAttestationRegistryTest is BaseTest {
    bytes32 constant SCHEMA = keccak256("xvn.eval.v1");

    function test_postAttestation_storesAndCounts() public {
        address seller = makeAddr("seller");
        vm.prank(seller);
        attest.postAttestation(1, keccak256("eval-a"), "ipfs://a", SCHEMA);

        assertEq(attest.getAttestationCount(1), 1);
        IEvalAttestationRegistry.Attestation[] memory all = attest.getAttestations(1);
        assertEq(all.length, 1);
        assertEq(all[0].evalResultHash, keccak256("eval-a"));
        assertEq(all[0].evalResultURI, "ipfs://a");
        assertEq(all[0].attester, seller);
        assertEq(all[0].schema, SCHEMA);
        assertGt(all[0].postedAt, 0);
    }

    function test_postAttestation_publishTimeAndThirdParty() public {
        address seller = makeAddr("seller");
        address validator = makeAddr("validator");

        vm.prank(seller);
        attest.postAttestation(7, keccak256("seller-eval"), "ipfs://s", SCHEMA);
        vm.prank(validator);
        attest.postAttestation(7, keccak256("validator-eval"), "ipfs://v", SCHEMA);

        IEvalAttestationRegistry.Attestation[] memory all = attest.getAttestations(7);
        assertEq(all.length, 2);
        assertEq(all[0].attester, seller);
        assertEq(all[1].attester, validator);
    }

    function test_postAttestation_emits() public {
        address attester = makeAddr("attester");
        vm.expectEmit(true, true, false, true, address(attest));
        emit IEvalAttestationRegistry.AttestationPosted(42, attester, keccak256("e"), SCHEMA);
        vm.prank(attester);
        attest.postAttestation(42, keccak256("e"), "ipfs://e", SCHEMA);
    }

    function test_count_zeroForUnknownListing() public view {
        assertEq(attest.getAttestationCount(99), 0);
    }
}

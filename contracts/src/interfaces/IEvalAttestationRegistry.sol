// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title IEvalAttestationRegistry — eval-attestation surface (surface spec §3.4).
interface IEvalAttestationRegistry {
    struct Attestation {
        bytes32 evalResultHash; // keccak256 of full eval JSON
        string evalResultURI; // ipfs://…
        address attester;
        uint64 postedAt;
        bytes32 schema; // EAS-style schema id, future-compatible
    }

    function postAttestation(uint256 listingId, bytes32 evalResultHash, string calldata evalResultURI, bytes32 schema)
        external;

    function getAttestations(uint256 listingId) external view returns (Attestation[] memory);

    function getAttestationCount(uint256 listingId) external view returns (uint256);

    event AttestationPosted(
        uint256 indexed listingId, address indexed attester, bytes32 evalResultHash, bytes32 schema
    );
}

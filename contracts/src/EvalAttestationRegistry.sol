// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";

import {IEvalAttestationRegistry} from "./interfaces/IEvalAttestationRegistry.sol";

/// @title EvalAttestationRegistry — eval attestations per listing (spec §3.4).
/// @notice Two write paths share one function: publish-time attestation by the
///         seller (their signed eval result for the canonical scenario), and
///         third-party attestation by independent validators who re-run the
///         eval and post the result. A cheap on-chain anti-fraud surface.
///
/// @dev UUPS proxy + operator-EOA admin for V2 testnet. The admin can upgrade
///      the implementation but CANNOT delete or mutate existing attestations
///      (surface spec §7.3). Schema id is EAS-style for future EAS
///      compatibility (open question §11: migrate to EAS on Mantle if deployed).
contract EvalAttestationRegistry is
    Initializable,
    OwnableUpgradeable,
    UUPSUpgradeable,
    IEvalAttestationRegistry
{
    /// @dev listingId => append-only attestation list.
    mapping(uint256 => Attestation[]) private _attestations;

    /// @dev Storage gap (surface spec §7.5). One slot used above.
    uint256[49] private __gap;

    error ZeroAddress();

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(address admin) external initializer {
        if (admin == address(0)) revert ZeroAddress();
        __Ownable_init(admin);
        __UUPSUpgradeable_init();
    }

    /// @inheritdoc IEvalAttestationRegistry
    /// @dev Permissionless. The `attester` is recorded as `msg.sender`; the
    ///      indexer/UI decides how to weight seller vs third-party attesters.
    function postAttestation(
        uint256 listingId,
        bytes32 evalResultHash,
        string calldata evalResultURI,
        bytes32 schema
    ) external override {
        _attestations[listingId].push(
            Attestation({
                evalResultHash: evalResultHash,
                evalResultURI: evalResultURI,
                attester: msg.sender,
                postedAt: uint64(block.timestamp),
                schema: schema
            })
        );
        emit AttestationPosted(listingId, msg.sender, evalResultHash, schema);
    }

    /// @inheritdoc IEvalAttestationRegistry
    function getAttestations(uint256 listingId)
        external
        view
        override
        returns (Attestation[] memory)
    {
        return _attestations[listingId];
    }

    /// @inheritdoc IEvalAttestationRegistry
    function getAttestationCount(uint256 listingId) external view override returns (uint256) {
        return _attestations[listingId].length;
    }

    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}
}

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title ValidationRegistry — ERC-8004 §3.3 minimal validation store.
/// @notice Append-only per-agent validation log. Two producers write here:
///         (1) `xvision-execution` posts a proof after each closed paper/live
///         trade (surface spec §8.2, "ValidationRegistry writes from
///         xvision-execution after closed paper trades"); (2) in-house attester
///         agents post `ValidationReceipt`s after independently re-checking a
///         lineage's claims (marketplace-plugin-design §1.1, §4).
///
/// @dev NEW in the surface spec (§8.3 step 3) — referenced by ADR 0008 and the
///      Strategy Engine spec §13 but never written into a contract tree before
///      this. Immutable by design (no proxy, no admin), mirroring the other two
///      ERC-8004 registries. Shape intentionally parallels ReputationRegistry
///      so the Rust client and subgraph treat all three registries uniformly.
///      This is a minimal stub pending ERC-8004 finalisation of the validation
///      request/response shape; xvision only needs the append-and-read surface.
contract ValidationRegistry {
    struct Validation {
        address validator; // who posted (execution wallet or attester agent)
        bytes32 resultHash; // keccak256 of the canonical validation JSON
        string resultURI; // ipfs://… (or inline JSON) of the full result
        string tag; // free-text class, e.g. "trade-proof" | "regime-verifier"
        uint256 timestamp;
    }

    /// @dev agentId => append-only validation list.
    mapping(uint256 => Validation[]) private _validations;

    /// @notice Indexer-friendly event (surface spec §6.1:
    ///         `ValidationPosted(uint256 agentId, bytes32 resultHash, ...)`).
    event ValidationPosted(uint256 indexed agentId, address indexed validator, bytes32 resultHash, string tag);

    /// @notice Post one validation entry for `agentId`.
    /// @dev Permissionless, like ReputationRegistry — anyone may validate any
    ///      agent. Trust weighting (in-house vs external attester) is an
    ///      off-chain indexer concern.
    function postValidation(uint256 agentId, bytes32 resultHash, string calldata resultURI, string calldata tag)
        external
    {
        _validations[agentId].push(
            Validation({
                validator: msg.sender,
                resultHash: resultHash,
                resultURI: resultURI,
                tag: tag,
                timestamp: block.timestamp
            })
        );
        emit ValidationPosted(agentId, msg.sender, resultHash, tag);
    }

    /// @notice Read one validation entry by index.
    function getValidation(uint256 agentId, uint256 index)
        external
        view
        returns (address validator, bytes32 resultHash, string memory resultURI, string memory tag, uint256 timestamp)
    {
        Validation storage v = _validations[agentId][index];
        return (v.validator, v.resultHash, v.resultURI, v.tag, v.timestamp);
    }

    /// @notice Number of validation entries posted for `agentId`.
    function getValidationCount(uint256 agentId) external view returns (uint256) {
        return _validations[agentId].length;
    }
}

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title ReputationRegistry — ERC-8004 §3.2 minimal feedback store.
/// @notice Append-only per-agent feedback log. xvision posts one entry per
///         closed decision cycle (the Rust client encodes a `TradeOutcome` as
///         JSON inline in `feedbackURI`, with `feedbackHash` == keccak256 of
///         that JSON; `value` is realised PnL * 1e6, `valueDecimals` == 6).
///         The marketplace plugin also uses this surface to anchor lineage
///         Merkle roots and session-commitment hashes (those are ordinary
///         `giveFeedback` calls with dedicated `tag1` values).
///
/// @dev Immutable by design — no proxy, no admin, no mutation or deletion of
///      existing entries (surface spec §7.3). Matches the signatures already
///      bound in `crates/xvision-identity/src/client.rs`.
contract ReputationRegistry {
    struct Feedback {
        address rater;
        int128 value;
        uint8 valueDecimals;
        string tag1;
        string tag2;
        string endpoint;
        string feedbackURI;
        bytes32 feedbackHash;
        uint256 timestamp;
    }

    /// @dev agentId => append-only feedback list.
    mapping(uint256 => Feedback[]) private _feedback;

    /// @notice Indexer-friendly event (surface spec §6.1). `agentId` and
    ///         `rater` are indexed for subgraph filtering.
    event FeedbackPosted(
        uint256 indexed agentId, address indexed rater, int128 value, bytes32 feedbackHash, string tag1
    );

    /// @notice Post one feedback entry for `agentId`.
    /// @dev No access control: anyone can rate any agent. Sybil/quality
    ///      filtering is an off-chain indexer concern, not an on-chain one —
    ///      consistent with the ERC-8004 draft's permissionless model.
    function giveFeedback(
        uint256 agentId,
        int128 value,
        uint8 valueDecimals,
        string calldata tag1,
        string calldata tag2,
        string calldata endpoint,
        string calldata feedbackURI,
        bytes32 feedbackHash
    ) external {
        _feedback[agentId].push(
            Feedback({
                rater: msg.sender,
                value: value,
                valueDecimals: valueDecimals,
                tag1: tag1,
                tag2: tag2,
                endpoint: endpoint,
                feedbackURI: feedbackURI,
                feedbackHash: feedbackHash,
                timestamp: block.timestamp
            })
        );
        emit FeedbackPosted(agentId, msg.sender, value, feedbackHash, tag1);
    }

    /// @notice Read one feedback entry by index.
    function getFeedback(uint256 agentId, uint256 index)
        external
        view
        returns (
            address rater,
            int128 value,
            uint8 valueDecimals,
            string memory tag1,
            string memory tag2,
            string memory endpoint,
            string memory feedbackURI,
            bytes32 feedbackHash,
            uint256 timestamp
        )
    {
        Feedback storage fb = _feedback[agentId][index];
        return (
            fb.rater,
            fb.value,
            fb.valueDecimals,
            fb.tag1,
            fb.tag2,
            fb.endpoint,
            fb.feedbackURI,
            fb.feedbackHash,
            fb.timestamp
        );
    }

    /// @notice Number of feedback entries posted for `agentId`.
    function getFeedbackCount(uint256 agentId) external view returns (uint256) {
        return _feedback[agentId].length;
    }
}

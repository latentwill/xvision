// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {IERC1155} from "@openzeppelin/contracts/token/ERC1155/IERC1155.sol";

/// @title ReputationRegistry — ERC-8004 §3.2 minimal feedback store.
/// @notice Append-only per-agent feedback log. xvision posts one entry per
///         closed decision cycle (the Rust client encodes a `TradeOutcome` as
///         JSON inline in `feedbackURI`, with `feedbackHash` == keccak256 of
///         that JSON; `value` is realised PnL * 1e6, `valueDecimals` == 6).
///         The marketplace plugin also uses this surface to anchor lineage
///         Merkle roots and session-commitment hashes (those are ordinary
///         `giveFeedback` calls with dedicated `tag1` values).
///
/// @dev ERC-8004 `giveFeedback` / `getFeedback` / `getFeedbackCount`
///      signatures and the `FeedbackPosted` event are UNCHANGED — they remain
///      bit-compatible with the bindings in `crates/xvision-identity/src/client.rs`.
///      What this revision adds (additively, no breaking signature change):
///
///      §3.6 license gate — when a marketplace listing is registered for an
///      `agentId` via {setListingForAgent}, `giveFeedback` for that agent
///      requires the submitter to hold ≥1 ERC-1155 license for that listing
///      (`LicenseToken.balanceOf(msg.sender, listingId) > 0`, where
///      `tokenId == listingId`). Agents with NO registered listing (pure
///      ERC-8004 identity, or platform/automated attestations) stay
///      permissionless, preserving the ERC-8004 draft's open model.
///
///      §3.7 revoke — {revokeFeedback} lets the original submitter (or the
///      admin) tombstone an entry. Storage stays append-only: nothing is
///      deleted; a tombstone flag is set and {FeedbackRevoked} is emitted so
///      an off-chain aggregate recompute can exclude that entry.
///
///      Access control for the wiring (license-token reference + per-agent
///      listing gate) is `Ownable` — the deploy-time admin is the platform
///      registrar, the same key that creates listings. The ERC-8004 feedback
///      path itself remains permissionless except for the per-listing gate.
///
///      TRUST MODEL (deliberate V2-testnet centralization — read before audit):
///      This contract is NOT trust-minimized. The owner is an operator-held EOA
///      (the platform registrar), and that key is intentionally privileged:
///        (a) Admin can unilaterally tombstone ANY rater's feedback via
///            {revokeFeedback} — not just its own entries — and can re-point or
///            clear the `agentId → listingId` gate mapping at any time via
///            {setListingForAgent}. A malicious or compromised owner can
///            therefore censor honest feedback, or open/close the license gate
///            on any agent at will. This is an accepted V2-testnet assumption,
///            not a property to be hardened here; trust-minimizing it (timelock,
///            multisig, per-rater self-custody of revocation) is deferred.
///        (b) Ownership is intended to PERSIST. The owner is not meant to renounce.
///            Renouncing ownership permanently freezes {setLicenseToken} (so the
///            license token can never be wired if it was not set pre-renounce),
///            {setListingForAgent} (gates can never be added, re-pointed, or
///            cleared again), and admin-side {revokeFeedback}. Renounce only with
///            full awareness that these admin operations become permanently
///            unavailable.
contract ReputationRegistry is Ownable {
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
        bool tombstoned;
    }

    /// @dev agentId => append-only feedback list.
    mapping(uint256 => Feedback[]) private _feedback;

    /// @dev The ERC-1155 LicenseToken whose `balanceOf(client, listingId)` the
    ///      §3.6 gate reads. Set once, post-deploy (LicenseToken is a proxy
    ///      deployed after this registry in the §8.3 sequence). A contract we
    ///      deploy and own — not an arbitrary attacker-controlled token.
    IERC1155 private _licenseToken;

    /// @dev agentId => listingId gate. 0 means "no listing registered" →
    ///      feedback for that agent is ungated (permissionless). `listingId`
    ///      starts at 1 in ListingRegistry, so 0 is an unambiguous "none".
    mapping(uint256 => uint256) private _listingForAgent;

    /// @notice Indexer-friendly event (surface spec §6.1). `agentId` and
    ///         `rater` are indexed for subgraph filtering. UNCHANGED.
    event FeedbackPosted(
        uint256 indexed agentId, address indexed rater, int128 value, bytes32 feedbackHash, string tag1
    );

    /// @notice Emitted when an entry is tombstoned (§3.7). Off-chain aggregate
    ///         recompute excludes tombstoned entries.
    event FeedbackRevoked(uint256 indexed agentId, uint256 indexed index, address indexed revoker);

    /// @notice Emitted when the LicenseToken reference is wired (one-shot).
    event LicenseTokenSet(address indexed licenseToken);

    /// @notice Emitted when a per-agent listing gate is registered/updated.
    event ListingForAgentSet(uint256 indexed agentId, uint256 indexed listingId);

    error LicenseTokenAlreadySet();
    error LicenseTokenNotSet();
    error ZeroAddress();
    error NotLicensed(uint256 agentId, uint256 listingId, address caller);
    error UnknownFeedback(uint256 agentId, uint256 index);
    error AlreadyRevoked(uint256 agentId, uint256 index);
    error NotFeedbackOwner(uint256 agentId, uint256 index, address caller);

    /// @param admin Platform registrar — owns the license-gate wiring.
    constructor(address admin) Ownable(admin) {}

    // -----------------------------------------------------------------------
    // Wiring (admin)
    // -----------------------------------------------------------------------

    /// @notice Wire the ERC-1155 LicenseToken. Callable exactly once by the
    ///         admin (one-shot, mirroring the other registries' wiring).
    function setLicenseToken(address licenseToken_) external onlyOwner {
        if (licenseToken_ == address(0)) revert ZeroAddress();
        if (address(_licenseToken) != address(0)) revert LicenseTokenAlreadySet();
        _licenseToken = IERC1155(licenseToken_);
        emit LicenseTokenSet(licenseToken_);
    }

    /// @notice Register the listing that gates feedback for `agentId`. Setting
    ///         a non-zero `listingId` turns the §3.6 license gate ON for that
    ///         agent; the registrar calls this when a strategy is listed
    ///         (agent = strategy = listing, AM3). Idempotent / re-pointable by
    ///         the admin (e.g. relisting). `listingId == 0` clears the gate.
    /// @dev    TRUST MODEL: this is an unrestricted owner power. The platform
    ///         registrar (an operator EOA) can point any agent's gate at any
    ///         listing, re-point it, or clear it (`listingId == 0`) at any time,
    ///         which silently flips that agent's feedback path between gated and
    ///         permissionless. This centralization is intentional for the
    ///         V2-testnet deployment and is NOT trust-minimized. Note also that
    ///         renouncing ownership permanently disables this function, freezing
    ///         every agent's gate in its last-set state (see the contract-level
    ///         trust-model note).
    function setListingForAgent(uint256 agentId, uint256 listingId) external onlyOwner {
        _listingForAgent[agentId] = listingId;
        emit ListingForAgentSet(agentId, listingId);
    }

    function licenseToken() external view returns (address) {
        return address(_licenseToken);
    }

    function listingForAgent(uint256 agentId) external view returns (uint256) {
        return _listingForAgent[agentId];
    }

    // -----------------------------------------------------------------------
    // Feedback (ERC-8004 §3.2 — gated by §3.6)
    // -----------------------------------------------------------------------

    /// @notice Post one feedback entry for `agentId`.
    /// @dev §3.6 gate: if a listing is registered for `agentId`, the caller
    ///      MUST hold ≥1 ERC-1155 license for that listing. Otherwise the
    ///      ERC-8004 permissionless model applies (anyone can rate). The
    ///      `balanceOf` read is to a contract we deploy and own (LicenseToken);
    ///      it is a `view` with no callback into this contract, so there is no
    ///      reentrancy surface. The single external call is the gate's last
    ///      act before state mutation; effects (the push + event) follow, which
    ///      also satisfies checks-effects-interactions (the check happens to be
    ///      an external view, and no further interaction occurs after it).
    /// @return feedbackId The index of the new entry within `agentId`'s log.
    function giveFeedback(
        uint256 agentId,
        int128 value,
        uint8 valueDecimals,
        string calldata tag1,
        string calldata tag2,
        string calldata endpoint,
        string calldata feedbackURI,
        bytes32 feedbackHash
    ) external returns (uint256 feedbackId) {
        // Checks: §3.6 license gate (only when a listing is registered).
        uint256 listingId = _listingForAgent[agentId];
        if (listingId != 0) {
            // Fail closed, but legibly: a gate is active yet no license token is
            // wired, so the gate cannot be satisfied by anyone. Revert with a
            // typed error rather than calling `balanceOf` on the zero address
            // (which would produce an opaque low-level revert).
            if (address(_licenseToken) == address(0)) revert LicenseTokenNotSet();
            if (_licenseToken.balanceOf(msg.sender, listingId) == 0) {
                revert NotLicensed(agentId, listingId, msg.sender);
            }
        }

        // Effects.
        feedbackId = _feedback[agentId].length;
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
                timestamp: block.timestamp,
                tombstoned: false
            })
        );
        emit FeedbackPosted(agentId, msg.sender, value, feedbackHash, tag1);
    }

    /// @notice Tombstone a feedback entry (§3.7). Callable by the original
    ///         submitter or the admin. History is preserved (append-only);
    ///         only a flag flips and {FeedbackRevoked} fires so off-chain
    ///         aggregation excludes the entry on next recompute.
    /// @dev    TRUST MODEL: the admin branch is an unrestricted owner power —
    ///         the platform registrar (an operator EOA) can tombstone ANY
    ///         rater's feedback, not only its own, and so can unilaterally
    ///         censor honest entries. This centralization is intentional for the
    ///         V2-testnet deployment and is NOT trust-minimized. Renouncing
    ///         ownership permanently disables the admin branch (see the
    ///         contract-level trust-model note); the original submitter can
    ///         always still revoke their own.
    function revokeFeedback(uint256 agentId, uint256 index) external {
        Feedback[] storage log = _feedback[agentId];
        if (index >= log.length) revert UnknownFeedback(agentId, index);

        Feedback storage fb = log[index];
        if (fb.tombstoned) revert AlreadyRevoked(agentId, index);
        if (msg.sender != fb.rater && msg.sender != owner()) {
            revert NotFeedbackOwner(agentId, index, msg.sender);
        }

        fb.tombstoned = true;
        emit FeedbackRevoked(agentId, index, msg.sender);
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// @notice Read one feedback entry by index. Return shape is UNCHANGED
    ///         (9-tuple) for ABI compatibility with the Rust reader; the
    ///         tombstone flag is exposed separately via {isTombstoned}.
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

    /// @notice True if entry `index` for `agentId` has been revoked (§3.7).
    /// @dev    Reverts {UnknownFeedback} for an out-of-range index (parity with
    ///         {revokeFeedback}) rather than surfacing a raw array OOB panic.
    function isTombstoned(uint256 agentId, uint256 index) external view returns (bool) {
        Feedback[] storage log = _feedback[agentId];
        if (index >= log.length) revert UnknownFeedback(agentId, index);
        return log[index].tombstoned;
    }

    /// @notice Number of feedback entries posted for `agentId` (incl. tombstoned).
    function getFeedbackCount(uint256 agentId) external view returns (uint256) {
        return _feedback[agentId].length;
    }
}

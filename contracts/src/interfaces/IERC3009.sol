// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title IERC3009 — the `transferWithAuthorization` slice of EIP-3009.
/// @notice USDC.e on Mantle is expected to implement EIP-3009, enabling the
///         x402 "sign off-chain, settle in one tx" buy path (surface spec §4.2).
///         Open question §11 / nav-doc B3: if USDC.e lacks EIP-3009, the x402
///         path falls back to Permit2 or a two-tx approve+buy. The direct
///         `Marketplace.buy` path does not depend on this interface.
interface IERC3009 {
    function transferWithAuthorization(
        address from,
        address to,
        uint256 value,
        uint256 validAfter,
        uint256 validBefore,
        bytes32 nonce,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external;

    /// @notice True if `nonce` has already been used by `authorizer`.
    function authorizationState(address authorizer, bytes32 nonce) external view returns (bool);
}

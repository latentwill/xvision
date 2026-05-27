// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title Splits — protocol-fee split math (surface spec §5).
/// @notice All math is 6-decimal USDC integer arithmetic. The fee is computed
///         by rounding *down*; the rounding dust accrues to the seller, never
///         the protocol (surface spec §5.2 convention).
library Splits {
    /// @notice Split `priceUSDC` into seller proceeds and protocol proceeds.
    /// @param priceUSDC The full sale price (6-decimal USDC).
    /// @param protocolFeeBps The per-listing fee snapshot, in basis points.
    /// @return sellerProceeds price - protocolProceeds (gets the rounding dust).
    /// @return protocolProceeds floor(price * bps / 10_000).
    function computeSplit(uint96 priceUSDC, uint16 protocolFeeBps)
        internal
        pure
        returns (uint96 sellerProceeds, uint96 protocolProceeds)
    {
        // protocolFeeBps is capped at MAX_PROTOCOL_FEE_BPS (1000) by the
        // Marketplace, so this product cannot overflow uint256 for any uint96
        // price, and the result fits back into uint96.
        protocolProceeds = uint96((uint256(priceUSDC) * protocolFeeBps) / 10_000);
        // Subtraction cannot underflow: protocolProceeds <= priceUSDC because
        // protocolFeeBps <= 10_000.
        sellerProceeds = priceUSDC - protocolProceeds;
    }
}

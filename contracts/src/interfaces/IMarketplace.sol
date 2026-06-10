// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title IMarketplace — sale surface (surface spec §3.2).
interface IMarketplace {
    /// @notice EIP-3009 `TransferWithAuthorization` payload for the x402 path.
    ///         The buyer signs this off-chain; a facilitator submits it.
    struct TransferAuthorization {
        address from;
        address to; // MUST equal the Marketplace address (settlement target)
        uint256 value; // MUST equal the listing price
        uint256 validAfter;
        uint256 validBefore;
        bytes32 nonce;
        uint8 v;
        bytes32 r;
        bytes32 s;
    }

    function buy(uint256 listingId, address recipient) external returns (uint256 licenseTokenId);

    function buyWithAuthorization(uint256 listingId, address recipient, TransferAuthorization calldata auth)
        external
        returns (uint256 licenseTokenId);

    function setProtocolFeeBps(uint16 newBps) external;

    function setFeeRecipient(address newRecipient) external;

    function setUsdc(address newUsdc) external;

    function protocolFeeBps() external view returns (uint16);

    function feeRecipient() external view returns (address);

    /// @dev `payerKind` is a **v1 placeholder** — in v1 it merely mirrors
    ///      `purchasePath` (0/1). DO NOT derive analytics from it yet. It is
    ///      typed `uint16` (wider than `purchasePath`'s tight enum) so the
    ///      eventual derivation can encode richer payer identity (EOA vs smart
    ///      account vs intent solver vs sponsored, possibly composite) without
    ///      an ABI change. Refinement ticket tracked in the marketplace program
    ///      plan §7.1 deferred-items register.
    event Sold(
        uint256 indexed listingId,
        uint256 indexed agentNftId,
        address indexed buyer,
        uint96 priceUSDC,
        uint96 sellerProceeds,
        uint96 protocolProceeds,
        uint256 licenseTokenId,
        uint16 payerKind, // v1 placeholder (mirrors purchasePath); see note above
        uint8 purchasePath // 0 = direct, 1 = x402
    );
    event ProtocolFeeBpsChanged(uint16 oldBps, uint16 newBps);
    event FeeRecipientChanged(address oldRecipient, address newRecipient);
    event UsdcChanged(address oldUsdc, address newUsdc);
}

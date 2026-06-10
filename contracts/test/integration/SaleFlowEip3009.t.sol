// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {Marketplace} from "../../src/Marketplace.sol";
import {IMarketplace} from "../../src/interfaces/IMarketplace.sol";
import {MockUSDC3009} from "../../src/test/MockUSDC3009.sol";

/// @notice End-to-end x402 sale flow against MockUSDC3009 — the EIP-3009 token
///         deployed on Mantle Sepolia. Unlike SaleFlow.t.sol (whose MockUSDC
///         skips signature checks), this exercises the REAL signed-authorization
///         path: the agent wallet signs an EIP-712 TransferWithAuthorization
///         off-chain and a facilitator submits `buyWithAuthorization` in one tx.
/// @dev Deploys a second Marketplace proxy pointed at MockUSDC3009 — the same
///      shape as the planned testnet UUPS re-point, against the same
///      ListingRegistry/LicenseToken wiring BaseTest sets up.
contract SaleFlowEip3009Test is BaseTest {
    MockUSDC3009 internal token;
    Marketplace internal market3009;

    address creator = makeAddr("creator");
    address facilitator = makeAddr("facilitator");
    address agentWallet;
    uint256 agentKey;

    function setUp() public override {
        super.setUp();
        (agentWallet, agentKey) = makeAddrAndKey("agentWallet3009");

        token = new MockUSDC3009();
        market3009 = Marketplace(
            _proxy(
                address(new Marketplace()),
                abi.encodeCall(
                    Marketplace.initialize,
                    (admin, address(listings), address(license), address(token), feeRecipient, INITIAL_FEE_BPS)
                )
            )
        );
        // Authorize the EIP-3009 marketplace to mint licenses alongside the
        // BaseTest one (ListingRegistry wiring is read-only for buys).
        license.setAuthorized(address(market3009), true);
    }

    function _signAuth(IMarketplace.TransferAuthorization memory auth)
        internal
        view
        returns (uint8 v, bytes32 r, bytes32 s)
    {
        bytes32 structHash = keccak256(
            abi.encode(
                token.TRANSFER_WITH_AUTHORIZATION_TYPEHASH(),
                auth.from,
                auth.to,
                auth.value,
                auth.validAfter,
                auth.validBefore,
                auth.nonce
            )
        );
        (v, r, s) = vm.sign(agentKey, keccak256(abi.encodePacked("\x19\x01", token.DOMAIN_SEPARATOR(), structHash)));
    }

    /// @dev Full x402 path with a REAL EIP-712 signature: agent wallet funds
    ///      itself from the faucet, signs the authorization, facilitator
    ///      submits — one tx settles USDC and mints the soulbound license.
    function test_endToEnd_x402Buy_realSignature() public {
        uint96 price = 5_000_000;
        uint256 lineage = _mintLineage(creator);
        uint256 lid = _createListing(creator, lineage, price, false);

        vm.prank(agentWallet);
        token.faucet(price);

        IMarketplace.TransferAuthorization memory auth = IMarketplace.TransferAuthorization({
            from: agentWallet,
            to: address(market3009),
            value: price,
            validAfter: 0,
            validBefore: type(uint256).max,
            nonce: keccak256("x402-3009-1"),
            v: 0,
            r: bytes32(0),
            s: bytes32(0)
        });
        (auth.v, auth.r, auth.s) = _signAuth(auth);

        vm.prank(facilitator);
        uint256 tokenId = market3009.buyWithAuthorization(lid, agentWallet, auth);

        assertGe(license.balanceOf(agentWallet, tokenId), 1);
        assertEq(token.balanceOf(agentWallet), 0);
        uint96 fee = uint96((uint256(price) * INITIAL_FEE_BPS) / 10_000);
        assertEq(token.balanceOf(creator), price - fee);
        assertEq(token.balanceOf(feeRecipient), fee);
        assertTrue(token.authorizationState(agentWallet, auth.nonce));
    }

    /// @dev A bad/forged signature reverts inside the token — no funds move,
    ///      no license mints.
    function test_x402Buy_forgedSignatureReverts() public {
        uint96 price = 5_000_000;
        uint256 lineage = _mintLineage(creator);
        uint256 lid = _createListing(creator, lineage, price, false);

        vm.prank(agentWallet);
        token.faucet(price);

        IMarketplace.TransferAuthorization memory auth = IMarketplace.TransferAuthorization({
            from: agentWallet,
            to: address(market3009),
            value: price,
            validAfter: 0,
            validBefore: type(uint256).max,
            nonce: keccak256("x402-3009-2"),
            v: 27,
            r: keccak256("garbage-r"),
            s: bytes32(uint256(1))
        });

        vm.prank(facilitator);
        vm.expectRevert();
        market3009.buyWithAuthorization(lid, agentWallet, auth);

        assertEq(token.balanceOf(agentWallet), price, "no funds moved");
        assertEq(license.balanceOf(agentWallet, lid), 0, "no license minted");
    }

    /// @dev Replaying the same signed authorization through the Marketplace is
    ///      rejected by the token's nonce state.
    function test_x402Buy_replayRejected() public {
        uint96 price = 2_000_000;
        uint256 lineage = _mintLineage(creator);
        uint256 lid = _createListing(creator, lineage, price, false);

        vm.startPrank(agentWallet);
        token.faucet(price);
        token.faucet(price); // funds for a hypothetical second buy
        vm.stopPrank();

        IMarketplace.TransferAuthorization memory auth = IMarketplace.TransferAuthorization({
            from: agentWallet,
            to: address(market3009),
            value: price,
            validAfter: 0,
            validBefore: type(uint256).max,
            nonce: keccak256("x402-3009-3"),
            v: 0,
            r: bytes32(0),
            s: bytes32(0)
        });
        (auth.v, auth.r, auth.s) = _signAuth(auth);

        vm.prank(facilitator);
        market3009.buyWithAuthorization(lid, agentWallet, auth);

        vm.prank(facilitator);
        vm.expectRevert("MockUSDC3009: authorization used or canceled");
        market3009.buyWithAuthorization(lid, agentWallet, auth);
    }

    /// @dev The direct approve+buy path also works against MockUSDC3009.
    function test_directBuy_against3009Token() public {
        uint96 price = 20_000_000;
        uint256 lineage = _mintLineage(creator);
        uint256 lid = _createListing(creator, lineage, price, false);

        address humanBuyer = makeAddr("humanBuyer3009");
        vm.startPrank(humanBuyer);
        token.faucet(price);
        token.approve(address(market3009), price);
        uint256 tokenId = market3009.buy(lid, humanBuyer);
        vm.stopPrank();

        assertGe(license.balanceOf(humanBuyer, tokenId), 1);
        uint96 fee = uint96((uint256(price) * INITIAL_FEE_BPS) / 10_000);
        assertEq(token.balanceOf(creator), price - fee);
        assertEq(token.balanceOf(feeRecipient), fee);
    }
}

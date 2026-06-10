// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {Marketplace} from "../../src/Marketplace.sol";
import {IMarketplace} from "../../src/interfaces/IMarketplace.sol";
import {MockUSDC3009} from "../../src/test/MockUSDC3009.sol";

/// @notice Mirrors the live Mantle Sepolia upgrade
///         (script/UpgradeMarketplaceSetUsdc.s.sol): a V1 Marketplace proxy
///         initialized against the old non-3009 MockUSDC is upgraded via
///         `upgradeToAndCall` to a fresh implementation with `setUsdc` calldata
///         pointing at the EIP-3009 MockUSDC3009, then the x402
///         `buyWithAuthorization` path is exercised end-to-end against the
///         SAME proxy address with a real EIP-712 signature.
contract UpgradeSetUsdcTest is BaseTest {
    MockUSDC3009 internal token3009;

    address creator = makeAddr("creator");
    address facilitator = makeAddr("facilitator");
    address agentWallet;
    uint256 agentKey;

    function setUp() public override {
        super.setUp(); // `market` proxy is initialized with the old MockUSDC.
        (agentWallet, agentKey) = makeAddrAndKey("agentWalletUpgrade");
        token3009 = new MockUSDC3009();
    }

    function _signAuth(IMarketplace.TransferAuthorization memory auth)
        internal
        view
        returns (uint8 v, bytes32 r, bytes32 s)
    {
        bytes32 structHash = keccak256(
            abi.encode(
                token3009.TRANSFER_WITH_AUTHORIZATION_TYPEHASH(),
                auth.from,
                auth.to,
                auth.value,
                auth.validAfter,
                auth.validBefore,
                auth.nonce
            )
        );
        (v, r, s) = vm.sign(agentKey, keccak256(abi.encodePacked("\x19\x01", token3009.DOMAIN_SEPARATOR(), structHash)));
    }

    function test_upgradeToAndCall_setUsdc_thenX402Buy() public {
        address oldToken = market.usdc();
        assertEq(oldToken, address(usdc), "precondition: proxy points at old MockUSDC");

        // Upgrade: new implementation + setUsdc(token3009) in one tx — the
        // exact shape of the live testnet upgrade.
        Marketplace newImpl = new Marketplace();
        vm.expectEmit(true, true, false, false, address(market));
        emit IMarketplace.UsdcChanged(oldToken, address(token3009));
        market.upgradeToAndCall(address(newImpl), abi.encodeCall(Marketplace.setUsdc, (address(token3009))));

        // Re-pointed, and pre-upgrade state preserved.
        assertEq(market.usdc(), address(token3009));
        assertEq(market.protocolFeeBps(), INITIAL_FEE_BPS);
        assertEq(market.feeRecipient(), feeRecipient);
        assertEq(market.listingRegistry(), address(listings));
        assertEq(market.licenseToken(), address(license));

        // x402 buy against the upgraded proxy with a real EIP-712 signature.
        uint96 price = 5_000_000;
        uint256 lineage = _mintLineage(creator);
        uint256 lid = _createListing(creator, lineage, price, false);

        vm.prank(agentWallet);
        token3009.faucet(price);

        IMarketplace.TransferAuthorization memory auth = IMarketplace.TransferAuthorization({
            from: agentWallet,
            to: address(market),
            value: price,
            validAfter: 0,
            validBefore: type(uint256).max,
            nonce: keccak256("upgrade-setusdc-1"),
            v: 0,
            r: bytes32(0),
            s: bytes32(0)
        });
        (auth.v, auth.r, auth.s) = _signAuth(auth);

        vm.prank(facilitator);
        uint256 tokenId = market.buyWithAuthorization(lid, agentWallet, auth);

        assertGe(license.balanceOf(agentWallet, tokenId), 1);
        assertEq(token3009.balanceOf(agentWallet), 0);
        uint96 fee = uint96((uint256(price) * INITIAL_FEE_BPS) / 10_000);
        assertEq(token3009.balanceOf(creator), price - fee);
        assertEq(token3009.balanceOf(feeRecipient), fee);
        assertTrue(token3009.authorizationState(agentWallet, auth.nonce));
    }
}

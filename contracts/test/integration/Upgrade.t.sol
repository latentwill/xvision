// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {BaseTest} from "../BaseTest.sol";
import {MarketplaceV2} from "../mocks/MarketplaceV2.sol";
import {Marketplace} from "../../src/Marketplace.sol";
import {IMarketplace} from "../../src/interfaces/IMarketplace.sol";
import {MockUSDC3009} from "../../src/test/MockUSDC3009.sol";

/// @notice UUPS upgrade-safety: state survives an implementation swap, and the
///         upgrade is owner-gated. (Local equivalent of the §9.3 fork test; the
///         mainnet-state replay version lives in test/fork/.)
contract UpgradeTest is BaseTest {
    address seller = makeAddr("seller");
    address buyer = makeAddr("buyer");

    function test_upgrade_preservesStateAndAddsBehavior() public {
        // Seed live state: a listing and a sold license.
        uint96 price = 15_000_000;
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, price, false);
        _fundAndApprove(buyer, price);
        vm.prank(buyer);
        market.buy(lid, buyer);
        assertEq(license.balanceOf(buyer, lid), 1);

        // Upgrade the Marketplace implementation.
        MarketplaceV2 v2 = new MarketplaceV2();
        market.upgradeToAndCall(address(v2), "");

        // Pre-upgrade state preserved through the proxy.
        assertEq(market.protocolFeeBps(), INITIAL_FEE_BPS);
        assertEq(market.feeRecipient(), feeRecipient);
        assertEq(market.listingRegistry(), address(listings));
        assertEq(license.balanceOf(buyer, lid), 1);

        // New behavior available at the same address.
        assertEq(MarketplaceV2(address(market)).version(), "v2");
    }

    function test_upgrade_onlyOwner() public {
        MarketplaceV2 v2 = new MarketplaceV2();
        vm.prank(makeAddr("intruder"));
        vm.expectRevert(); // OwnableUnauthorizedAccount
        market.upgradeToAndCall(address(v2), "");
    }

    /// @dev The exact shape of the planned testnet re-point: one atomic
    ///      `upgradeToAndCall(newImpl, setUsdc(newToken))` swaps the sale
    ///      currency to the EIP-3009 token, after which the x402 path works
    ///      with a real signed authorization through the SAME proxy.
    function test_upgrade_setUsdc_repointsTokenAndBuysViaEip3009() public {
        // Seed pre-upgrade state: a listing priced in the old MockUSDC.
        uint96 price = 5_000_000;
        uint256 lineage = _mintLineage(seller);
        uint256 lid = _createListing(seller, lineage, price, false);
        address oldToken = market.usdc();

        // Atomic upgrade + re-point.
        MockUSDC3009 token = new MockUSDC3009();
        vm.expectEmit(address(market));
        emit IMarketplace.UsdcChanged(oldToken, address(token));
        market.upgradeToAndCall(
            address(new Marketplace()), abi.encodeCall(Marketplace.setUsdc, (address(token)))
        );
        assertEq(market.usdc(), address(token));

        // Pre-upgrade state preserved.
        assertEq(market.protocolFeeBps(), INITIAL_FEE_BPS);
        assertEq(market.feeRecipient(), feeRecipient);
        assertEq(market.listingRegistry(), address(listings));

        // x402 buy with a REAL EIP-712 signature against the new token.
        (address agentWallet, uint256 agentKey) = makeAddrAndKey("agentWalletRepoint");
        vm.prank(agentWallet);
        token.faucet(price);

        IMarketplace.TransferAuthorization memory auth = IMarketplace.TransferAuthorization({
            from: agentWallet,
            to: address(market),
            value: price,
            validAfter: 0,
            validBefore: type(uint256).max,
            nonce: keccak256("repoint-3009-1"),
            v: 0,
            r: bytes32(0),
            s: bytes32(0)
        });
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
        (auth.v, auth.r, auth.s) =
            vm.sign(agentKey, keccak256(abi.encodePacked("\x19\x01", token.DOMAIN_SEPARATOR(), structHash)));

        vm.prank(makeAddr("facilitator"));
        uint256 tokenId = market.buyWithAuthorization(lid, agentWallet, auth);

        assertGe(license.balanceOf(agentWallet, tokenId), 1);
        assertEq(token.balanceOf(agentWallet), 0);
        uint96 fee = uint96((uint256(price) * INITIAL_FEE_BPS) / 10_000);
        assertEq(token.balanceOf(seller), price - fee);
        assertEq(token.balanceOf(feeRecipient), fee);
    }
}

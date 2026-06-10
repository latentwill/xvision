// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Script, console2} from "forge-std/Script.sol";
import {Marketplace} from "../src/Marketplace.sol";

/// @title UpgradeMarketplaceSetUsdc — UUPS-upgrade the Mantle Sepolia
///        Marketplace proxy to an implementation with `setUsdc`, atomically
///        re-pointing the sale currency at the EIP-3009 MockUSDC3009.
///        TESTNET ONLY.
/// @notice The original deploy initialized `_usdc` to a community mock with no
///         EIP-3009 support, which dead-ends the x402 `buyWithAuthorization`
///         path. This script deploys the new Marketplace implementation and
///         calls `upgradeToAndCall(newImpl, setUsdc(MOCK_USDC_3009))` as the
///         proxy owner — one atomic tx, storage layout unchanged.
///
/// @dev Usage (key passed inline-env, never echoed):
///        PRIVATE_KEY=... forge script script/UpgradeMarketplaceSetUsdc.s.sol \
///          --rpc-url https://rpc.sepolia.mantle.xyz --broadcast
contract UpgradeMarketplaceSetUsdc is Script {
    error WrongChain(uint256 chainId);
    error UsdcUnchanged(address usdc);

    /// @dev Mantle Sepolia Marketplace UUPS proxy (config/mantle-sepolia.toml).
    address constant MARKETPLACE_PROXY = 0x4b9450642f2b3Da248e90b4FEBaA8eCA87E78cE8;
    /// @dev MockUSDC3009 deployed by DeployMockUsdc.s.sol (Sourcify-verified).
    address constant MOCK_USDC_3009 = 0x68AA91f73F359035875759e1d4C4949A27c84c88;

    function run() external returns (address newImpl) {
        // Hard guard: V2 testnet upgrade, Mantle Sepolia only.
        if (block.chainid != 5003) revert WrongChain(block.chainid);

        uint256 key = vm.envUint("PRIVATE_KEY");
        Marketplace proxy = Marketplace(MARKETPLACE_PROXY);

        vm.startBroadcast(key);
        newImpl = address(new Marketplace());
        proxy.upgradeToAndCall(newImpl, abi.encodeCall(Marketplace.setUsdc, (MOCK_USDC_3009)));
        vm.stopBroadcast();

        if (proxy.usdc() != MOCK_USDC_3009) revert UsdcUnchanged(proxy.usdc());

        console2.log("Marketplace impl upgraded:", newImpl);
        console2.log("usdc() now:", proxy.usdc());
        console2.log("-> Sourcify-verify the new impl, then smoke an x402 buy");
    }
}

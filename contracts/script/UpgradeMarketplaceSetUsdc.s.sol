// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Script, console2} from "forge-std/Script.sol";
import {Marketplace} from "../src/Marketplace.sol";

/// @title UpgradeMarketplaceSetUsdc — UUPS-upgrade the Mantle Sepolia
///        Marketplace proxy to an implementation with `setUsdc`, re-pointing
///        the sale currency at the EIP-3009 MockUSDC3009. TESTNET ONLY.
/// @notice Deploys a fresh Marketplace implementation, then calls
///         `upgradeToAndCall(newImpl, setUsdc(MOCK_USDC_3009))` on the proxy
///         as the owner (operator EOA) — one tx swaps the impl and re-points
///         the token. Old-token allowances/balances do NOT carry over.
///
/// @dev Usage (key passed inline-env, never echoed):
///        PRIVATE_KEY=... forge script script/UpgradeMarketplaceSetUsdc.s.sol \
///          --rpc-url https://rpc.sepolia.mantle.xyz --broadcast
contract UpgradeMarketplaceSetUsdc is Script {
    /// @dev Marketplace UUPS proxy on Mantle Sepolia.
    address constant MARKETPLACE_PROXY = 0x4b9450642f2b3Da248e90b4FEBaA8eCA87E78cE8;
    /// @dev MockUSDC3009 (EIP-3009, faucet-capped) on Mantle Sepolia.
    address constant MOCK_USDC_3009 = 0x68AA91f73F359035875759e1d4C4949A27c84c88;

    error WrongChain(uint256 chainId);
    error UsdcNotRepointed(address got);

    function run() external returns (address newImpl) {
        // Hard guard: Mantle Sepolia only.
        if (block.chainid != 5003) revert WrongChain(block.chainid);

        uint256 key = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(key);
        newImpl = address(new Marketplace());
        Marketplace(MARKETPLACE_PROXY).upgradeToAndCall(newImpl, abi.encodeCall(Marketplace.setUsdc, (MOCK_USDC_3009)));
        vm.stopBroadcast();

        address usdcNow = Marketplace(MARKETPLACE_PROXY).usdc();
        if (usdcNow != MOCK_USDC_3009) revert UsdcNotRepointed(usdcNow);

        console2.log("New Marketplace impl:", newImpl);
        console2.log("Proxy usdc() now:", usdcNow);
        console2.log("-> Sourcify-verify the new impl and update config/mantle-sepolia.toml");
    }
}

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Script, console2} from "forge-std/Script.sol";
import {MockUSDC3009} from "../src/test/MockUSDC3009.sol";

/// @title DeployMockUsdc — deploy the EIP-3009-capable test USDC to Mantle
///        Sepolia (chain 5003). TESTNET ONLY.
/// @notice No EIP-3009 USDC exists on Mantle Sepolia, so the x402
///         `buyWithAuthorization` path needs this stand-in. Mantle MAINNET
///         bridged USDC (0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9) supports
///         EIP-3009 natively — never deploy this there.
///
/// @dev Usage (key passed inline-env, never echoed):
///        PRIVATE_KEY=... forge script script/DeployMockUsdc.s.sol \
///          --rpc-url https://rpc.sepolia.mantle.xyz --broadcast
contract DeployMockUsdc is Script {
    error WrongChain(uint256 chainId);

    function run() external returns (address token) {
        // Hard guard: testnet token, Mantle Sepolia only.
        if (block.chainid != 5003) revert WrongChain(block.chainid);

        uint256 key = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(key);
        token = address(new MockUSDC3009());
        vm.stopBroadcast();

        console2.log("MockUSDC3009 (Mantle Sepolia):", token);
        console2.log("-> update [marketplace.usdc] in config/mantle-sepolia.toml");
        console2.log("-> Marketplace re-point requires a UUPS upgrade (no setter)");
    }
}

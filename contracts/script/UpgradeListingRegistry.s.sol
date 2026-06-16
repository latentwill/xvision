// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Script, console2} from "forge-std/Script.sol";
import {ListingRegistry} from "../src/ListingRegistry.sol";

/// @title UpgradeListingRegistry — UUPS-upgrade the Mantle Sepolia
///        ListingRegistry proxy to an implementation that adds `updatePrice`
///        (seller-only in-place repricing). TESTNET ONLY.
/// @notice Adds a function only — no new storage, no initializer — so the
///         upgrade is a bare `upgradeToAndCall(newImpl, "")`. The proxy address
///         and every existing listing are preserved.
///
/// @dev Usage (key passed inline-env, never echoed):
///        PRIVATE_KEY=... forge script script/UpgradeListingRegistry.s.sol \
///          --rpc-url https://rpc.sepolia.mantle.xyz --broadcast
contract UpgradeListingRegistry is Script {
    /// @dev ListingRegistry UUPS proxy on Mantle Sepolia (config/mantle-sepolia.toml).
    address constant LISTING_REGISTRY_PROXY = 0x64b5ae5B91CB2846e7dA8cE883f2023b98E2cD22;

    error WrongChain(uint256 chainId);

    function run() external returns (address newImpl) {
        // Hard guard: Mantle Sepolia only.
        if (block.chainid != 5003) revert WrongChain(block.chainid);

        uint256 key = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(key);
        newImpl = address(new ListingRegistry());
        // Bare impl swap — updatePrice adds no state, so no initializer call.
        ListingRegistry(LISTING_REGISTRY_PROXY).upgradeToAndCall(newImpl, "");
        vm.stopBroadcast();

        // Sanity: the proxy still answers a read after the swap (storage intact).
        uint256 total = ListingRegistry(LISTING_REGISTRY_PROXY).totalListings();
        console2.log("New ListingRegistry impl:", newImpl);
        console2.log("Proxy totalListings() still:", total);
        console2.log("-> Sourcify-verify the new impl; proxy address is unchanged.");
    }
}

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Script, console2} from "forge-std/Script.sol";
import {ListingRegistry} from "../src/ListingRegistry.sol";

/// @title UpgradeListingRegistryMainnet — UUPS-upgrade the **Mantle MAINNET**
///        ListingRegistry proxy to an implementation that adds `updatePrice`
///        (seller-only in-place repricing).
/// @notice PRODUCTION. Adds a function only — no new storage, no initializer —
///         so this is a bare `upgradeToAndCall(newImpl, "")`. The proxy address
///         and every existing listing are preserved. The new impl is whatever
///         `src/ListingRegistry.sol` compiles to in THIS checkout — run only
///         from a current, reviewed `main`.
///
/// @dev The proxy is currently owned by the operator EOA (config/mantle.toml
///      `admin`); `upgradeToAndCall` reverts unless broadcast by that owner.
///      If ownership has since moved to a timelock/multisig, DO NOT use this
///      script — route the upgrade through that governance instead.
///
///      Usage (key passed inline-env, never echoed; operator runs this):
///        PRIVATE_KEY=... forge script script/UpgradeListingRegistryMainnet.s.sol \
///          --rpc-url https://rpc.mantle.xyz --broadcast
contract UpgradeListingRegistryMainnet is Script {
    /// @dev ListingRegistry UUPS proxy on Mantle MAINNET (config/mantle.toml).
    address constant LISTING_REGISTRY_PROXY = 0xF491b6102F5c50Db46AeEc7fFb3D520aaF2f0151;

    error WrongChain(uint256 chainId);

    function run() external returns (address newImpl) {
        // Hard guard: Mantle MAINNET only (chain 5000).
        if (block.chainid != 5000) revert WrongChain(block.chainid);

        uint256 key = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(key);
        newImpl = address(new ListingRegistry());
        // Bare impl swap — updatePrice adds no state, so no initializer call.
        ListingRegistry(LISTING_REGISTRY_PROXY).upgradeToAndCall(newImpl, "");
        vm.stopBroadcast();

        // Sanity: the proxy still answers a read after the swap (storage intact).
        uint256 total = ListingRegistry(LISTING_REGISTRY_PROXY).totalListings();
        console2.log("New ListingRegistry impl (mainnet):", newImpl);
        console2.log("Proxy totalListings() still:", total);
        console2.log("-> Sourcify-verify the new impl; proxy address is unchanged.");
    }
}

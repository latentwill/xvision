// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Test} from "forge-std/Test.sol";

/// @title ForkUpgradeTest — replay an implementation upgrade against LIVE chain
///        state before a mainnet/V4 change (surface spec §9.3).
/// @dev Env-gated: skipped unless `MANTLE_SEPOLIA_RPC_URL` is set AND the
///      contracts have been deployed (addresses loaded from
///      `config/mantle-sepolia.toml`). Until Phase 3/5 deploy actually happens
///      there is no live state to fork, so this is a documented harness, not a
///      passing assertion. See README §"Fork tests".
contract ForkUpgradeTest is Test {
    function test_fork_upgradePreservesListingsAndLicenses() public {
        string memory rpc = vm.envOr("MANTLE_SEPOLIA_RPC_URL", string(""));
        if (bytes(rpc).length == 0) {
            vm.skip(true);
            return;
        }

        vm.createSelectFork(rpc);

        // TODO(Phase 5 deploy): once addresses exist on Mantle Sepolia —
        //   1. load Marketplace / ListingRegistry / LicenseToken proxy addrs;
        //   2. snapshot a known listing + a license balance;
        //   3. prank the operator EOA, deploy + upgradeToAndCall(newImpl);
        //   4. assert the listing struct and license balance are unchanged;
        //   5. assert v1 event topic-zeros still emit (storage-layout invariant
        //      is separately enforced by `forge inspect ... storageLayout` in CI).
        vm.skip(true);
    }
}

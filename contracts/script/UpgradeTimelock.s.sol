// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Script} from "forge-std/Script.sol";

/// @title UpgradeTimelock — queue/execute UUPS upgrades through the 7-day
///        TimelockController. V4-GATED STUB.
/// @notice Not implemented for V2. In V2 testnet the proxy admin is the operator
///         EOA and upgrades are a direct `upgradeToAndCall` (see the local test
///         in test/integration/Upgrade.t.sol). The timelock + 2-of-3 multisig
///         land in V4 prep (surface spec §7.2, §7.4), and every mainnet upgrade
///         that goes through `schedule()` requires a fresh external audit first
///         (§9.5).
///
///         When implemented, this exposes two entrypoints — `queue(newImpl)`
///         (multisig calls `TimelockController.schedule`) and `execute(newImpl)`
///         (after the 7-day delay) — plus storage-layout verification against
///         the on-chain implementation before either.
contract UpgradeTimelock is Script {
    error TimelockUpgradeIsV4Gated();

    function run() external pure {
        revert TimelockUpgradeIsV4Gated();
    }
}

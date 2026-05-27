// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Script} from "forge-std/Script.sol";

/// @title DeployMainnet — Mantle mainnet (chain 5000) deploy. V4-GATED STUB.
/// @notice Deliberately NOT implemented. Mainnet deploy is gated on the V2 exit
///         gate, an external audit, and the timelock + 2-of-3 multisig governance
///         setup (surface spec §7.4, §9.5; blockchain nav doc Phase 8 / V4 prep).
///
///         When unblocked, this mirrors `DeployTestnet` exactly — SAME nonce-0
///         EOA, SAME `XvnDeployer` address, SAME `keccak256("xvn.<name>.v1")`
///         salts — so the four marketplace proxies land at the addresses already
///         predictable from testnet. The only deltas: admin/feeRecipient point
///         at the TimelockController (not the operator EOA), and the chain id
///         guard rejects anything but 5000.
contract DeployMainnet is Script {
    error MainnetDeployIsV4Gated();

    function run() external pure {
        // V4 prep wires: timelock + multisig admin, audited impls, faucet/funded
        // treasury fee recipient. Until then this must not run.
        revert MainnetDeployIsV4Gated();
    }
}

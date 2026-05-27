// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Script, console2} from "forge-std/Script.sol";
import {IdentityRegistry} from "../src/registries/IdentityRegistry.sol";

/// @title RegisterPlatformAgent — mint xvn itself as ERC-8004 agent #0
///        (surface spec §3.5). REVIEW/RESEARCH ARTIFACT — not run in this slice.
/// @dev One-shot. Registers the platform manifest CID so 8004-aware indexers
///      (0xbits, dmihal, AgentCity) light up xvn for free (the GEO play).
///      Run on a FRESH IdentityRegistry so the platform claims token id 0;
///      `config/*.toml`'s `platform_agent_token_id` then equals 0.
///
///      Required env:
///        IDENTITY_REGISTRY     — deployed IdentityRegistry address
///        PLATFORM_MANIFEST_URI — ipfs://<cid> of the platform-agent manifest
contract RegisterPlatformAgent is Script {
    function run() external returns (uint256 tokenId) {
        address identity = vm.envAddress("IDENTITY_REGISTRY");
        string memory manifestUri = vm.envString("PLATFORM_MANIFEST_URI");

        vm.startBroadcast();
        tokenId = IdentityRegistry(identity).register(manifestUri);
        vm.stopBroadcast();

        require(tokenId == 0, "RegisterPlatformAgent: registry not fresh (agent #0 expected)");
        console2.log("Platform agent registered as token id:", tokenId);
        console2.log("Manifest:", manifestUri);
    }
}

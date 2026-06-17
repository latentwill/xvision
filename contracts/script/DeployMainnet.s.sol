// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Script, console2} from "forge-std/Script.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";

import {XvnDeployer} from "../src/XvnDeployer.sol";
import {IdentityRegistry} from "../src/registries/IdentityRegistry.sol";
import {ReputationRegistry} from "../src/registries/ReputationRegistry.sol";
import {ValidationRegistry} from "../src/registries/ValidationRegistry.sol";
import {LicenseToken} from "../src/LicenseToken.sol";
import {ListingRegistry} from "../src/ListingRegistry.sol";
import {EvalAttestationRegistry} from "../src/EvalAttestationRegistry.sol";
import {Marketplace} from "../src/Marketplace.sol";

/// @title DeployMainnet — Mantle mainnet (chain 5000) deploy in the §8.3 order.
/// @notice HACKATHON-GRADE deploy (operator decision 2026-06-14). This
///         deliberately bypasses the V4 governance gate (external audit +
///         TimelockController + 2-of-3 multisig) and deploys with the **operator
///         EOA as proxy admin and fee recipient** — same posture as
///         `DeployTestnet`, just on mainnet. It is NOT production-secured: the
///         EOA admin can upgrade/rug the proxies and there is no audit. For a
///         production cutover, redeploy (or `UpgradeTimelock`) with
///         admin/feeRecipient pointing at a TimelockController + treasury
///         multisig. See the V4 prep notes (surface spec §7.4, §9.5).
///
/// @dev Determinism (surface spec §5, §6.5): byte-for-byte identical to
///      `DeployTestnet` — every contract goes through the CREATE2 `XvnDeployer`
///      with `keccak256("xvn.<name>.v1")` salts, so reusing the SAME nonce-0 EOA
///      that deployed the testnet factory yields the SAME addresses on mainnet.
///      Proxies are initialized atomically via constructor `_data` (no
///      uninitialized front-run window).
///
///      Required env:
///        OPERATOR_EOA      — proxy admin + fee recipient (hackathon-grade)
///        USDC_ADDRESS      — USDC.e on Mantle mainnet (chain 5000)
///        LICENSE_URI       — ERC-1155 metadata URI template (e.g. ".../{id}")
///      Optional env:
///        XVN_DEPLOYER      — pre-deployed factory; if unset a fresh one is
///                            deployed (MUST be from a nonce-0 EOA for the
///                            cross-chain address guarantee).
///        PROTOCOL_FEE_BPS  — default 500 (5%).
contract DeployMainnet is Script {
    error WrongChain(uint256 got, uint256 want);

    struct Deployed {
        address xvnDeployer;
        address identityRegistry;
        address reputationRegistry;
        address validationRegistry;
        address licenseToken;
        address listingRegistry;
        address evalAttestation;
        address marketplace;
    }

    function run() external returns (Deployed memory d) {
        // Fail-closed: this script is mainnet-only. Refuse to broadcast against
        // any other chain (e.g. an accidental testnet/anvil RPC).
        if (block.chainid != 5000) revert WrongChain(block.chainid, 5000);

        address operator = vm.envAddress("OPERATOR_EOA");
        address usdc = vm.envAddress("USDC_ADDRESS");
        string memory licenseUri = vm.envString("LICENSE_URI");
        uint16 feeBps = uint16(vm.envOr("PROTOCOL_FEE_BPS", uint256(500)));

        vm.startBroadcast();

        XvnDeployer factory = _factory();
        d.xvnDeployer = address(factory);

        // 1-3. Immutable ERC-8004 registries (no proxy), via the factory so
        //      their addresses are deterministic too.
        d.identityRegistry = _via(factory, _salt("IdentityRegistry"), type(IdentityRegistry).creationCode);
        // ReputationRegistry takes an `admin` (registrar) constructor arg for the
        // §3.6 license-gate wiring — append it to the creation code.
        d.reputationRegistry = _via(
            factory,
            _salt("ReputationRegistry"),
            abi.encodePacked(type(ReputationRegistry).creationCode, abi.encode(operator))
        );
        d.validationRegistry = _via(factory, _salt("ValidationRegistry"), type(ValidationRegistry).creationCode);

        // 5. LicenseToken (UUPS) — init(admin, uri). Minter set empty.
        d.licenseToken = _proxy(
            factory,
            "LicenseToken",
            type(LicenseToken).creationCode,
            abi.encodeCall(LicenseToken.initialize, (operator, licenseUri))
        );

        // 6. ListingRegistry (UUPS) — reads IdentityRegistry.
        d.listingRegistry = _proxy(
            factory,
            "ListingRegistry",
            type(ListingRegistry).creationCode,
            abi.encodeCall(ListingRegistry.initialize, (operator, d.identityRegistry))
        );

        // 7. EvalAttestationRegistry (UUPS).
        d.evalAttestation = _proxy(
            factory,
            "EvalAttestationRegistry",
            type(EvalAttestationRegistry).creationCode,
            abi.encodeCall(EvalAttestationRegistry.initialize, (operator))
        );

        // 8. Marketplace (UUPS) — reads ListingRegistry, calls LicenseToken.
        d.marketplace = _proxy(
            factory,
            "Marketplace",
            type(Marketplace).creationCode,
            abi.encodeCall(
                Marketplace.initialize, (operator, d.listingRegistry, d.licenseToken, usdc, operator, feeBps)
            )
        );

        // Atomic wiring (operator == owner of all proxies).
        LicenseToken(d.licenseToken).setAuthorized(d.marketplace, true);
        LicenseToken(d.licenseToken).setListingRegistry(d.listingRegistry);
        ListingRegistry(d.listingRegistry).setMarketplace(d.marketplace);
        // §3.6: wire the LicenseToken into the ReputationRegistry so per-listing
        // feedback gates can read `balanceOf`.
        ReputationRegistry(d.reputationRegistry).setLicenseToken(d.licenseToken);
        // Wire ReputationRegistry <-> ListingRegistry so per-agent feedback gates
        // are set ATOMICALLY when a strategy is listed (createListing ->
        // setListingForAgent). The ListingRegistry is the authorized registrar.
        ReputationRegistry(d.reputationRegistry).setListingRegistrar(d.listingRegistry);
        ListingRegistry(d.listingRegistry).setReputationRegistry(d.reputationRegistry);

        vm.stopBroadcast();

        _log(d);
    }

    // -----------------------------------------------------------------------
    // Helpers (identical to DeployTestnet — keep in sync)
    // -----------------------------------------------------------------------

    function _factory() internal returns (XvnDeployer) {
        address existing = vm.envOr("XVN_DEPLOYER", address(0));
        if (existing != address(0)) return XvnDeployer(existing);
        console2.log("WARNING: deploying a fresh XvnDeployer. For the cross-chain");
        console2.log("address guarantee this MUST come from a nonce-0 EOA.");
        return new XvnDeployer();
    }

    function _salt(string memory name) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked("xvn.", name, ".v1"));
    }

    function _implSalt(string memory name) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked("xvn.", name, ".impl.v1"));
    }

    /// @dev Deploy a non-proxied contract (registry) via CREATE2.
    function _via(XvnDeployer factory, bytes32 salt, bytes memory code) internal returns (address) {
        address predicted = factory.computeAddress(salt, keccak256(code));
        address deployed = factory.deploy(salt, code);
        require(deployed == predicted, "DeployMainnet: addr mismatch");
        return deployed;
    }

    /// @dev Deploy a UUPS impl + initialized ERC1967 proxy, both via CREATE2.
    function _proxy(XvnDeployer factory, string memory name, bytes memory implCode, bytes memory initData)
        internal
        returns (address proxy)
    {
        address impl = _via(factory, _implSalt(name), implCode);
        bytes memory proxyCode = abi.encodePacked(type(ERC1967Proxy).creationCode, abi.encode(impl, initData));
        proxy = _via(factory, _salt(name), proxyCode);
    }

    function _log(Deployed memory d) internal pure {
        console2.log("=== xvn marketplace - Mantle MAINNET (chain 5000) ===");
        console2.log("XvnDeployer            ", d.xvnDeployer);
        console2.log("IdentityRegistry       ", d.identityRegistry);
        console2.log("ReputationRegistry     ", d.reputationRegistry);
        console2.log("ValidationRegistry     ", d.validationRegistry);
        console2.log("LicenseToken (proxy)   ", d.licenseToken);
        console2.log("ListingRegistry (proxy)", d.listingRegistry);
        console2.log("EvalAttestation (proxy)", d.evalAttestation);
        console2.log("Marketplace (proxy)    ", d.marketplace);
        console2.log("-> copy these into config/mantle.toml");
    }
}

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Test} from "forge-std/Test.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";

import {IdentityRegistry} from "../src/registries/IdentityRegistry.sol";
import {ReputationRegistry} from "../src/registries/ReputationRegistry.sol";
import {ValidationRegistry} from "../src/registries/ValidationRegistry.sol";
import {ListingRegistry} from "../src/ListingRegistry.sol";
import {LicenseToken} from "../src/LicenseToken.sol";
import {Marketplace} from "../src/Marketplace.sol";
import {EvalAttestationRegistry} from "../src/EvalAttestationRegistry.sol";
import {MockUSDC} from "./mocks/MockUSDC.sol";

/// @title BaseTest — deploys and wires the full marketplace stack the way the
///        §8.3 deploy sequence does, but in-memory. Subclasses inherit the
///        deployed handles and helpers.
/// @dev The test contract is the `admin` (proxy owner) so it can call the
///      one-shot wiring setters directly without pranking.
abstract contract BaseTest is Test {
    IdentityRegistry internal identity;
    ReputationRegistry internal reputation;
    ValidationRegistry internal validation;
    ListingRegistry internal listings;
    LicenseToken internal license;
    Marketplace internal market;
    EvalAttestationRegistry internal attest;
    MockUSDC internal usdc;

    address internal admin = address(this);
    address internal feeRecipient = makeAddr("feeRecipient");

    uint16 internal constant INITIAL_FEE_BPS = 500; // 5%

    function setUp() public virtual {
        // 1-3. Immutable ERC-8004 registries (no proxy). ReputationRegistry
        // takes an admin (registrar) for the §3.6 license-gate wiring.
        identity = new IdentityRegistry();
        reputation = new ReputationRegistry(admin);
        validation = new ValidationRegistry();

        usdc = new MockUSDC();

        // 5. LicenseToken proxy (minter set empty).
        license = LicenseToken(
            _proxy(
                address(new LicenseToken()),
                abi.encodeCall(LicenseToken.initialize, (admin, "https://viewer.example/{id}"))
            )
        );

        // 6. ListingRegistry proxy (reads IdentityRegistry).
        listings = ListingRegistry(
            _proxy(
                address(new ListingRegistry()), abi.encodeCall(ListingRegistry.initialize, (admin, address(identity)))
            )
        );

        // 7. EvalAttestationRegistry proxy.
        attest = EvalAttestationRegistry(
            _proxy(address(new EvalAttestationRegistry()), abi.encodeCall(EvalAttestationRegistry.initialize, (admin)))
        );

        // 8. Marketplace proxy (reads ListingRegistry, calls LicenseToken).
        market = Marketplace(
            _proxy(
                address(new Marketplace()),
                abi.encodeCall(
                    Marketplace.initialize,
                    (admin, address(listings), address(license), address(usdc), feeRecipient, INITIAL_FEE_BPS)
                )
            )
        );

        // Atomic wiring step from §8.3: authorize Marketplace as the minter,
        // wire ListingRegistry <-> Marketplace and LicenseToken -> ListingRegistry.
        license.setAuthorized(address(market), true);
        license.setListingRegistry(address(listings));
        listings.setMarketplace(address(market));

        // Wire the LicenseToken into the ReputationRegistry so the §3.6
        // license gate can read `balanceOf(client, listingId)`.
        reputation.setLicenseToken(address(license));

        // Finding 1: wire the ReputationRegistry <-> ListingRegistry so the
        // per-agent feedback gate is set ATOMICALLY at listing creation. The
        // ListingRegistry is authorized as the ReputationRegistry's listing
        // registrar, and the ListingRegistry holds a reference to call
        // `setListingForAgent`. After this, `createListing` auto-gates the
        // agent — no manual `setListingForAgent` is needed.
        reputation.setListingRegistrar(address(listings));
        listings.setReputationRegistry(address(reputation));
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    function _proxy(address impl, bytes memory data) internal returns (address) {
        return address(new ERC1967Proxy(impl, data));
    }

    /// @dev Mint a lineage NFT to `owner`, returning its token id.
    function _mintLineage(address owner) internal returns (uint256 id) {
        vm.prank(owner);
        id = identity.register("ipfs://lineage-manifest");
    }

    /// @dev Create a Tier-0 listing under `agentNftId` as `seller`.
    function _createListing(address seller, uint256 agentNftId, uint96 price, bool transferable)
        internal
        returns (uint256 listingId)
    {
        vm.prank(seller);
        listingId =
            listings.createListing(agentNftId, keccak256("variant-bundle"), "ipfs://variant", 0, price, transferable);
    }

    /// @dev Fund `who` with `amount` mock-USDC and approve the Marketplace.
    function _fundAndApprove(address who, uint256 amount) internal {
        usdc.mint(who, amount);
        vm.prank(who);
        usdc.approve(address(market), amount);
    }
}

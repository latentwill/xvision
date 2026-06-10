// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Test} from "forge-std/Test.sol";
import {MockUSDC3009} from "../../src/test/MockUSDC3009.sol";

/// @notice Unit tests for the EIP-3009-capable testnet USDC mock: real
///         signature verification, nonce replay protection, validity window,
///         cancelAuthorization, receiveWithAuthorization, and the faucet cap.
contract MockUSDC3009Test is Test {
    MockUSDC3009 token;

    address payer;
    uint256 payerKey;
    address payee = makeAddr("payee");
    address relayer = makeAddr("relayer");

    function setUp() public {
        token = new MockUSDC3009();
        (payer, payerKey) = makeAddrAndKey("payer");
    }

    // -----------------------------------------------------------------------
    // Signing helpers
    // -----------------------------------------------------------------------

    function _signTransfer(
        uint256 key,
        address from,
        address to,
        uint256 value,
        uint256 validAfter,
        uint256 validBefore,
        bytes32 nonce
    ) internal view returns (uint8 v, bytes32 r, bytes32 s) {
        bytes32 structHash = keccak256(
            abi.encode(token.TRANSFER_WITH_AUTHORIZATION_TYPEHASH(), from, to, value, validAfter, validBefore, nonce)
        );
        (v, r, s) = vm.sign(key, keccak256(abi.encodePacked("\x19\x01", token.DOMAIN_SEPARATOR(), structHash)));
    }

    function _signReceive(
        uint256 key,
        address from,
        address to,
        uint256 value,
        uint256 validAfter,
        uint256 validBefore,
        bytes32 nonce
    ) internal view returns (uint8 v, bytes32 r, bytes32 s) {
        bytes32 structHash = keccak256(
            abi.encode(token.RECEIVE_WITH_AUTHORIZATION_TYPEHASH(), from, to, value, validAfter, validBefore, nonce)
        );
        (v, r, s) = vm.sign(key, keccak256(abi.encodePacked("\x19\x01", token.DOMAIN_SEPARATOR(), structHash)));
    }

    function _signCancel(uint256 key, address authorizer, bytes32 nonce)
        internal
        view
        returns (uint8 v, bytes32 r, bytes32 s)
    {
        bytes32 structHash = keccak256(abi.encode(token.CANCEL_AUTHORIZATION_TYPEHASH(), authorizer, nonce));
        (v, r, s) = vm.sign(key, keccak256(abi.encodePacked("\x19\x01", token.DOMAIN_SEPARATOR(), structHash)));
    }

    // -----------------------------------------------------------------------
    // Metadata / faucet
    // -----------------------------------------------------------------------

    function test_metadata() public view {
        assertEq(token.name(), "USD Coin (xvn test)");
        assertEq(token.symbol(), "USDC");
        assertEq(token.decimals(), 6);
    }

    function test_faucet_mintsToCaller() public {
        vm.prank(payer);
        token.faucet(5_000e6);
        assertEq(token.balanceOf(payer), 5_000e6);
        assertEq(token.totalSupply(), 5_000e6);
    }

    function test_faucet_capEnforced() public {
        vm.prank(payer);
        vm.expectRevert("MockUSDC3009: faucet cap exceeded");
        token.faucet(10_000e6 + 1);

        // exactly at cap is fine
        vm.prank(payer);
        token.faucet(10_000e6);
        assertEq(token.balanceOf(payer), 10_000e6);
    }

    // -----------------------------------------------------------------------
    // transferWithAuthorization
    // -----------------------------------------------------------------------

    function test_transferWithAuthorization_happyPath() public {
        vm.prank(payer);
        token.faucet(100e6);

        bytes32 nonce = keccak256("n1");
        (uint8 v, bytes32 r, bytes32 s) = _signTransfer(payerKey, payer, payee, 60e6, 0, type(uint256).max, nonce);

        // any relayer can submit
        vm.prank(relayer);
        token.transferWithAuthorization(payer, payee, 60e6, 0, type(uint256).max, nonce, v, r, s);

        assertEq(token.balanceOf(payee), 60e6);
        assertEq(token.balanceOf(payer), 40e6);
        assertTrue(token.authorizationState(payer, nonce));
    }

    function test_transferWithAuthorization_replayRejected() public {
        vm.prank(payer);
        token.faucet(100e6);

        bytes32 nonce = keccak256("n2");
        (uint8 v, bytes32 r, bytes32 s) = _signTransfer(payerKey, payer, payee, 10e6, 0, type(uint256).max, nonce);

        token.transferWithAuthorization(payer, payee, 10e6, 0, type(uint256).max, nonce, v, r, s);

        vm.expectRevert("MockUSDC3009: authorization used or canceled");
        token.transferWithAuthorization(payer, payee, 10e6, 0, type(uint256).max, nonce, v, r, s);
    }

    function test_transferWithAuthorization_badSignatureRejected() public {
        vm.prank(payer);
        token.faucet(100e6);

        bytes32 nonce = keccak256("n3");
        (, uint256 mallorKey) = makeAddrAndKey("mallory");
        (uint8 v, bytes32 r, bytes32 s) = _signTransfer(mallorKey, payer, payee, 10e6, 0, type(uint256).max, nonce);

        vm.expectRevert("MockUSDC3009: invalid signature");
        token.transferWithAuthorization(payer, payee, 10e6, 0, type(uint256).max, nonce, v, r, s);
    }

    function test_transferWithAuthorization_tamperedParamsRejected() public {
        vm.prank(payer);
        token.faucet(100e6);

        bytes32 nonce = keccak256("n4");
        (uint8 v, bytes32 r, bytes32 s) = _signTransfer(payerKey, payer, payee, 10e6, 0, type(uint256).max, nonce);

        // attacker bumps the value — digest no longer matches the signature
        vm.expectRevert("MockUSDC3009: invalid signature");
        token.transferWithAuthorization(payer, payee, 99e6, 0, type(uint256).max, nonce, v, r, s);
    }

    function test_transferWithAuthorization_windowEnforced() public {
        vm.prank(payer);
        token.faucet(100e6);
        vm.warp(1_000_000);

        // not yet valid
        bytes32 n5 = keccak256("n5");
        (uint8 v, bytes32 r, bytes32 s) =
            _signTransfer(payerKey, payer, payee, 10e6, block.timestamp + 100, type(uint256).max, n5);
        vm.expectRevert("MockUSDC3009: authorization not yet valid");
        token.transferWithAuthorization(payer, payee, 10e6, block.timestamp + 100, type(uint256).max, n5, v, r, s);

        // expired
        bytes32 n6 = keccak256("n6");
        (v, r, s) = _signTransfer(payerKey, payer, payee, 10e6, 0, block.timestamp - 1, n6);
        vm.expectRevert("MockUSDC3009: authorization expired");
        token.transferWithAuthorization(payer, payee, 10e6, 0, block.timestamp - 1, n6, v, r, s);
    }

    // -----------------------------------------------------------------------
    // receiveWithAuthorization
    // -----------------------------------------------------------------------

    function test_receiveWithAuthorization_payeeOnly() public {
        vm.prank(payer);
        token.faucet(100e6);

        bytes32 nonce = keccak256("n7");
        (uint8 v, bytes32 r, bytes32 s) = _signReceive(payerKey, payer, payee, 25e6, 0, type(uint256).max, nonce);

        // a third party cannot submit a receive authorization
        vm.prank(relayer);
        vm.expectRevert("MockUSDC3009: caller must be the payee");
        token.receiveWithAuthorization(payer, payee, 25e6, 0, type(uint256).max, nonce, v, r, s);

        // the payee can
        vm.prank(payee);
        token.receiveWithAuthorization(payer, payee, 25e6, 0, type(uint256).max, nonce, v, r, s);
        assertEq(token.balanceOf(payee), 25e6);
    }

    // -----------------------------------------------------------------------
    // cancelAuthorization
    // -----------------------------------------------------------------------

    function test_cancelAuthorization_voidsNonce() public {
        vm.prank(payer);
        token.faucet(100e6);

        bytes32 nonce = keccak256("n8");
        (uint8 tv, bytes32 tr, bytes32 ts) = _signTransfer(payerKey, payer, payee, 10e6, 0, type(uint256).max, nonce);
        (uint8 cv, bytes32 cr, bytes32 cs) = _signCancel(payerKey, payer, nonce);

        token.cancelAuthorization(payer, nonce, cv, cr, cs);
        assertTrue(token.authorizationState(payer, nonce));

        vm.expectRevert("MockUSDC3009: authorization used or canceled");
        token.transferWithAuthorization(payer, payee, 10e6, 0, type(uint256).max, nonce, tv, tr, ts);
    }

    function test_cancelAuthorization_requiresAuthorizerSignature() public {
        bytes32 nonce = keccak256("n9");
        (, uint256 mallorKey) = makeAddrAndKey("mallory");
        (uint8 v, bytes32 r, bytes32 s) = _signCancel(mallorKey, payer, nonce);

        vm.expectRevert("MockUSDC3009: invalid signature");
        token.cancelAuthorization(payer, nonce, v, r, s);
    }

    // -----------------------------------------------------------------------
    // Plain ERC-20 surface (Marketplace `buy` path uses transferFrom)
    // -----------------------------------------------------------------------

    function test_erc20_approveTransferFrom() public {
        vm.prank(payer);
        token.faucet(100e6);

        vm.prank(payer);
        token.approve(relayer, 50e6);

        vm.prank(relayer);
        token.transferFrom(payer, payee, 50e6);

        assertEq(token.balanceOf(payee), 50e6);
        assertEq(token.allowance(payer, relayer), 0);
    }
}

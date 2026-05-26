// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Test} from "forge-std/Test.sol";
import {XvnDeployer} from "../../src/XvnDeployer.sol";

/// @dev Trivial target with a constructor arg, to prove init-code + args deploy.
contract DeployTarget {
    uint256 public x;

    constructor(uint256 x_) {
        x = x_;
    }
}

contract XvnDeployerTest is Test {
    XvnDeployer deployer;

    function setUp() public {
        deployer = new XvnDeployer();
    }

    function _initCode(uint256 arg) internal pure returns (bytes memory) {
        return abi.encodePacked(type(DeployTarget).creationCode, abi.encode(arg));
    }

    function test_deploy_matchesComputedAddress() public {
        bytes32 salt = keccak256("xvn.Test.v1");
        bytes memory code = _initCode(42);

        address predicted = deployer.computeAddress(salt, keccak256(code));
        address deployed = deployer.deploy(salt, code);

        assertEq(deployed, predicted, "CREATE2 address must be predictable");
        assertEq(DeployTarget(deployed).x(), 42, "constructor arg applied");
    }

    function test_deploy_emitsDeployed() public {
        bytes32 salt = keccak256("xvn.Emit.v1");
        bytes memory code = _initCode(7);
        address predicted = deployer.computeAddress(salt, keccak256(code));

        vm.expectEmit(true, true, true, true, address(deployer));
        emit XvnDeployer.Deployed(predicted, salt, address(this));
        deployer.deploy(salt, code);
    }

    function test_deploy_revert_emptyBytecode() public {
        vm.expectRevert(XvnDeployer.EmptyBytecode.selector);
        deployer.deploy(keccak256("s"), "");
    }

    function test_deploy_revert_duplicateSaltAndCode() public {
        bytes32 salt = keccak256("xvn.Dup.v1");
        bytes memory code = _initCode(1);
        deployer.deploy(salt, code);

        // Same (salt, initcode) => same address => CREATE2 fails => returns 0.
        vm.expectRevert(XvnDeployer.DeploymentFailed.selector);
        deployer.deploy(salt, code);
    }

    function test_computeAddress_dependsOnSalt() public view {
        bytes32 h = keccak256(_initCode(1));
        assertTrue(
            deployer.computeAddress(keccak256("a"), h) != deployer.computeAddress(keccak256("b"), h)
        );
    }
}

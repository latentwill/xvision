// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title XvnDeployer — deterministic CREATE2 factory for the xvn contract set.
/// @notice Deploys any contract at an address that depends only on this
///         factory's address, the salt, and the init bytecode — never on the
///         deployer EOA's nonce. Combined with reusing the **nonce-0 EOA** on
///         every chain (so this factory lands at the same address everywhere),
///         this makes every xvn contract's address predictable and identical
///         across chains we later mirror to (surface spec §6.5).
///
/// @dev Salts are `keccak256("xvn.<contractName>.v1")` by convention; the
///      factory itself imposes no salt format. Deploy this factory FIRST, from
///      a freshly-funded EOA whose nonce is 0 (blockchain nav doc §3, Phase 3).
///      The factory is immutable and unprivileged — anyone can deploy through
///      it; determinism comes from (salt, bytecode), not from access control.
contract XvnDeployer {
    /// @notice Emitted on every successful deployment.
    event Deployed(address indexed deployed, bytes32 indexed salt, address indexed deployer);

    error DeploymentFailed();
    error EmptyBytecode();

    /// @notice Deploy `bytecode` via CREATE2 under `salt`.
    /// @param salt Deterministic salt, e.g. `keccak256("xvn.Marketplace.v1")`.
    /// @param bytecode Full init (creation) bytecode, including constructor args.
    /// @return deployed The address of the newly deployed contract.
    function deploy(bytes32 salt, bytes calldata bytecode) external returns (address deployed) {
        if (bytecode.length == 0) revert EmptyBytecode();

        bytes memory code = bytecode;
        assembly ("memory-safe") {
            deployed := create2(0, add(code, 0x20), mload(code), salt)
        }
        if (deployed == address(0)) revert DeploymentFailed();

        emit Deployed(deployed, salt, msg.sender);
    }

    /// @notice Predict the address `deploy(salt, bytecode)` would produce.
    /// @param salt The same salt that will be passed to `deploy`.
    /// @param bytecodeHash `keccak256(bytecode)` of the init code.
    /// @return The counterfactual deployment address.
    function computeAddress(bytes32 salt, bytes32 bytecodeHash) external view returns (address) {
        return address(
            uint160(
                uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, bytecodeHash)))
            )
        );
    }
}

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title MockUSDC — minimal 6-decimal ERC-20 with an EIP-3009 surface.
/// @dev Self-contained (no OZ ERC20 dependency) to keep the test mock obvious.
///      `transferWithAuthorization` here SKIPS EIP-712 signature verification —
///      real USDC.e verifies the (v,r,s) signature over the EIP-712 struct. The
///      mock keeps the parts the Marketplace relies on: a validity window and
///      single-use nonces, so the nonce-replay test (§9.1) is meaningful.
contract MockUSDC {
    string public name = "USD Coin (mock)";
    string public symbol = "USDC";
    uint8 public constant decimals = 6;
    uint256 public totalSupply;

    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    /// @notice EIP-3009 authorization nonce state: authorizer => nonce => used.
    mapping(address => mapping(bytes32 => bool)) public authorizationState;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
        totalSupply += amount;
        emit Transfer(address(0), to, amount);
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        _transfer(msg.sender, to, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        uint256 a = allowance[from][msg.sender];
        require(a >= amount, "MockUSDC: insufficient allowance");
        if (a != type(uint256).max) allowance[from][msg.sender] = a - amount;
        _transfer(from, to, amount);
        return true;
    }

    function _transfer(address from, address to, uint256 amount) internal {
        require(balanceOf[from] >= amount, "MockUSDC: insufficient balance");
        unchecked {
            balanceOf[from] -= amount;
            balanceOf[to] += amount;
        }
        emit Transfer(from, to, amount);
    }

    /// @notice EIP-3009 transferWithAuthorization (mock — no sig check).
    function transferWithAuthorization(
        address from,
        address to,
        uint256 value,
        uint256 validAfter,
        uint256 validBefore,
        bytes32 nonce,
        uint8, /* v */
        bytes32, /* r */
        bytes32 /* s */
    ) external {
        require(block.timestamp > validAfter, "MockUSDC: auth not yet valid");
        require(block.timestamp < validBefore, "MockUSDC: auth expired");
        require(!authorizationState[from][nonce], "MockUSDC: auth nonce used");
        authorizationState[from][nonce] = true;
        _transfer(from, to, value);
    }
}

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

/// @title MockUSDC3009 — testnet USDC stand-in with a REAL EIP-3009 surface.
/// @notice Deployed to Mantle Sepolia (chain 5003) so the x402
///         `Marketplace.buyWithAuthorization` path can be exercised end-to-end.
///         No EIP-3009-capable USDC exists on Mantle Sepolia (the community
///         mock at 0xAcab8129E2cE587fD203FD770ec9ECAFA2C88080 has no
///         signature-transfer support); Mantle MAINNET bridged USDC
///         (0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9) fully supports
///         EIP-3009, so this contract is testnet-only.
///
/// @dev Unlike `test/mocks/MockUSDC.sol` (which skips signature checks), this
///      contract verifies the full EIP-712 signature over the canonical
///      EIP-3009 structs, enforces the validAfter/validBefore window, and
///      tracks single-use nonces — matching Circle's FiatTokenV2 semantics for
///      `transferWithAuthorization`, `receiveWithAuthorization`, and
///      `cancelAuthorization`. Anyone can mint via the per-call-capped
///      `faucet`. TESTNET ONLY — do not deploy to mainnet.
contract MockUSDC3009 {
    // -----------------------------------------------------------------------
    // ERC-20 metadata / state
    // -----------------------------------------------------------------------

    string public constant name = "USD Coin (xvn test)";
    string public constant symbol = "USDC";
    uint8 public constant decimals = 6;
    uint256 public totalSupply;

    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    /// @notice EIP-3009 authorization nonce state: authorizer => nonce => used.
    mapping(address => mapping(bytes32 => bool)) public authorizationState;

    /// @notice Max amount a single `faucet` call can mint (10,000 USDC).
    uint256 public constant FAUCET_CAP = 10_000e6;

    // -----------------------------------------------------------------------
    // EIP-712 / EIP-3009 constants
    // -----------------------------------------------------------------------

    /// @dev Canonical EIP-712 / EIP-3009 typehashes (match Circle FiatTokenV2).
    bytes32 private constant _EIP712_DOMAIN_TYPEHASH =
        keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
    bytes32 public constant TRANSFER_WITH_AUTHORIZATION_TYPEHASH = keccak256(
        "TransferWithAuthorization(address from,address to,uint256 value,uint256 validAfter,uint256 validBefore,bytes32 nonce)"
    );
    bytes32 public constant RECEIVE_WITH_AUTHORIZATION_TYPEHASH = keccak256(
        "ReceiveWithAuthorization(address from,address to,uint256 value,uint256 validAfter,uint256 validBefore,bytes32 nonce)"
    );
    bytes32 public constant CANCEL_AUTHORIZATION_TYPEHASH =
        keccak256("CancelAuthorization(address authorizer,bytes32 nonce)");

    /// @dev Cached domain separator + the chain id it was built for. Rebuilt on
    ///      the fly if the chain forks (matches OZ EIP712 behaviour).
    bytes32 private immutable _cachedDomainSeparator;
    uint256 private immutable _cachedChainId;

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event AuthorizationUsed(address indexed authorizer, bytes32 indexed nonce);
    event AuthorizationCanceled(address indexed authorizer, bytes32 indexed nonce);

    constructor() {
        _cachedChainId = block.chainid;
        _cachedDomainSeparator = _buildDomainSeparator();
    }

    // -----------------------------------------------------------------------
    // Faucet
    // -----------------------------------------------------------------------

    /// @notice Open faucet: mints `amount` (≤ 10,000 USDC per call) to caller.
    function faucet(uint256 amount) external {
        require(amount <= FAUCET_CAP, "MockUSDC3009: faucet cap exceeded");
        balanceOf[msg.sender] += amount;
        totalSupply += amount;
        emit Transfer(address(0), msg.sender, amount);
    }

    // -----------------------------------------------------------------------
    // ERC-20
    // -----------------------------------------------------------------------

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
        require(a >= amount, "MockUSDC3009: insufficient allowance");
        if (a != type(uint256).max) allowance[from][msg.sender] = a - amount;
        _transfer(from, to, amount);
        return true;
    }

    function _transfer(address from, address to, uint256 amount) internal {
        require(to != address(0), "MockUSDC3009: transfer to zero");
        require(balanceOf[from] >= amount, "MockUSDC3009: insufficient balance");
        unchecked {
            balanceOf[from] -= amount;
            balanceOf[to] += amount;
        }
        emit Transfer(from, to, amount);
    }

    // -----------------------------------------------------------------------
    // EIP-3009
    // -----------------------------------------------------------------------

    /// @notice EIP-3009 `transferWithAuthorization` with full EIP-712
    ///         signature verification, validity window, and single-use nonce.
    function transferWithAuthorization(
        address from,
        address to,
        uint256 value,
        uint256 validAfter,
        uint256 validBefore,
        bytes32 nonce,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external {
        _requireValidAuthorization(from, nonce, validAfter, validBefore);
        _requireValidSignature(
            from,
            keccak256(
                abi.encode(TRANSFER_WITH_AUTHORIZATION_TYPEHASH, from, to, value, validAfter, validBefore, nonce)
            ),
            v,
            r,
            s
        );
        _markAuthorizationAsUsed(from, nonce);
        _transfer(from, to, value);
    }

    /// @notice EIP-3009 `receiveWithAuthorization`: same as transfer, but the
    ///         caller must be the payee — prevents front-running submission.
    function receiveWithAuthorization(
        address from,
        address to,
        uint256 value,
        uint256 validAfter,
        uint256 validBefore,
        bytes32 nonce,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external {
        require(msg.sender == to, "MockUSDC3009: caller must be the payee");
        _requireValidAuthorization(from, nonce, validAfter, validBefore);
        _requireValidSignature(
            from,
            keccak256(abi.encode(RECEIVE_WITH_AUTHORIZATION_TYPEHASH, from, to, value, validAfter, validBefore, nonce)),
            v,
            r,
            s
        );
        _markAuthorizationAsUsed(from, nonce);
        _transfer(from, to, value);
    }

    /// @notice EIP-3009 `cancelAuthorization`: voids an unused nonce.
    function cancelAuthorization(address authorizer, bytes32 nonce, uint8 v, bytes32 r, bytes32 s) external {
        require(!authorizationState[authorizer][nonce], "MockUSDC3009: auth already used");
        _requireValidSignature(
            authorizer, keccak256(abi.encode(CANCEL_AUTHORIZATION_TYPEHASH, authorizer, nonce)), v, r, s
        );
        authorizationState[authorizer][nonce] = true;
        emit AuthorizationCanceled(authorizer, nonce);
    }

    // -----------------------------------------------------------------------
    // EIP-712 plumbing
    // -----------------------------------------------------------------------

    function DOMAIN_SEPARATOR() public view returns (bytes32) {
        return block.chainid == _cachedChainId ? _cachedDomainSeparator : _buildDomainSeparator();
    }

    function _buildDomainSeparator() private view returns (bytes32) {
        return keccak256(
            abi.encode(
                _EIP712_DOMAIN_TYPEHASH, keccak256(bytes(name)), keccak256(bytes("1")), block.chainid, address(this)
            )
        );
    }

    function _requireValidAuthorization(address authorizer, bytes32 nonce, uint256 validAfter, uint256 validBefore)
        private
        view
    {
        require(block.timestamp > validAfter, "MockUSDC3009: authorization not yet valid");
        require(block.timestamp < validBefore, "MockUSDC3009: authorization expired");
        require(!authorizationState[authorizer][nonce], "MockUSDC3009: authorization used or canceled");
    }

    function _requireValidSignature(address signer, bytes32 structHash, uint8 v, bytes32 r, bytes32 s) private view {
        bytes32 digest = keccak256(abi.encodePacked("\x19\x01", DOMAIN_SEPARATOR(), structHash));
        address recovered = ecrecover(digest, v, r, s);
        require(recovered != address(0) && recovered == signer, "MockUSDC3009: invalid signature");
    }

    function _markAuthorizationAsUsed(address authorizer, bytes32 nonce) private {
        authorizationState[authorizer][nonce] = true;
        emit AuthorizationUsed(authorizer, nonce);
    }
}

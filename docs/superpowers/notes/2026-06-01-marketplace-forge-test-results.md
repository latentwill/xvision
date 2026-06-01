# Marketplace Forge test results — 2026-06-01

First successful compile+test run of the marketplace contracts tree.
Contracts were authored without a local Foundry toolchain (see
`2026-05-27-marketplace-contracts-phase-3-5-status.md`); this session
completed the first full compile and green test run.

## Environment

- Forge: 1.7.1-Homebrew (commit `4072e48`)
- Solc: 0.8.24
- OZ: openzeppelin-contracts v5.0.2, openzeppelin-contracts-upgradeable v5.0.2
- forge-std: v1.9.7
- EVM target: `shanghai` (Mantle Sepolia 5003 + Mantle mainnet 5000 compatible)

## Setup

`contracts/lib/` is gitignored (not committed). Dependencies must be
cloned locally before building:

```bash
cd contracts
git clone --depth 1 --branch v1.9.7 https://github.com/foundry-rs/forge-std.git lib/forge-std
git clone --depth 1 --branch v5.0.2 https://github.com/OpenZeppelin/openzeppelin-contracts.git lib/openzeppelin-contracts
git clone --depth 1 --branch v5.0.2 https://github.com/OpenZeppelin/openzeppelin-contracts-upgradeable.git lib/openzeppelin-contracts-upgradeable
```

`forge install` is NOT usable here — `contracts/.gitignore` excludes
`/lib/`, which blocks `forge install`'s git-submodule registration.
Use the manual `git clone` commands above. Do NOT add lib/* entries
to root `.gitmodules` (no gitlink is tracked, resulting in an orphaned
submodule reference).

## Build result

```
forge build
Compiling 89 files with Solc 0.8.24
Solc 0.8.24 finished in 4.02s
Compiler run successful!
```

Two `warning[block-timestamp]` lints in `test/mocks/MockUSDC.sol` (lines
70–71). Expected for a mock EIP-3009 token; not a source contract concern.

## Test results

```
Ran 9 test suites: 58 tests passed, 0 failed, 1 skipped (59 total)
```

| Suite | Tests | Result |
|---|---|---|
| `test/unit/Marketplace.t.sol` | 17 | PASS |
| `test/unit/ListingRegistry.t.sol` | 11 | PASS |
| `test/unit/Registries.t.sol` | 8 | PASS |
| `test/unit/LicenseToken.t.sol` | 7 | PASS |
| `test/unit/XvnDeployer.t.sol` | 5 | PASS |
| `test/unit/EvalAttestationRegistry.t.sol` | 4 | PASS |
| `test/integration/SaleFlow.t.sol` | 4 | PASS |
| `test/integration/Upgrade.t.sol` | 2 | PASS |
| `test/fork/Upgrade.fork.t.sol` | 0 / 1 skipped | SKIP (no live RPC) |

Total wall-clock: ~15 ms. CPU time: ~67 ms.

## Deployment gas (implementation contracts, `forge test --gas-report`)

| Contract | Deploy gas | Bytecode size |
|---|---|---|
| `EvalAttestationRegistry` | 934,634 | 4,206 bytes |
| `LicenseToken` | 1,854,229 | 8,457 bytes |
| `ListingRegistry` | 1,459,462 | 6,632 bytes |
| `Marketplace` | 1,551,511 | 6,959 bytes |
| `XvnDeployer` | 184,407 | 635 bytes |
| `IdentityRegistry` | 1,172,689 | 5,434 bytes |
| `ReputationRegistry` | 625,392 | 2,675 bytes |
| `ValidationRegistry` | 442,045 | 1,827 bytes |
| `ERC1967Proxy` (OZ, per instance) | 273,214 | 1,328 bytes |

All four UUPS contracts (`ListingRegistry`, `Marketplace`, `LicenseToken`,
`EvalAttestationRegistry`) are well under the 24.576 KB contract size limit.

## Smoke deploy (Mantle Sepolia, chain 5003)

**Skipped.** No `MANTLE_SEPOLIA_RPC_URL`, `MANTLE_RPC_URL`, or
`MANTLESCAN_API_KEY` in the environment. The fork test
(`test_fork_upgradePreservesListingsAndLicenses`) also auto-skips for the
same reason — it is a documented harness pending Phase 3/5 deploy.

To run when credentials are available:

```bash
export MANTLE_SEPOLIA_RPC_URL=<rpc>
export PRIVATE_KEY=<deployer key>
forge script script/DeployTestnet.s.sol \
  --rpc-url mantle_sepolia \
  --broadcast \
  --verify
```

## Caveats from original status doc (still open)

1. **§4.5 spec deviation** — revoked-listing EIP-3009 settlement: impl
   reverts the whole tx (nonce untouched), spec prose implies nonce consumed
   but USDC not moved. Behaviour is documented in
   `test_revokedBetween402AndSettlement_revertsCleanly` and is the safer
   choice; spec prose needs updating.

2. **`payerKind` is a v1 placeholder** — mirrors `purchasePath`; §3.2
   deferred exact derivation to a future phase. Field is present in the
   `Sold` event so the indexer can be refined without an ABI change.

## Next steps

- Wire `forge build` into CI (GHA lane) with ABI pin under
  `crates/xvision-identity/abi/v1/` (§8.5).
- Implement `Erc8004MantleDriver` against the alloy bindings.
- Deploy to Mantle Sepolia (Phase 3/5 deploy, separate gated session).
- Confirm PUSH0 acceptance on chain 5003 (evm_version = "shanghai" smoke test).

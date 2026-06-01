# xvn marketplace contracts

Foundry project for the xvision on-chain surface — the ERC-8004 registries
(Phase 3) and the marketplace contracts (Phase 5) from
[`docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`](../docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md),
sequenced by
[`docs/superpowers/plans/2026-05-26-blockchain-plan-navigation.md`](../docs/superpowers/plans/2026-05-26-blockchain-plan-navigation.md).

> **⚠️ Status: written for review/research — NOT YET COMPILED.** These contracts
> and tests were authored against the spec without a local Foundry toolchain.
> Before trusting anything here, run `forge build` and `forge test` (below) and
> fix whatever the compiler flags. Nothing has been deployed; the deploy scripts
> are documentation, not runbooks for this slice.

## Layout

```
contracts/
├── foundry.toml
├── src/
│   ├── registries/                 # Phase 3 — immutable ERC-8004 (no proxy)
│   │   ├── IdentityRegistry.sol     #   lineage NFT; agent #0 = platform
│   │   ├── ReputationRegistry.sol   #   per-cycle feedback
│   │   └── ValidationRegistry.sol   #   per-trade proofs + attester receipts (NEW)
│   ├── XvnDeployer.sol             # Phase 3 — CREATE2 factory (deterministic addrs)
│   ├── ListingRegistry.sol         # Phase 5 — UUPS, listing CRUD
│   ├── Marketplace.sol             # Phase 5 — UUPS, buy + x402 + fee split
│   ├── LicenseToken.sol            # Phase 5 — UUPS ERC-1155, soulbound default
│   ├── EvalAttestationRegistry.sol # Phase 5 — UUPS, eval attestations
│   ├── interfaces/                 # I*.sol — shared surfaces incl. IERC3009
│   └── libraries/Splits.sol        # fee-split math
├── script/                         # deploy scripts (review only this slice)
└── test/{unit,integration,fork,mocks}/
```

The four marketplace contracts are UUPS proxies with the **operator EOA as admin**
for V2 testnet (no timelock/multisig until V4 — nav doc §5). The three ERC-8004
registries are immutable (no proxy). See the surface spec for the full rationale.

## Dependencies

Dependencies are git submodules under `lib/` (gitignored, not committed). Install
them before building:

```bash
cd contracts
forge install foundry-rs/forge-std@v1.9.7
forge install OpenZeppelin/openzeppelin-contracts@v5.0.2
forge install OpenZeppelin/openzeppelin-contracts-upgradeable@v5.0.2
```

Remappings are declared in `foundry.toml`. The code targets **OpenZeppelin v5**
(`Ownable(initialOwner)`, ERC-1155 `_update` hook, namespaced upgradeable
storage). The exact tags above are intentional: newer OZ v5 releases may use
Cancun-only builtins such as `mcopy`. v5.0.2 is pre-`mcopy`, so it compiles
cleanly under the `shanghai` EVM target this tree uses.

## Build & test

```bash
forge build
forge test                      # unit + integration
forge test --match-path 'test/fork/*'   # fork tests (need MANTLE_SEPOLIA_RPC_URL)
forge coverage                  # coverage report
```

### Fork tests

`test/fork/Upgrade.fork.t.sol` self-skips unless `MANTLE_SEPOLIA_RPC_URL` is set
**and** contracts are deployed (addresses in `config/mantle-sepolia.toml`). Until
Phase 3/5 deploy there is no live state to fork, so it is a documented harness.
The locally-runnable upgrade-safety test is `test/integration/Upgrade.t.sol`.

### EVM target

`evm_version = "shanghai"` in `foundry.toml`. Mantle mainnet (5000) and Sepolia
(5003) have supported Shanghai (incl. PUSH0) for well over a year, so `shanghai`
is safe and reclaims PUSH0's gas savings on constant loads. Not `cancun`: this
tree uses no transient storage / `mcopy` (and OZ is pinned to v5.0.2, pre-`mcopy`).
Confirm PUSH0 acceptance with a smoke deploy to chain 5003 before relying on it
for mainnet.

### Upgrade safety

`extra_output = ["storageLayout"]` is on so CI can diff storage layouts across
implementation versions (surface spec §7.5). Every upgradeable contract reserves
a `uint256[..] __gap`. New state in v2+ goes into the gap, never above existing
vars. Wire `forge inspect <Contract> storageLayout` into CI before any upgrade.

## Deploy ordering (review only)

`script/DeployTestnet.s.sol` implements the §8.3 sequence via CREATE2 so every
address is deterministic and identical on a future mainnet deploy (same nonce-0
EOA → same factory → same salts). `script/RegisterPlatformAgent.s.sol` mints xvn
as ERC-8004 agent #0. `DeployMainnet.s.sol` / `UpgradeTimelock.s.sol` are V4-gated
stubs that revert.

Deploys run on the local build host or CI — **never on a deploy VPS** (project
no-Cargo / no-build-on-host rule also covers Foundry).

## Rust integration

`crates/xvision-identity/src/contracts.rs` holds the `alloy::sol!` bindings for
every contract here. `crates/xvision-marketplace` wraps them behind the
`AnchorDriver` port (with a functional `MockDriver` and a stubbed
`Erc8004MantleDriver`). Both are excluded from the workspace `default-members`,
so the hot dev loop never compiles the heavy `alloy` tree.

## Out of scope for this slice

Generative art / `tokenURI` encoding (Phase 4 locks it), the x402 resource-server
host, sealed-bundle (Tier B) delivery, the on-chain refund/resale paths, and EAS
migration are all deferred — see surface spec §10 and nav-doc §4.

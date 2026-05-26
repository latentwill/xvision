# Marketplace contracts — Phase 3 + 5 status (2026-05-27)

Status handoff for the smart-contract slice of the marketplace program. Scope
this session: **write the contracts for review/research — no deployment, no
local compile** (operator directive). Branch `feat/marketplace-contracts` off
`origin/main`.

## What landed

A new `contracts/` Foundry tree (surface-spec §8.1) plus Rust wiring stubs.

### Phase 3 — ERC-8004 registries + CREATE2 factory (`contracts/src/registries/`, `XvnDeployer.sol`)
- `IdentityRegistry` — ERC-721 lineage NFT (one NFT per lineage, §3.1.1); first
  mint is agent #0; emits `AgentRegistered` for indexers.
- `ReputationRegistry` — per-cycle feedback (matches the ABI already bound in
  `xvision-identity/src/client.rs`); adds `FeedbackPosted` event.
- `ValidationRegistry` — NEW (§8.3 step 3); per-trade proofs + attester receipts.
- `XvnDeployer` — CREATE2 factory; `deploy` + `computeAddress`.

### Phase 5 — marketplace contracts (UUPS, operator-EOA admin)
- `ListingRegistry` — listing CRUD; lineage-ownership check; per-listing fee
  snapshot; content-rotation-only updates.
- `Marketplace` — direct `buy` + x402 `buyWithAuthorization` (EIP-3009);
  95/5 split via `Splits`; `nonReentrant`; pausable; `Sold` carries
  `payerKind`/`purchasePath`.
- `LicenseToken` — ERC-1155, `tokenId == listingId`, soulbound by default with
  per-listing transferable opt-in read live from `ListingRegistry`.
- `EvalAttestationRegistry` — publish-time + third-party eval attestations.
- `interfaces/` (incl. `IERC3009`), `libraries/Splits.sol`.

### Scripts (review only — not run)
`DeployTestnet.s.sol` (deterministic §8.3 sequence), `RegisterPlatformAgent.s.sol`
(agent #0), `DeployMainnet.s.sol` + `UpgradeTimelock.s.sol` (V4-gated stubs).

### Tests (Foundry — uncompiled)
Unit (per contract), integration (`SaleFlow`, `Upgrade`), fork stub
(env-gated), mocks (`MockUSDC` w/ EIP-3009, `ReentrantReceiver`, `MarketplaceV2`).
Covers §9.1/§9.2: fee split, snapshot immutability, soulbound enforcement,
reentrancy, EIP-3009 nonce replay, revoked-listing settlement, free listings,
max price, UUPS upgrade state preservation.

### Rust
- `xvision-identity/src/contracts.rs` — `alloy::sol!` bindings for all five new
  contracts + `MarketplaceAddresses` (returns `None` per chain, like
  `RegistryAddresses`).
- `crates/xvision-marketplace/` — skeleton: `AnchorDriver` port, functional
  in-memory `MockDriver`, stubbed `Erc8004MantleDriver`, `IpfsStore` +
  `PinataDriver` stub, subgraph `schema.graphql` (§6.3). Excluded from
  `default-members`.
- `config/mantle-sepolia.toml` + `config/mantle.toml` (§8.4, placeholder addrs).

## Hard caveats for the reviewer

1. **Nothing was compiled.** No Foundry toolchain locally this session. First
   action on pickup: `cd contracts && forge install …` (see README) then
   `forge build && forge test`, and fix whatever the compiler flags. The
   highest-risk areas are OZ-v5 specifics (the `_update` soulbound hook, the
   `balanceOf` override collision, `upgradeToAndCall`) and the qualified-event
   `emit` in tests (needs solc ≥0.8.22; we target 0.8.24).
2. **Spec deviation flagged:** §4.5 says a listing revoked between 402-issue and
   settlement leaves the EIP-3009 nonce "consumed but USDC not moved." This impl
   checks `revoked` *before* pulling funds (checks-effects), so the whole tx
   reverts and the nonce is untouched — safer, but it contradicts the prose.
   Confirm the intended behavior. (Test documents the actual behavior.)
3. **`payerKind` derivation is a v1 placeholder** (mirrors `purchasePath`).
   §3.2 deferred the exact derivation to "Phase 1/5"; kept as a distinct event
   field so the indexer can refine without an ABI change. Needs a decision.
4. **`evm_version = "paris"`** chosen conservatively for Mantle; bump after
   confirming opcode support on chain 5003.

## Next (not done here)
- Compile + green the suite; wire `forge build` into a CI lane (§8.5 ABI pin
  under `crates/xvision-identity/abi/v1/`).
- Implement `Erc8004MantleDriver` against the bindings (Phase 5 Rust).
- CLI verbs `xvn marketplace publish|buy|attest|list`, `xvn admin
  register-platform-agent` (surface spec §8.2).
- Then deploy to Mantle Sepolia (Phase 3/5 deploy) — separate, gated session.

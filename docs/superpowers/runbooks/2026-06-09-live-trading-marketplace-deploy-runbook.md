# Live Trading + Marketplace — Deployment & Post-Deployment Runbook

**Created:** 2026-06-09. **Status of code:** merged to `main` via PR #886 (squash `996f607`).
**What this covers:** every step a human must perform to take the merged code live on
**Mantle Sepolia (testnet)**, the post-deploy code-wiring that is gated on that deploy,
the end-to-end verification gate, and the consolidated deferred-items register.

> Source plans: `docs/superpowers/plans/2026-06-08-blockchain-implementation-synthesis.md`
> (§6 runbook, §9 status). Spec: `docs/superpowers/specs/2026-06-08-live-trading-marketplace-spec.md`.

---

## 0. What is already done vs. what remains

**Done & on `main` (PR #886):** Live Trading backend (per-run pause / close-on-cancel /
on-demand flatten), the `/live` cockpit (now a sidebar entry; paper/Alpaca only,
`VenueLabel::Live` OFF), the marketplace contracts (license-gated reputation,
auto-wired gate, x402 `recipient==auth.from`, UUPS `ReentrancyGuardUpgradeable`),
the Rust drivers (`Erc8004MantleDriver`, `PinataDriver` — implemented, not yet wired
to a runtime surface), the C6 attestation engine (pure verdict + 20-trade trigger +
gated bridge), and the C8 marketplace frontend (opt-in gated, fixture-backed).

**Remains:** everything in this runbook — it is all manual or deploy-gated.

**Hard rule (CLAUDE.md):** all Foundry builds + deploys run on the **local build host
or CI**, NEVER on the small VPS / Coolify deploy nodes (no `cargo`/`forge`/Docker-build
there). `VenueLabel::Live` (real money) stays OFF until V4.

---

## 1. Pre-deployment prerequisites

- [ ] **Local build host** with `forge` (1.7.1+), the Rust toolchain, and `pnpm` — NOT a deploy node.
- [ ] **`MANTLE_SEPOLIA_RPC_URL`** (chain id 5003) reachable from the build host.
- [ ] **`MANTLESCAN_API_KEY`** for contract verification.
- [ ] **Pinata account + JWT** (for `PinataDriver` IPFS pinning).
- [ ] **Subgraph host decision** — Goldsky / The Graph hosted / self-host (C2).
- [ ] **Validation Registry signer host decision** (C5) — trusted validator service.
- [ ] Confirm `contracts/` builds & tests green on the build host: `cd contracts && forge build && forge test` (expect 83 passing, 1 fork test skipped without an RPC).

### Decisions required before/at deploy
- [x] **AM3 agent granularity** — RESOLVED: agent = strategy = listing (1:1). Already in code + subgraph schema.
- [ ] **AM2** canonical gen-art renderer — Rust SVG (`xvision-identity/src/genart.rs`) vs the frontend canvas (`GenArtPlaceholder.tsx`) **diverge**; pick the canonical one before the mint flow is used.
- [ ] **A5** wallet provider — MetaMask is fine for testnet; revisit before mainnet.
- [ ] USDC.e on Mantle: confirm in step 2.3 below.

---

## 2. Deployment — manual testnet bring-up

Run from the local build host. Each step is gas-spending and/or irreversible where noted.

1. [ ] **Mint the nonce-0 EOA ("forever wallet").** Reused on every chain to keep CREATE2
   addresses deterministic. **Back up the key; this is a one-time, irreversible identity decision.**
2. [ ] **Fund the EOA on Mantle Sepolia** via faucet — pre-fund ~5× estimated footprint.
3. [ ] **Probe USDC.e EIP-3009 support on Mantle** (`transferWithAuthorization` + nonce + validity window).
   - Supported → the x402 `buyWithAuthorization` path is usable.
   - Not supported → fall back to approve+`buy` (or Permit2). The contract's positive-price
     x402 path assumes EIP-3009 with nonce+window enforcement (see audit L-3).
4. [ ] **Deploy the contracts:** run `contracts/script/DeployTestnet.s.sol` against
   `MANTLE_SEPOLIA_RPC_URL` (deploys `XvnDeployer` → `IdentityRegistry`,
   `ReputationRegistry`, `ValidationRegistry` → the 4 UUPS proxies `LicenseToken`,
   `ListingRegistry`, `Marketplace`, `EvalAttestationRegistry`; admin = operator EOA,
   `initialFeeBps = 1000`). **Gas-spending, irreversible.**
   - ⚠️ **Wiring order matters** (audit Low-1): the script wires `setLicenseToken`,
     `setReputationRegistry`/`setListingRegistrar` so the license gate is live at
     `createListing`. Keep those wiring calls together; if the registrar isn't wired,
     `createListing` will revert. Verify the script does all of: `LicenseToken.setListingRegistry`,
     `ReputationRegistry.setLicenseToken` + `setListingRegistrar`, `ListingRegistry.setMarketplace` + `setReputationRegistry`.
5. [ ] **Mint xvn as agent #0:** run `contracts/script/RegisterPlatformAgent.s.sol`.
6. [ ] **Record deployed addresses** into `config/mantle-sepolia.toml` and set the dashboard
   service env vars: `MANTLE_TESTNET_IDENTITY_REGISTRY`, `MANTLE_TESTNET_REPUTATION_REGISTRY`,
   and the marketplace addresses (`MarketplaceAddresses::mantle_testnet()` currently returns
   `None` until these are populated). Without these, all on-chain paths no-op by design.
7. [ ] **Verify contracts on Mantlescan** (`MANTLESCAN_API_KEY`) and **pin the verified ABIs**
   under `crates/xvision-identity/abi/v1/` (AM7). The `sol!` bindings in `contracts.rs` were
   written ahead of deploy — reconcile them against the verified ABIs.
8. [ ] **Provision IPFS pinning:** create the Pinata JWT and set it where `PinataDriver` reads it.
9. [ ] **Deploy the subgraph** (C2) to the chosen host. The repo currently has only
   `crates/xvision-marketplace/subgraph/schema.graphql` (entity `Agent`, post-AM3) — the
   `subgraph.yaml` manifest + AssemblyScript mappings are **net-new** and must be authored
   against the deployed addresses + ABIs.
10. [ ] **Stand up the Validation Registry signer** (C5) — the trusted validator service that
    gates listing publication.
11. [ ] *(Optional — A6/A7)* provision a public read-only viewer (`xvn.market`) + a relay for
    Tier-B sealed-bundle decryption, if a public marketplace surface is wanted (deferrable if dashboard-only).

---

## 3. Post-deployment — wire the deploy-gated code

These are code changes that were intentionally left as documented seams because they could
not be built/tested without a live deploy. Do them on the build host after §2, then redeploy
the app image (local-build → ship, per CLAUDE.md). Each was reachability-audited 2026-06-09.

1. [ ] **Drivers → runtime surface.** `Erc8004MantleDriver` / `PinataDriver` have **no non-test
   caller** today. Decide the surface that performs on-chain writes (the CLI `marketplace.rs`
   deliberately blocks `MARKETPLACE_DRIVER=onchain`; neither MCP nor dashboard depends on
   `xvision-marketplace` yet). Wire the chosen surface to construct `Erc8004MantleDriver` with the
   deployed `MarketplaceAddresses` + a signer, and `PinataDriver` with the JWT.
2. [ ] **Real `MarketplaceData` impl (AM6 / C7).** Replace `FixtureMarketplaceData` (the single
   line `MarketplaceLayout.tsx`) with a real client backed by the dashboard API / subgraph.
   This is what feeds the frontend listings, receipts, and the `listed_sharpe` the attestation
   engine needs.
3. [ ] **C6 attestation → live loop (Seam A + B + the engine→identity dependency).**
   - Add `xvision-identity` as a dependency of the path that will submit on-chain (the engine
     does NOT depend on identity today — this is a deliberate dependency decision, not just a call).
   - **Seam A:** in `crates/xvision-engine/src/eval/executor/backtest.rs` `run_inner_live`,
     add `realized_pnl` to `LiveDecisionOutcome`, accumulate a rolling per-trade returns buffer,
     and call `maybe_attest(n_trades, &buffer, periods_per_year, listed_sharpe)` at the
     fill-recognition site (`if outcome.fill_happened { n_trades += 1; … }`).
   - **Seam B:** source `listed_sharpe` from the real `MarketplaceData`/`PublicManifest` (step 2).
   - On a fired trigger: record the off-chain Ed25519 pre-anchor (already reachable via
     `xvn eval attest`) and, when registries are configured + the operator holds a license,
     submit on-chain via `IdentityClient::submit_attestation` (value=verdict, decimals=0).
4. [ ] **Real `buyWithAuthorization` in the frontend.** The buy CTA currently calls the fixture
   `purchaseIntent` with an honest "simulated purchase" note (`LineageRoute.tsx`). Wire the real
   path: `useWallet` must expose a **signer** (EIP-3009 `transferWithAuthorization` signing) +
   chain id; the buy mutation calls the real `MarketplaceData.buyWithAuthorization`. The contract
   enforces `recipient == auth.from`, so set the recipient to the connected wallet.
5. [ ] **(When ready to trade real money — V4 only)** flip `VenueLabel::Live` on. Until then the
   cockpit is paper/Alpaca only.

---

## 4. Verification — V2 testnet exit gate

End-to-end on Sepolia, **TESTNET-labelled throughout**:

- [ ] Mint identity (agent #0 = xvn; a test strategy mints its own agent).
- [ ] Create a listing → validation-gated publish succeeds; the license gate is **active immediately**
      (a non-licensee `giveFeedback` reverts `NotLicensed` with no manual wiring).
- [ ] Buy a license (ERC-1155 mint to buyer; x402 or approve+buy per step 2.3).
- [ ] Deploy the purchased strategy live (paper) from the cockpit; pause / resume / flatten / stop
      all behave (positions close on cancel).
- [ ] Run ≥20 live trades → the 20-trade attestation fires → verdict (100/50/0, `tradingYield`/`month`,
      **decimals=0**) is posted on-chain by a license holder and visible in the dashboard.
- [ ] Reputation/attestations render in the marketplace UI via the real data seam.
- [ ] Confirm every chain-bound surface shows the shared `TestnetBadge`/banner.

---

## 5. Deferred-items register (consolidated, with owner)

| # | Item | Blocked on | Owner / phase |
|---|---|---|---|
| 1 | Drivers wired to a runtime write-surface | deploy + a surface decision | §3.1 |
| 2 | Real `MarketplaceData` impl + subgraph indexer (manifest + mappings) | deployed addresses + subgraph host | §2.9 / §3.2 (C7) |
| 3 | C6 attestation in the live loop (`maybe_attest` uncalled) | `listed_sharpe` source (#2) + engine→identity dep | §3.3 |
| 4 | C6 on-chain submission | deployed registries + license + signer | §3.3 |
| 5 | Real frontend `buyWithAuthorization` | deployed Marketplace + `useWallet` signer + #2 | §3.4 |
| 6 | AM2 canonical gen-art renderer (Rust SVG vs canvas diverge) | a design decision | §1 decisions |
| 7 | C5 validator signer service | host + design | §2.10 |
| 8 | AM7 verified-ABI pinning under `crates/xvision-identity/abi/v1/` | post-deploy verify | §2.7 |
| 9 | Public viewer / Tier-B relay (A6/A7) | decision (deferrable if dashboard-only) | §2.11 |
| 10 | V4 mainnet hardening: `Ownable2Step`/renounce guard, `XvnDeployer` CREATE2 gating, run `slither` + `slither-check-upgradeability`, multisig + timelock, fee-recipient, migration scope, Alpaca-live cutover | external audit + V4 | §6 below |
| 11 | Pre-existing `decisions_count` 30/100-bar test failures (`supervisor_notes` missing in a minimal harness) | a test-harness fix (predates this work) | test cleanup |
| 12 | Cosmetic: ~30 prose comments still say "migration 061/062" (now 062/063) | none (cosmetic) | optional |

---

## 6. Mainnet / V4 (OUT OF SCOPE — gated)

`DeployMainnet.s.sol` / `UpgradeTimelock.s.sol` revert by design until: external **audit**
complete (4–8 wk lead — start early), **2-of-3 multisig** signers chosen (3rd "community
trustee" TBD), **timelock** wired, fee-recipient set, testnet→mainnet **migration scope**
decided, and the **Alpaca-live cutover** (`VenueLabel::Live`) decision made. **Real money = V4.**
The 2026-06-09 contract security audit (OZ + Trail-of-Bits skills) found no Critical/High; the
deferred Lows above are the mainnet-hardening checklist.

---

## Addendum 2026-06-10 — USDC re-point (UUPS upgrade, executed)

The original deploy initialized the Marketplace with a community mock USDC that
has **no EIP-3009 support** (research confirmed no EIP-3009 USDC exists on
Mantle Sepolia at all; Mantle MAINNET bridged USDC supports it natively). To
keep the x402 path demoable:

1. `MockUSDC3009` (full EIP-3009 + `faucet()`, cap 10,000e6/call) deployed at
   [`0x68aA91F73f359035875759E1d4C4949A27c84C88`](https://sepolia.mantlescan.xyz/address/0x68AA91f73F359035875759e1d4C4949A27c84c88)
   (PR #898, Sourcify exact_match).
2. `Marketplace.setUsdc` added (onlyOwner, `UsdcChanged` event, storage layout
   untouched) and the proxy upgraded atomically:
   - New impl: [`0x1FE13b656d9571798F7B3074f54eaFBfDc88bC44`](https://sepolia.mantlescan.xyz/address/0x1fe13b656d9571798f7b3074f54eafbfdc88bc44)
     (deploy tx `0xb18c3d04…bbfe2`)
   - `upgradeToAndCall(newImpl, setUsdc(0x68aA…4c88))`:
     [`0x7b990ce1…a9c83`](https://sepolia.mantlescan.xyz/tx/0x7b990ce18eb24cae6e2c28ff7d48963949a03be0af66349daab02aa4b01a9c83)
   - Script: `contracts/script/UpgradeMarketplaceSetUsdc.s.sol` (chain-5003 guarded).
3. Verified live: `usdc()` returns the new token; end-to-end on-chain smoke —
   `faucet` → `approve` → `buy(listing 1)` minted license #1
   ([buy tx](https://sepolia.mantlescan.xyz/tx/0x16c7cbda65fc882e3090d52a153950429f8a8facdac5dd53d6e5943b6a102964)).

Caveat: listing prices are token-unit-denominated; old-token balances/allowances
do not carry over (irrelevant on testnet — no prior sales). §2.3's EIP-3009
probe is now resolved: **x402 works on Sepolia via MockUSDC3009; mainnet uses
real bridged USDC unmodified.**

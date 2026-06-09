# Blockchain Implementation — Synthesis & Execution Plan

> **Date:** 2026-06-08
> **Status:** Draft for operator review
> **Supersedes navigation in:** [`2026-05-26-blockchain-plan-navigation.md`](./2026-05-26-blockchain-plan-navigation.md) (old nav doc)
> **Drives:** [`2026-06-08-live-trading-marketplace-spec.md`](../specs/2026-06-08-live-trading-marketplace-spec.md) (new spec — **the priority**)
>
> **Purpose:** Reconcile the old blockchain navigation doc (phase-ordered build map,
> written 2026-05-26 *before* most of it was built) against the new Live Trading +
> Marketplace spec (2026-06-08). Strip out work that is already done or superseded,
> flag built work that must be amended, and lay out the remaining path to a
> blockchain-live testnet — with every manual-intervention point called out.
>
> **Audit basis:** four-surface code audit run 2026-06-08 (contracts, Rust crates,
> frontend, live-trading backend). Findings inline below.

---

## 0. TL;DR

Between 2026-05-26 and 2026-06-08, **old-plan Phases 0–6 were largely built** — but
entirely against **mocks and fixtures**. The shape is real; nothing touches a chain.

- **Contracts:** all 8 written + unit/integration tested on anvil. **Zero deployed** to Mantle Sepolia or mainnet. Config addresses are all-zero placeholders.
- **Rust crates:** `MockDriver` + `IdentityClient` (real, op-only) work; the on-chain `Erc8004MantleDriver` and `PinataDriver` are `NotImplemented` stubs. No CLI path reaches a chain.
- **Frontend marketplace:** 6 routes + buy/sell/receipt/attestation UI fully built, **100% fixture-backed** behind a single swappable `MarketplaceData` seam.
- **Live Trading page:** only the minimal chart exists. The new cockpit (strip, stats, positions, transport controls) is **not built**, and its **3 backend blockers do not exist**.

**So the work splits cleanly into two tracks:**

1. **Live Trading (priority, off-chain).** Backend blockers + cockpit UI. No chain dependency. Ship this first per the new spec's implementation order.
2. **Marketplace go-live (the actual "blockchain implementation").** Deploy contracts → implement drivers → wire the frontend data seam to a real backend/subgraph → attestation engine. This is mostly *activation of already-built shapes* plus a defined set of manual deploy steps.

---

## 1. Current build state (audited 2026-06-08)

Legend: ✅ done · 🟡 built-but-mock/fixture · 🟥 not built · ⚠️ needs amendment

| Layer | Item | State | Evidence |
|---|---|---|---|
| **Contracts** | IdentityRegistry / ReputationRegistry / ValidationRegistry (ERC-8004) | ✅ written, anvil-tested | `contracts/src/registries/` |
| | LicenseToken (ERC-1155, soulbound-default, per-listing transferable) | ✅ written | `contracts/src/LicenseToken.sol` |
| | ListingRegistry / Marketplace / EvalAttestationRegistry | ✅ written (UUPS, op-EOA admin) | `contracts/src/` |
| | XvnDeployer (CREATE2 factory) | ✅ written | `contracts/src/XvnDeployer.sol` |
| | x402 / EIP-3009 `buyWithAuthorization` | ✅ written | `Marketplace.sol`, `IERC3009.sol` |
| | Fee split (configurable, snapshot per listing, cap 10%) | ✅ written | `libraries/Splits.sol`, `Marketplace.sol:41` |
| | Licenses soulbound-by-default (`transferableLicense` opt-in per listing) | ✅ written | `LicenseToken.sol:43,111` |
| | ~~ERC-2981 secondary royalty~~ | ⛔ **not needed** — infinite-supply + soulbound ⇒ no secondary market (operator decision 2026-06-08) | — |
| | **On-chain deploy (Sepolia / mainnet)** | 🟥 **never run** | only `broadcast/.../31337` (anvil); configs all-zero |
| **Deploy scripts** | DeployTestnet / RegisterPlatformAgent | ✅ written, never executed | `contracts/script/` |
| | DeployMainnet / UpgradeTimelock | ✅ V4-gated stubs (revert) | `contracts/script/` — out of scope here |
| **Rust** | `xvision-identity` SVG gen-art + manifest types | ✅ real | `crates/xvision-identity/src/genart.rs` |
| | `IdentityClient` register / post_reputation / read | 🟡 real code, op-only, never wired to CLI; integ tests `#[ignore]` | `client.rs` |
| | `sol!` bindings | ⚠️ hand-written ahead of deploy; pin verified ABI post-deploy | `contracts.rs` |
| | `xvision-marketplace` `AnchorDriver` + `MockDriver` | 🟡 mock only | `adapter.rs` |
| | `Erc8004MantleDriver` (4 verbs) | 🟥 `NotImplemented` | `adapter.rs:194` |
| | `IpfsStore` + `PinataDriver` | 🟥 `NotImplemented` | `ipfs.rs` |
| | Subgraph | 🟡 schema only, no mappings | `xvision-marketplace/subgraph/schema.graphql` |
| | `RegistryAddresses` / `MarketplaceAddresses` | 🟡 env-var driven, currently `None` | `client.rs`, `contracts.rs` |
| **CLI** | `xvn marketplace list/publish/buy/attest` | 🟡 wired to `MockDriver`; `MARKETPLACE_DRIVER=onchain` deliberately rejected | `xvision-cli/.../marketplace.rs` |
| | `xvn admin register-platform-agent` / identity verbs | 🟥 not surfaced (op-only via `examples/mint_identity.rs`) | `docs/cli-non-surfaced.md` |
| **Frontend MP** | browse / lineage / creator / leaderboard / sell / receipt | ✅ built | `frontend/web/src/features/marketplace/routes/` |
| | `MarketplaceData` fixture seam (one-line swap point) | 🟡 `FixtureMarketplaceData` | `data/MarketplaceData.ts`, `routes/MarketplaceLayout.tsx:7` |
| | gen-art renderer (bitfields v2 canvas) | ✅ real, seed-driven | `components/GenArtPlaceholder.tsx` |
| | attestation viewer / VerifiedBadge / X402Badge / TxChip | ✅ built | `components/` |
| | `useWallet` (MetaMask/EIP-1193 + localStorage) | 🟡 stub-ish, single-chain, TESTNET-labelled | `lib/wallet.ts`, `routes/settings/wallet.tsx` |
| | Settings → **Marketplace opt-in** tab | 🟥 not built (only a bare wallet connect tab) | — |
| **Live page** | `/live` minimal chart (`LiveChartV2Container`) | ✅ exists | `routes/live.tsx` |
| | cockpit: strip / stat strip / positions table / transport / wallet banner | 🟥 not built | new spec §2 |
| **Live backend** | equity + decision SSE stream (`EquityPoint`, `LiveDecisionRow.pnl_realized`) | ✅ exists | `xvision-engine/src/api/chart.rs:1093` |
| | **per-strategy pause** (`paused` on run + eval-loop check + routes) | 🟥 global only (`SafetyManager`) | `safety/state.rs`, `safety/gate.rs:149` |
| | **stop + close positions** (cancel closes broker positions) | 🟥 cancel only terminates | `api/eval.rs:420` |
| | **`VenueLabel::Live`** | ⚠️ enum exists, **rejected at validation** | `eval/live_config.rs:231` |
| | attestation auto-trigger (20-trade → reputation post) | 🟥 manual Ed25519 attest only; no chain post | `api/eval.rs:4095` |
| **Brokers** | Alpaca paper | ✅ | `xvision-execution/src/alpaca.rs` |
| | Bybit testnet (`BybitPaperSurface`) | ✅ (old-plan Phase 2 done) | `bybit.rs:299` |
| | Orderly live | 🟡 impl exists, not executor-wired | `orderly.rs`, `broker_surface.rs:680` |
| | **real-money live surface** (`AlpacaLiveSurface` etc.) | 🟥 none | — |

---

## 2. Old plan → disposition (what to delete, keep, or carry forward)

The old nav doc's Phase ordering is **superseded** by the new spec's implementation
order. Per-phase disposition:

| Old phase | Disposition |
|---|---|
| **Ph0** Harden existing (regression suite) | ✅ **DONE** — `d2e676a` (A/B, multi-asset, Alpaca paper regression suite). Drop from remaining work. |
| **Ph1** Design sprint (marketplace/identity/gen-art/layout mockups) | ✅ **DONE** — direction doc + Phase-F frontend (PRs #616–619) + metadata data-contract spec. Drop. |
| **Ph2** Bybit provider | ✅ **DONE** — `BybitPaperSurface` (testnet). Drop. |
| **Ph3** Foundry + ERC-8004 testnet-ready | 🟡 **PARTIAL** — contracts + scripts + anvil tests done (#627). **Not deployed.** → folds into **Track C / §5 deploy runbook**. |
| **Ph4** Identity page + gen-art | 🟡 **PARTIAL** — gen-art (canvas) + identity/lineage page **built but fixture-backed**. → activation only (§3 Track C). ⚠️ gen-art divergence (§4). |
| **Ph5** Marketplace contracts + Rust crate | 🟡 **PARTIAL** — contracts written, crate skeleton + MockDriver done. **Drivers stubbed.** → **Track C**. |
| **Ph6** Marketplace UI | ✅ **largely DONE** (fixture-backed) → needs data-seam wiring only (§3 Track C). |
| **Ph7** UI layout pass / popup audit | ➡️ **Re-homed.** Now tracked by the separate design sweep (`2026-06-08-master-implementation-list.md` / design-improvement-sweep-qa). Out of scope for this blockchain plan. |
| **Ph8** V3 (autooptimizer) + V4 (mainnet) prep | ➡️ **Out of scope.** V3 autooptimizer already advanced independently. V4 (audit, multisig, timelock, mainnet) remains gated — see §6. |

**Net:** Phases 0–2 are fully done. Phases 3–6 are "shapes built, not activated."
Phase 7 belongs to the design sweep. Phase 8 stays gated.

### Old open-questions still live (carry forward)

From old nav §4, these are **still unresolved and now block the new spec**:

- **A5** Wallet provider (Privy vs WalletConnect vs MetaMask). Current code = MetaMask-only. New spec just reads `useWallet().address`, so MetaMask is *sufficient for testnet*; decide before mainnet.
- **A6** Public read-only viewer + domain (`xvn.market`). New spec is silent on the public viewer; if the marketplace is operator-dashboard-only for testnet, A6 can defer.
- **A7 / E1** Sealed-bundle relay host + device-fingerprint freshness — required for **Tier B sealed listings only**. Tier A (open) ships without it.
- **B3** USDC.e on Mantle supports EIP-3009 `transferWithAuthorization` — **probe before relying on x402 `buyWithAuthorization`**; fallback is approve+buy (2-tx), already implemented as `buy()`.
- **C1/C7** IPFS: ship `PinataDriver` for V2, `iroh` deferred to V3 (confirmed).
- **C2** Subgraph host (Goldsky / The Graph / self-host) — needed to back the frontend data seam with real listing/attestation data.

---

## 3. What the new spec ADDS or RE-PRIORITIZES vs the old plan

The new spec is **not** a contract redesign — the token stack (ERC-8004 + ERC-1155 +
x402 + IPFS) matches what's already built. Its novelty is:

1. **Live Trading cockpit** as a first-class surface with **3 hard backend blockers** the old plan never scoped (per-strategy pause, stop+close, `VenueLabel::Live`). **This is the new top priority.**
2. **Automated attestation loop** — 20-trade trigger → sharpe-delta → `giveFeedback`, license-gated on-chain (`balanceOf > 0`). Old plan mentioned "ReputationRegistry writes per cycle" but the **20-trade rolling trigger + sharpe-delta verdict mapping is new and precise** (spec §3.6).
3. **Live equity curve from on-chain anchored trades only** (spec §3.7) — buyer-contributed, median-aggregated, opt-out at purchase. New, and depends on anchoring being live.
4. **Express-deploy overlap** (buy → install → "Deploy live →" → cockpit) tying the two surfaces together (spec §4).
5. **Revenue split stated as 90/10 primary** (spec §3.8). Contract default is configurable; set `initialFeeBps = 1000` (10%) at deploy — **config, not code**.

---

## 4. Built work that must be AMENDED

These are places where existing code conflicts with, or falls short of, the new spec.
Each needs a decision or a change before go-live.

| # | Amendment | Where | Why |
|---|---|---|---|
| ~~AM1~~ | ~~ERC-2981 secondary royalty~~ — **dropped 2026-06-08.** ERC-1155 licenses are infinite-supply and soulbound-by-default, so there is no secondary market to take a royalty on. Spec item 17 + §3.8 "secondary royalty" rows are **cut**; `transferableLicense` stays as-is (creators may still opt in, but no royalty wiring). | — | No work. |
| **AM2** | **Gen-art divergence.** Two implementations exist: Rust `genart.rs` (deterministic **SVG** → `data:` URI for `tokenURI`) and frontend `GenArtPlaceholder.tsx` (**canvas bitfields v2**). They render *different art from the same seed*. Spec §3.9 puts `"image": "<genArtSeed rendered>"` on-chain. | `xvision-identity/src/genart.rs` vs `frontend/.../GenArtPlaceholder.tsx` | The on-chain `tokenURI` image and the dashboard preview will not match unless reconciled. **Decide the canonical renderer** before mint flow goes live. |
| **AM3** | ~~One-NFT-per-lineage vs one-agent-per-strategy~~ — **RESOLVED 2026-06-08: the ERC-8004 agent = the strategy/listing** (one `agentId` per listed strategy, `agentId ↔ agent_id` ULID, per spec §3.9). The old A4 "one-NFT-per-lineage" model is **dropped** — lineage/derivatives is deprioritized now that the mechanism was largely removed from the optimizer. | `IdentityRegistry.sol`, spec §3.9 | **Action:** `register()` is called per strategy/listing, not per lineage. The subgraph `Lineage` entity should be renamed/repurposed to a per-strategy agent record (or `parentLineage` left as an optional self-reference, unused for v2). No "variants as content-hash records under a lineage NFT." Confirm before deploy so events/storage are final. |
| **AM4** | **Attestation: two systems.** Backend has an Ed25519 off-chain `attest()` writing `eval_attestations`; the chain path is ERC-8004 `giveFeedback` via `IdentityClient::post_reputation`. They are unconnected. | `api/eval.rs:4095` vs `client.rs:349` | New spec §3.6 wants on-chain, license-gated, sharpe-delta attestation. **Decide** whether off-chain attest is retired, kept as a pre-anchor stage, or both. |
| **AM5** | **`Erc8004MantleDriver` + `PinataDriver` are stubs.** | `adapter.rs:194`, `ipfs.rs:46` | Implement the 4 verbs (`publish/buy/attest/revoke`) and Pinata `put/get`. This is the core "make it transact" work. |
| **AM6** | **Frontend data seam is fixtures.** `FixtureMarketplaceData` returns fake `TxRef`s and hardcoded listings. | `MarketplaceData.ts`, `MarketplaceLayout.tsx:7` | Add a real `MarketplaceData` impl backed by the dashboard API / subgraph. One-line provider swap, but the API/subgraph behind it is net-new (C2). |
| **AM7** | **`sol!` bindings written ahead of deploy.** | `contracts.rs` | After Sepolia deploy + verify, pin verified ABI JSON under `crates/xvision-identity/abi/v1/` and regenerate. |
| **AM8** | **`xvn marketplace` CLI hard-rejects `onchain`.** | `marketplace.rs:87` | Once drivers are real, decide the on-chain surface: MCP/dashboard (current intent) or a gated CLI flag. Identity writes stay op-only by policy (`cli-non-surfaced.md`). |
| **AM9** | **Forge build/EVM drift.** Audit hit an OZ v5.0.2 `mcopy` vs `evm_version=shanghai` compile issue locally, though #627/#716 reported 58/58 green. | `contracts/foundry.toml` | **Verify `forge build && forge test` is green** before any deploy. Reconcile EVM target (matches memory item: PR #630 "evm shanghai, gated on 5003 smoke deploy"). |

---

## 5. Remaining work — execution plan

Two tracks. **Track A (Live Trading) is the priority and has no chain dependency** —
do it first. Track C (Marketplace go-live) is the blockchain implementation proper.

### Track A — Live Trading backend blockers (must ship first; off-chain)

> New spec §2.12, §6 "Must ship first." Pure Rust + DB. No wallet, no chain.

- **A1. Per-strategy pause.** Add `paused` to the run record + migration (per
  `cycle-migration` skill / dual-migration-dir rules). Check it in the eval loop
  *before decision dispatch* (alongside the existing global `SafetyManager`). Routes
  `POST /api/eval/runs/:id/pause` + `/resume`. (`safety/state.rs`, `safety/gate.rs`,
  `eval/run.rs`.)
- **A2. Stop + close positions.** Extend cancel (`api/eval.rs:420`) to compute open
  positions from `eval_decisions` (opens without matching close) and submit close
  orders through the broker surface (wrapped in `SafetyGate::check_broker_submit`)
  *before* terminating. Persist closes so equity/PnL settle.
- **A3. `VenueLabel::Live` enablement.** Remove/gate the v1 rejection
  (`live_config.rs:231`) behind a per-strategy verdict + kill-switch, and wire a
  **real-money broker surface** (`AlpacaLiveSurface`, or wire `OrderlyLiveSurface`).
  ⚠️ **This is the real-money gate — treat as V4-adjacent.** For the testnet
  marketplace milestone, `Live` can stay rejected; paper/testnet venues drive
  attestation. Coordinate with engine team (spec §5 deferral).

### Track B — Live Trading cockpit UI (after A1/A2; no chain)

> New spec §2.4–2.11, §6 items 4–9.

- B1. Strategy strip + column picker (`safeStorageGet/Set`, key `live_trading_strip_metric`).
- B2. Wallet banner (disabled-actions state; separate from `SafetyPauseBanner`).
- B3. Account stat strip (equity / daily PnL / drawdown / unrealized) from the existing SSE stream.
- B4. Active positions table (derive from `DecisionRowDto` opens; no pagination v1).
- B5. Transport controls (pause/resume single-click; stop = type-to-confirm via `HaltStrategyButton`, inline-expand, no modal).
- B6. `LiveStrategiesSection` home → compact summary strip + "Go to Live Trading →".
- B7. `/live/:id` deep-link opens cockpit with strategy pre-selected.

*Backend data (equity, decisions, pnl_realized) already streams — this is wiring + layout.*

### Track C — Marketplace go-live (the blockchain implementation)

> Activates the already-built shapes. Order matters: deploy → bindings → drivers →
> data seam → attestation. Manual deploy steps are flagged in §6.

- **C1. Contract finishing.** Resolve **AM3 (agent granularity)** and **AM9 (forge
  green)**. Re-run unit/integration tests. *(If AM3 changes storage or events, do it
  before the Sepolia deploy so addresses are final.)* No royalty work (AM1 dropped).
- **C2. Deploy to Mantle Sepolia.** → **§6 runbook (MANUAL).**
- **C3. Wire addresses + bindings.** Populate `config/mantle-sepolia.toml`; set
  `MANTLE_TESTNET_*` env on the dashboard service; pin verified ABIs (**AM7**);
  flip `RegistryAddresses`/`MarketplaceAddresses` to resolve.
- **C4. Implement `Erc8004MantleDriver`** (4 verbs) and **`PinataDriver`** (`put/get`)
  (**AM5**). Turn the `#[ignore]` anvil integration tests into Sepolia smoke tests.
- **C5. Validation Registry gate** (spec §3.3): listing publish requires (a) ≥1
  anchored backtest, (b) validator `response >= 70`, (c) creator wallet + registered
  agent. Needs a **trusted validator signing service** — *new, undesigned* (who signs,
  where it runs).
- **C6. Attestation engine** (spec §3.6, **AM4**): 20-trade rolling trigger →
  sharpe-delta vs listed → verdict (100/50/0, `tradingYield`/`month`) →
  license-gated `giveFeedback` (`balanceOf > 0`). Background job in the engine;
  connect off-chain attest to on-chain post.
- **C7. Real backend data seam** (**AM6**): build the subgraph mappings (C2 host
  decision) + a `MarketplaceData` API impl; swap `FixtureMarketplaceData` →
  real client. Live equity aggregation (spec §3.7, median across contributors,
  opt-out, `revokeFeedback` removes from aggregate).
- **C8. Frontend activation:** Settings → Marketplace opt-in tab; real buy flow
  (x402 `buyWithAuthorization` if B3 passes, else approve+buy); express-deploy CTA
  on receipt (spec §3.4); purchased-strategy badge + "Source" filter in `/strategies`
  (spec §3.5); enforce TESTNET labelling everywhere.

### Dependency sketch

```
Track A (pause, stop+close)  ──► Track B (cockpit UI)        [off-chain, ship first]
Track A3 (VenueLabel::Live)  ──► real-money trading          [gated, V4-adjacent]

C1 contracts ──► §6 DEPLOY (manual) ──► C3 bindings ──► C4 drivers ──► C6 attestation
                                                    └─► C7 data seam ──► C8 frontend
```

---

## 6. Manual-intervention runbook (human-in-the-loop)

Everything that **cannot be done by an agent / by code alone**. Per `CLAUDE.md`:
**all Foundry builds + deploys happen on the local build host or CI — never on the
small VPS / Coolify nodes** (no Cargo/Docker-build on deploy hosts).

### Testnet bring-up (the "deploy actual smart contracts" milestone)

1. **Mint the nonce-0 EOA ("forever wallet").** This wallet is reused on every chain
   to keep CREATE2 addresses deterministic — earmark it carefully, back up the key.
   *(Manual, one-time, irreversible identity decision.)*
2. **Fund the EOA on Mantle Sepolia.** Faucet ops (old C6): pre-fund ~5× estimated
   chain footprint. *(Manual.)*
3. **Verify USDC.e EIP-3009 support on Mantle (B3 probe).** Determines x402 buy path
   vs approve+buy fallback. *(Manual investigation.)*
4. **Run `DeployTestnet.s.sol`** from the local build host against `MANTLE_SEPOLIA_RPC_URL`
   (deploys XvnDeployer → 3 registries → 4 UUPS proxies, admin = operator EOA, fee
   `initialFeeBps = 1000`). *(Manual, gas-spending, irreversible.)*
5. **Run `RegisterPlatformAgent.s.sol`** to mint xvn as agent #0. *(Manual.)*
6. **Paste deployed addresses** into `config/mantle-sepolia.toml` and set
   `MANTLE_TESTNET_*` env vars on the dashboard service (C3). *(Manual.)*
7. **Verify contracts on Mantlescan** (`MANTLESCAN_API_KEY`) and **pin verified ABIs**
   (AM7). *(Manual.)*
8. **Provision IPFS pinning** — Pinata account + JWT for `PinataDriver` (C1 backstop tier). *(Manual.)*
9. **Deploy the subgraph** to the chosen host (C2: Goldsky / The Graph / self-host). *(Manual + decision.)*
10. **Stand up the Validation Registry signer** (C5) — trusted validator service. *(Manual + design.)*
11. *(If public viewer wanted — A6)* provision `xvn.market` / public read-only viewer + relay for Tier B sealed bundles (A7). *(Manual + decision; deferrable if dashboard-only.)*

### Decisions required before/at deploy

- ~~AM3 agent granularity~~ — **resolved: agent = strategy/listing** (lineage dropped). Apply to subgraph schema + `register()` call sites before deploy.
- **AM2** canonical gen-art renderer (Rust SVG vs canvas) — before mint flow.
- **A5** wallet provider — MetaMask OK for testnet; decide before mainnet.
- **C2** subgraph host; **C5** validator host.

### Mainnet / V4 (explicitly OUT OF SCOPE here — gated)

`DeployMainnet.s.sol` / `UpgradeTimelock.s.sol` revert by design until: external
**audit** complete (4–8 wk lead — start the conversation early, old C5), **2-of-3
multisig** signers chosen (D1, 3rd "community trustee" TBD), **timelock** wired,
fee-recipient address set (D2), testnet→mainnet **migration scope** decided (D3),
and the **Alpaca-live cutover** decision made (D4 / Track A3). Real money = V4.

---

## 7. Recommended sequencing

1. **Now → Track A + B (Live Trading).** Highest user value, zero chain risk, unblocks the cockpit the operator actually asked for. (A3 `VenueLabel::Live` stays gated.)
2. **Parallel: Track C1 + AM resolutions.** Finish contracts (royalty, agent granularity, forge-green) so the Sepolia deploy is final-shape.
3. **§6 testnet bring-up (manual).** The "deploy actual smart contracts" milestone.
4. **Track C4–C8.** Implement drivers, attestation, real data seam, frontend activation — against live Sepolia.
5. **V2 testnet exit gate:** end-to-end on Sepolia — mint identity → list (validation-gated) → buy (license) → live-trade → 20-trade attestation → reputation visible in dashboard, **TESTNET-labelled throughout.**
6. **V4 prep** (audit/multisig/timelock/mainnet) — separate gated launch.

---

## 8. Maintenance / doc disposition

- This doc **replaces the phase-ordering role** of `2026-05-26-blockchain-plan-navigation.md`. Keep the old nav doc as the source for the *resolved design decisions* (its §4 A1–A4) and source-material map; this doc owns *remaining execution*.
- The old nav doc's Phase 7 (layout/popup audit) is now owned by the **design sweep** (`2026-06-08-master-implementation-list.md`), not this plan.
- Update §1 state table as each item lands; move resolved AM/§6 decisions into the relevant spec or a new ADR.

---

## 9. Implementation status — 2026-06-09 session (branch `worktree-live-marketplace-impl`)

All agent-doable code up to the manual deploy wall is **DONE, reviewed (spec + code-quality; contracts also got a security audit), and pushed** to `origin/worktree-live-marketplace-impl`. Not merged to main; not deployed.

**Completed (each two-stage reviewed):**
- **A1** per-run pause — additive broker-submit skip, fail-closed live gate, migration 061. **A2** close-positions-on-cancel — broker flatten via the normal fill path. **A3** on-demand flatten endpoint (migration 062) — shared close helper with A2, pause→flatten path tested.
- **B-I…B-IV** Live Trading cockpit — `/live` cockpit shell, strategy strip + wallet banner, account stat strip + active positions table, transport controls (pause/resume/stop/flatten, inline-expand, optimistic cache), home summary strip. `RunSummary` ts-rs regenerated with `paused`/`paused_at`/`flatten_requested`.
- **C1** contracts — §3.6 license-gated `giveFeedback` + `revokeFeedback` (tombstone) in `ReputationRegistry`; AM3 agent=strategy/listing semantics + subgraph schema `Lineage`→`Agent`; AM9 OZ re-pinned **v5.0.2** (had drifted to 5.6.1, breaking the `shanghai` build). 72 forge tests.
- **Security audit** (OZ + building-secure-contracts skills) over the whole `contracts/` tree → fixed **M-1** (UUPS `ReentrancyGuardUpgradeable` + `__gap` recompute), **M-2** (x402 `recipient == auth.from` binding), **L-1** (free-listing one-per-recipient cap). 79 forge tests. Audit is now a standing review step (memory: `feedback_contract_security_audit_review_step`).
- **C4** drivers — `Erc8004MantleDriver` (4 verbs) + `PinataDriver` (put/get) real; x402 recipient guard mirrors M-2.
- **C6** attestation engine — pure verdict (sharpe-delta → 100/50/0), 20-trade rolling trigger, AM4 bridge (off-chain Ed25519 pre-anchor KEPT + on-chain `post_reputation` ADDED, license-gated, deploy-gated no-op).
- **C8** frontend activation — Settings→Marketplace opt-in tab (localStorage, default-off, gates nav + routes), shared `TestnetBadge`/`TestnetBanner`, fixture buy CTA with honest "simulated purchase" cue.

### Deferred-items register (owning phase)

| Item | Why deferred | Owner |
|---|---|---|
| Real on-chain `buyWithAuthorization` (EIP-3009) from the frontend | needs deployed contract + `useWallet` signer + real `MarketplaceData` impl | §6 deploy + AM6 (C7) + a wallet-signer task |
| `Erc8004MantleDriver` full anvil/Sepolia round-trip test | `sol!` bindings are interface-only (no bytecode); `#[ignore]`'d scaffold present | §6 deploy + AM7 ABI pin |
| C6 live-loop hook: per-trade return-series accumulation; `listed_sharpe` source | `maybe_attest` takes them as inputs today; series-threading into the live driver + listing-manifest read are integration seams | Track C7 (data seam) + a live-driver wiring task |
| AM6/C7 real `MarketplaceData` impl + subgraph indexer (manifest + mappings) | net-new backend/subgraph behind the fixture seam | C7 (deploy-gated) |
| AM2 canonical gen-art renderer (Rust SVG vs frontend canvas-bitfield diverge) | decide canonical before mint flow goes live | pre-mint decision |
| C5 validator signer service | not started | C5 |
| AM7 verified ABI pinning under `crates/xvision-identity/abi/v1/` | post-deploy + verify | §6 deploy |
| Audit Lows deferred to V4: renounce-bricks/`Ownable2Step`, permissionless `XvnDeployer` CREATE2 gating, install + run `slither`/`slither-check-upgradeability` | mainnet trust-model hardening | V4 (multisig/timelock prep) |
| Pre-existing `decisions_count` 30/100-bar test failures (`supervisor_notes` missing in a minimal harness) | pre-existing on base; not introduced here | a test-harness fix |

### Reachability pass (2026-06-09)

A reachability audit traced every new capability from its entry point. **Reachable end-to-end:** A1/A2/A3 (pause/resume/cancel-flatten/flatten — routes registered in `dashboard/.../server.rs`, executor honor points in `backtest.rs`, cockpit `useTransport`); the `/live` cockpit components (data-wired); C8 marketplace (opt-in-gated nav + routes + fixture buy CTA). **Intentionally orphaned-pending-wiring (NOT accidental dead code):**

- **C6 attestation engine (`maybe_attest`) is not called by the live loop yet.** Two real blockers beyond the documented seams: (1) the verdict needs `listed_sharpe`, which has **no source until the marketplace data seam (C7) lands**; (2) the on-chain submit lives in `xvision-identity`, which is **not a dependency of `xvision-engine`** — wiring it requires a deliberate dependency decision, not just a function call. Until both exist, force-wiring an always-inert `maybe_attest` call into the critical 4000-line live loop would add risk for zero behavior. Hook point when ready: `backtest.rs` ~`run_inner_live` fill-recognition site (add `realized_pnl` to `LiveDecisionOutcome`, accumulate a rolling returns buffer, call `maybe_attest`).
- **`Erc8004MantleDriver` / `PinataDriver` / identity `submit_attestation`/`holds_license`/`post_reputation`** have no non-test callers — deploy-gated (need deployed contracts/addresses/JWT/signer). The CLI `marketplace.rs::driver()` deliberately blocks `MARKETPLACE_DRIVER=onchain` (message corrected 2026-06-09 to stop pointing at a non-existent MCP/dashboard path).

**Discoverability:** the `/live` cockpit has no left-nav (`Sidebar.tsx`) entry — reachable only via deep-link and the home `LiveSummaryStrip` link. Adding a nav entry is a product decision (main framed live trading as "not enabled in this build"; `VenueLabel::Live` is OFF / no real money), tracked separately.

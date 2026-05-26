# Blockchain Plan — Navigation Doc

> **Purpose:** Single entry point for the blockchain build. Maps the current
> project state to a phase-ordered plan, surfaces open questions that still
> need direction, and links out to every spec/plan/ADR that informs the work.
> Created 2026-05-26.
>
> **Related execution-board doc:** [`2026-05-13-v2-v4-action-plan.md`](./2026-05-13-v2-v4-action-plan.md) — the broader V2-V4 roadmap. This doc is narrower (blockchain-only) and adds explicit phase ordering + open-question tracking.

---

## 1. Current state (as of 2026-05-26)

**Completed but not yet trusted (need regression tests):**
- A/B compare
- Multi-asset
- Live ordering via Alpaca paper

**To build out (this doc's scope):**
- Marketplace (contracts + UI + design)
- Generative art NFT for strategies
- Testnet readiness (ERC-8004 stubs on Mantle Sepolia)
- Bybit provider
- Foundry project (`contracts/` tree)
- Everything in [`smart-contract-surface-design.md`](../specs/2026-05-08-smart-contract-surface-design.md)
- Non-custodial wallet rail (per [`non-custodial-agent-wallets-design.md`](../specs/2026-05-09-non-custodial-agent-wallets-design.md))

**Design dissatisfaction (revisit before building):**
- ~~Marketplace UX~~ — direction set 2026-05-26 in [`2026-05-26-marketplace-design-direction.md`](./2026-05-26-marketplace-design-direction.md). Phase 1 produces mockups from that direction.
- ~~Identity page~~ — direction set 2026-05-26, same doc.
- Overall UI layout pass — still pending (Phase 7).

**Status of upstream specs:**
- [`smart-contract-surface-design.md`](../specs/2026-05-08-smart-contract-surface-design.md) — Deferred, design accepted
- [`marketplace-plugin-design.md`](../specs/2026-05-09-marketplace-plugin-design.md) — Deferred, design accepted
- [`non-custodial-agent-wallets-design.md`](../specs/2026-05-09-non-custodial-agent-wallets-design.md) — Draft
- [ADR 0008 ERC-8004 deployment](../../../decisions/0008-erc8004-deployment.md) — Accepted, deferred

**What exists in code today:**
- `crates/xvision-identity/` — stub `sol!` bindings against ERC-8004 draft interface; `RegistryAddresses::mantle_testnet()` and `mantle_mainnet()` both return `None`; integration tests are `#[ignore]`d. Crate is opt-in (excluded from workspace default-members).
- No `contracts/` Foundry tree yet.
- No `xvision-marketplace` crate yet.

---

## 2. End state

V2 complete and V3/V4 prep started in parallel. Specifically:

- A new operator can install xvision, paper-trade on **both** Alpaca (equities) and Bybit (crypto), mint a strategy NFT on Mantle Sepolia, list it, see its identity page, buy it on testnet, and watch reputation/validation receipts accumulate.
- No real money has moved at this point — V4 (mainnet, live capital) is a separate, gated launch.
- V3 (autoresearcher) and V4 (mainnet) preparation tracks have started in parallel.

**Reframe to keep straight:** "real money trading" = V4, not V3. V3 is autoresearcher (mostly off-chain, consumes V2C primitives). Don't conflate them when planning.

---

## 3. Phase ordering

### Phase 0 — Harden what's already done (1-2 weeks)

Pin A/B compare, multi-asset, and Alpaca paper ordering in a regression test suite that runs on every PR. Everything downstream touches either the eval engine or the broker layer, so a silent regression here is expensive later.

**No net-new work during this phase.** Resist the temptation.

**Exit:** all three flows have integration tests; CI fails if any regresses.

### Phase 1 — Design sprint (1 week, no code)

Treat marketplace UX, identity page, generative art, and the UI layout pass as **one design exercise**, not four. They share components: a listing card shows the strategy's gen-art; the identity page is what a listing's strategy-detail link goes to; the layout pass standardizes the chrome they live inside. Doing them separately produces inconsistency and rework.

**Why design before contracts:** contract metadata (`agentURI` JSON shape, `tokenURI` art encoding, event field shape) is determined by what the identity page wants to display. Build contracts first and you redo them.

> **Direction set 2026-05-26.** Marketplace + identity UX, public/private storage split, gen-art approach, and the "thin read-only viewer" architecture are locked in [`2026-05-26-marketplace-design-direction.md`](./2026-05-26-marketplace-design-direction.md). Phase 1's job is to turn that direction into mockups + a metadata spec, not to re-litigate the direction.

**Output:**
- Locked mockups for the four surfaces in the direction doc §4–§5: marketplace browse (`/marketplace`), strategy identity page (`/marketplace/lineage/<name>`), creator profile (`/marketplace/creator/<handle>`), leaderboard (`/marketplace/leaderboard`).
- Mockup for the seller-onboarding inline flow (`/marketplace/sell`) and purchase-receipt page (`/marketplace/receipts/<tx>`).
- Generative-art algorithm locked (per direction doc §6.2 Tier 0 — deterministic SVG, `data:` URI). Sample wall of 200 to verify it doesn't degrade at scale.
- Metadata spec: one page listing fields in the NFT `tokenURI` JSON (Tier 0), the IPFS-pinned Tier 1 metadata JSON, the sealed Tier 2 bundle, and the events the chain emits for marketplace browse + creator profile.
- Layout standards (chrome, navigation, empty states, loading states) applied to existing pages first as a dry run.

**Exit:** mockups reviewed + metadata spec written + gen-art encoding chosen.

### Phase 2 — Bybit provider (2-3 weeks)

Slots here because (a) independent of contracts so it runs concurrently with Phase 1, (b) going from one broker to two stress-tests the broker abstraction before Orderly testnet adds a third, (c) Bybit testnet gives live crypto data + paper fills — the missing piece between Alpaca paper (equities) and Orderly testnet (perps with synthetic prices).

**Exit:** Bybit testnet paper trade lands in the event store; the same strategy that runs on Alpaca paper runs on Bybit paper unchanged at the strategy layer.

### Phase 3 — Foundry + ERC-8004 testnet ready (2 weeks)

Bootstrap the `contracts/` tree (Foundry, OpenZeppelin, OZ-upgradeable, forge-std). Mint your **nonce-0 EOA** — this wallet is reused on every chain forever, so earmark it carefully. Deploy `XvnDeployer` (CREATE2 factory) from that EOA. **This must come first** or none of the deterministic-address work pays off later.

Implement and deploy the three ERC-8004 stubs from ADR 0008 (`IdentityRegistry`, `ReputationRegistry`, `ValidationRegistry`) directly to Sepolia — no proxy needed; these are immutable by design.

Wire `xvision-identity::RegistryAddresses::mantle_testnet()` to real addresses. Run the existing `#[ignore]`d anvil integration tests against Sepolia. End-to-end: mint one strategy NFT through the CLI, write one reputation receipt, read it back in the dashboard.

**Exit:** "testnet ready" shipped as a small slice with no marketplace surface yet. Tick that off the list.

### Phase 4 — Identity page + generative art (2-3 weeks)

Build the identity page from the Phase 1 design. Implement the gen-art algorithm — recommended approach: **deterministic SVG from `agent_id` + manifest hash, embedded inline in `tokenURI` as a `data:` URI**. No IPFS dependency, on-chain forever, regeneratable client-side for previews before mint.

Hook the identity page to the real Sepolia mint flow from Phase 3. **Build the gallery view in this phase, not later** — that's where you find out whether the gen-art looks like garbage on a wall of 200 strategies, and it's the cheapest moment to redo the algorithm.

**Exit:** any minted strategy has a public identity page with deterministic art and on-chain provenance.

### Phase 5 — Marketplace contracts + Rust crate (3-4 weeks)

Extend the Foundry project with `ListingRegistry`, `Marketplace`, `LicenseToken`, `EvalAttestationRegistry`. UUPS proxy shape with **admin = operator EOA** (no timelock, no multisig — those land at V4 per project decision). Deploy via the CREATE2 factory at `keccak256("xvn.<name>.v1")` salts so mainnet addresses are predictable later.

Atomically: `LicenseToken.setAuthorized(Marketplace, true)`. Run `RegisterPlatformAgent.s.sol` to mint xvn itself as agent #0.

Build `xvision-marketplace` Rust crate alongside:
- `AnchorDriver` trait + `MockDriver` (for tests) + `Erc8004MantleDriver` (for real writes)
- Orchestration verbs: `publish_listing`, `buy_listing`, `attest_eval`, `revoke_listing`
- Bundle hashing in `xvision-engine`
- ValidationRegistry writes from `xvision-execution` after closed paper trades
- ReputationRegistry writes per cycle
- CLI verbs (`xvn marketplace publish | buy | attest | list`, `xvn admin register-platform-agent`)

**Exit:** end-to-end testnet trade → seal → publish → buy → license verified, driven from CLI and from `xvision-marketplace` programmatically.

### Phase 6 — Marketplace UI (2-3 weeks)

Wallet-connect in Settings → Marketplace (the user-facing Persona-B opt-in). Marketplace tab: browse, listing detail (which links to the identity page from Phase 4 — that's the payoff of building identity first). Buy flow. Attestation viewer. Reputation leaderboard. **Testnet labeling enforced everywhere a chain action appears.**

**Exit:** a non-operator user can connect a wallet, browse listings, buy on testnet, and verify the license — all through the dashboard, no CLI required.

### Phase 7 — UI layout pass across everything (1-2 weeks)

Apply the Phase 1 layout pass to every page: strategies, agents, scenarios, evals, live charts, settings, docs, marketplace, identity. Polish at the end so inconsistencies surface in aggregate and can be fixed in one sweep, not piecemeal.

This phase is also where the popup audit lives — per `CLAUDE.md`'s no-popups rule, several existing surfaces use `Dialog`/`Modal`/`Sheet`/`Popover` and need migrating.

**Exit:** UI/UX review passes on desktop and mobile. Empty states normalized. Loading/error states consistent.

### V2 EXIT GATE

Everything above complete. Then:

### Phase 8 — V3 prep + V4 prep, in parallel

**V3 track (autoresearcher):** start the mutation-lineage-gate-seal loop per [`autoresearcher-1-mutator-lineage-gate-seal.md`](./2026-05-09-autoresearcher-1-mutator-lineage-gate-seal.md). The chain coupling is light by design — autoresearcher core is chain-free, the marketplace plugin reads its artifacts. Runs concurrently with V4 prep.

**V4 track (real money):**
- Engage an audit firm immediately (4-8 week lead times — start the conversation the day V2 ships).
- Pick the 2-of-3 multisig signers (founder + ops + community trustee — third still TBD per surface spec §11).
- Implement timelock + multisig deploy scripts.
- Decide: is Alpaca live enabled at V4 cutover, or is Alpaca paper the indefinite safe default? Config flip is small; operational decision is large.
- Define testnet → mainnet migration scope (which identities, listings, ratings, receipts get migrated / reissued / discarded).

---

## 4. Open questions still needing direction

Organized by category. Each item is something that should be decided before its dependent phase starts.

### A. Design decisions (Phase 1 inputs)

Most A-block items resolved 2026-05-26 in [`2026-05-26-marketplace-design-direction.md`](./2026-05-26-marketplace-design-direction.md) §7. Status tracked here:

| # | Question | Status |
|---|---|---|
| A1 | **Marketplace UX:** fee surfacing, Tier A vs B, x402, free listings, transferable licenses. | **Resolved** (direction doc §7). Inline fee display, `Open` badge for Tier A, `🤖 x402` badge, free listings == Tier A treatment, transferable licenses deferred (default non-transferable for V2). |
| A2 | **Identity page scope:** what's on it, public vs dashboard route, operator vs buyer split, agent #0 (xvn) page. | **Resolved** (direction doc §4.2, §5.1). Public route `/marketplace/lineage/<name>`. Operator identity stays in self-hosted XVN Settings. Agent #0 uses creator-profile template. |
| A3 | **Generative art approach.** | **Resolved** (direction doc §6.2 Tier 0). Deterministic SVG from `agent_id + manifestHash`, embedded `data:` URI in `tokenURI`, per-variant, lineage-coherent palette. Evolution-with-reputation deferred. |
| A4 | **Lineage NFT vs strategy NFT conflict.** | **Resolved** (direction doc §7 + this nav). **One NFT per lineage** wins. Variants are content-hash records under the lineage NFT. Surface spec needs the terminology amendment (see §8 below). |
| A5 | **Settings → Marketplace wallet-connect UX.** | Still open. Privy vs WalletConnect vs MetaMask. Browse stays walletless; wallet required only at mint/purchase. Decide during Phase 1 mockup pass. |
| A6 | **Public viewer + domain pick.** (New as of 2026-05-26.) Direction doc §6.1 recommends a thin read-only viewer at a public domain (`xvn.market` / `xvision.dev` / TBD). Fallback Option B is IPFS-gateway-only with reduced virality. | Open. Decision required before Phase 4 (identity page goes live). |
| A7 | **Sealed-bundle relay host.** (New as of 2026-05-26.) Direction doc §6.2 Tier 2 specifies a small relay that verifies `LicenseToken.balanceOf(buyer) ≥ 1` and signs decryption authorizations. Where does it run — operator-hosted alongside the viewer, or distributed? | Open. Affects Phase 5 (Tier B sealed listings). |

### B. Hard load-bearing assumptions (Phase 0 probes)

Per [`non-custodial-agent-wallets-design.md`](../specs/2026-05-09-non-custodial-agent-wallets-design.md) §1.1, these *must* be verified before code is written. Plan-stage probes only.

| # | Assumption | If false |
|---|---|---|
| B1 | Orderly's `add_orderly_key` supports a permission scope that includes order placement but excludes vault withdrawal. | Entire non-custodial security model collapses. Need Safe + custom session-key contract, or deposit-only-working-capital mode. |
| B2 | Orderly supports per-position isolated-margin mode in addition to default cross-margin. | Cross-margin contagion risk unmitigatable; per-strategy hard caps degrade to "intentional-overallocation defense only." |
| B3 | USDC.e on Mantle supports `transferWithAuthorization` (EIP-3009). | x402 buy path needs fallback to Permit2 or two-tx approve+buy. |

### C. Infrastructure / vendor picks (Phase 3 inputs)

| # | Choice | Notes |
|---|---|---|
| C1 | **IPFS pinning architecture** | **Resolved 2026-05-26** in [direction doc §6.2.1](./2026-05-26-marketplace-design-direction.md) + §7. Three-tier: install-mesh (every XVN install embeds an `iroh` node, default-pins its own listings + licenses) + viewer gateway (cold-traffic speed) + paid backstop (Pinata/equivalent, funded from marketplace fees). Library: `iroh` Rust-side, `Helia` browser-side, behind an `IpfsStore` trait. |
| C2 | **Subgraph host** | The Graph hosted / decentralized / Goldsky / Alchemy / self-host. Decision affects platform manifest URL. |
| C3 | **`xvision.dev` domain** | Not provisioned. Used in platform manifest schema URL. Pin to IPFS for v1 if not ready by mainnet. |
| C4 | **EAS on Mantle vs bespoke `EvalAttestationRegistry`** | If canonical Ethereum Attestation Service is deployed on Mantle, prefer EAS for tooling compatibility. Default plan is bespoke. |
| C5 | **Audit firm** | Trail of Bits / OpenZeppelin / equivalent. 4-8 week lead time. Pick during Phase 0 so it's lined up by Phase 8. |
| C6 | **Faucet ops** | Who pre-funds the operator wallet on Sepolia, from where, how often. Marketplace-plugin spec recommends 5× estimated chain footprint. |
| C7 | **`iroh` integration timing** | V2 ships with only a `PinataDriver` implementation of `IpfsStore` (backstop tier only) and swaps in `IrohDriver` at V3 once install base justifies it, vs `iroh` from V2 day one. Recommendation in [direction doc §8.10](./2026-05-26-marketplace-design-direction.md): defer to V3; ship V2 with the `IpfsStore` trait so the swap is mechanical. Confirm in Phase 5 planning. |

### D. Mainnet / governance TBDs (Phase 8 V4 track)

| # | Question | Source |
|---|---|---|
| D1 | **Multisig 3rd signer** ("community trustee") identity | Surface spec §11 |
| D2 | **Fee recipient address at launch** — placeholder until treasury multisig deployed | Surface spec §11 |
| D3 | **Testnet → mainnet migration scope** — which identities, strategies, ratings, receipts get migrated vs reissued vs discarded | V2-V4 action plan §V4 |
| D4 | **Alpaca live enable at V4 cutover?** — config flip is small, operational decision is large | This doc |

### E. Edge case flows not yet designed

| # | Flow | Notes |
|---|---|---|
| E1 | **Tier B sealed-bundle delivery** | Spec says fetch is gated by `LicenseToken.balanceOf >= 1` plus device fingerprint + signature freshness. The fingerprint/freshness piece is mentioned but not designed — what's the fingerprint, how is it issued, what's the freshness window, what happens on device migration. |
| E2 | **License revocation flow** | If a strategy turns out malicious, what happens to existing licenses? Currently undefined. |
| E3 | **Refund flow** | Explicitly out-of-scope on-chain. Operator process for off-chain refunds is undefined. |
| E4 | **Attribution ledger UX** | Off-chain `agent_id → realized PnL + funding` ledger is specced but has no dashboard surface. Operators will want this view. |
| E5 | **Settlement wallet operations** | The 5% drips accumulate somewhere. Who sweeps, how often, into what treasury? |

---

## 5. Hard rules (carry forward)

- **No timelock on testnet.** UUPS proxy admin = operator EOA. Timelock + multisig land at Phase 8 / V4 prep, never sooner. This is a deliberate departure from the surface spec's day-one timelock recommendation, traded for testnet iteration speed.
- **Same CREATE2 salts on testnet and mainnet.** Nonce-0 EOA deployed identically; XvnDeployer at the same address on both chains; same `keccak256("xvn.<name>.v1")` salts. Mainnet addresses are predictable from Phase 3 onward.
- **Testnet labeling everywhere.** Every chain action in UI / API / logs is tagged testnet. V2C exit gate — non-negotiable.
- **Alpaca paper + Bybit paper stay the safe defaults** even after V2C testnet flows ship. Chain rail is additive, not a replacement.
- **No-popups UI rule** ([per `CLAUDE.md`](../../../CLAUDE.md)) applies to all new marketplace + identity surfaces. Confirmations, detail views, settings — everything routes, docks, rails, or inline-expands.
- **Deploy-host no-Cargo rule** ([per `CLAUDE.md`](../../../CLAUDE.md)) — Foundry deploys + verification happen on the local build host or CI, never on the small VPS / Coolify nodes.

---

## 6. Source material map

Existing specs and ADRs that inform this plan. **Each is the authoritative source for its slice; this doc only orders and sequences.**

| Source | Disposition |
|---|---|
| [V2-V4 action plan](./2026-05-13-v2-v4-action-plan.md) | Broader roadmap; this doc is the blockchain-slice navigation inside it. |
| [Marketplace + identity design direction](./2026-05-26-marketplace-design-direction.md) | Authoritative direction for marketplace UX, identity page scope, public/private storage split, gen-art approach, and the public viewer architecture. Feeds Phase 1 mockup pass. |
| [Smart contract surface design](../specs/2026-05-08-smart-contract-surface-design.md) | Authoritative spec for the marketplace contract surface. Amended 2026-05-26 with lineage NFT terminology fix per direction doc §7 (A4). Open questions §11 feed into §4 above. |
| [Non-custodial agent wallets design](../specs/2026-05-09-non-custodial-agent-wallets-design.md) | Authoritative spec for the trading rail. Validation gates §1.1 feed into §4.B above. |
| [Marketplace plugin design](../specs/2026-05-09-marketplace-plugin-design.md) | Plugin-level spec. Amended 2026-05-26 to clarify that its §7 dashboard tab is the Persona A operator surface; Persona B public marketplace UX is owned by the direction doc. Lineage NFT decision is now canonical across both specs (A4 resolved). |
| [Karpathy autoresearcher design](../specs/2026-05-09-karpathy-autoresearcher-design.md) | V3 source of truth. Chain-free core; plugin reads its `CycleSeal` artifacts. |
| [ADR 0008 ERC-8004 deployment](../../../decisions/0008-erc8004-deployment.md) | Phase 3 deploy runbook. |
| [Blockchain-1 non-custodial wallets plan](./2026-05-10-blockchain-1-non-custodial-wallets-plan.md) + [amendments](./2026-05-10-blockchain-1-non-custodial-wallets-amendments.md) | Phase 0 probe sources for B1-B2. |
| [`crates/xvision-identity/`](../../../crates/xvision-identity/) | Existing stub crate; expand in Phase 3. |
| [ERC-8004 agent uses](../../erc-8004-agent-uses.md) | Background research on the standard. |

---

## 7. Phase summary (one-pager)

```
Phase 0  Harden existing                   1-2 wks   tests for A/B, multi-asset, paper
Phase 1  Design sprint                     1 wk      marketplace + identity + gen-art + layout
Phase 2  Bybit provider                    2-3 wks   crypto paper rail (parallel with Ph1)
Phase 3  Foundry + ERC-8004 testnet ready  2 wks     XvnDeployer + 3 stub registries on Sepolia
Phase 4  Identity page + gen-art           2-3 wks   public per-strategy homepage
Phase 5  Marketplace contracts + crate     3-4 wks   4 marketplace contracts + xvision-marketplace
Phase 6  Marketplace UI                    2-3 wks   browse / detail / buy / wallet connect
Phase 7  UI layout pass                    1-2 wks   polish all pages
────────────────────────────────────────────────────────────────────────────────
V2 EXIT GATE
────────────────────────────────────────────────────────────────────────────────
Phase 8  V3 prep + V4 prep (parallel)      ongoing   autoresearcher loop / audit + mainnet prep
```

Total time-to-V2-exit: **~14-21 weeks** at single-developer pace, less with parallelization on Phases 1-2 and Phases 5-6.

---

## 8. Maintenance

- **Update §4 (open questions) as decisions are made.** Move resolved items to the relevant spec or to a new ADR; leave a one-liner here pointing at it.
- **Update §1 (current state) when a phase exits.** Strike-through what's shipped; new state replaces stale state.
- **Don't re-litigate decisions logged in linked specs here.** This doc orders; the specs decide.

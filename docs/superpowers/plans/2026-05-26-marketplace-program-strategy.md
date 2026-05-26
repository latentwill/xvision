# Marketplace Program — Strategy & Front Door

> **Purpose:** One front door for the marketplace build. The marketplace UI is
> the *face* of a stack that runs underneath it (ERC-8004 identity, marketplace
> contracts, a Rust crate, a subgraph, IPFS pinning, a non-custodial trading
> rail). Six design/spec docs describe pieces of that stack at different dates
> and different levels of staleness. This doc **reconciles them, audits what's
> out of date, sets the build order (front-end first), and names the first
> spec to write.**
>
> **Created:** 2026-05-26. **Anchored on:** the phase spine in
> [`2026-05-26-blockchain-plan-navigation.md`](./2026-05-26-blockchain-plan-navigation.md).
> This doc does not replace that nav doc — it elaborates it into an executable
> program, reorders it front-end-first per operator direction, and corrects two
> overstatements (gen-art status; older-spec staleness).
>
> **Status:** Strategy proposed; pending operator review. When approved, the
> first executable artifact is the **Phase F frontend spec** (§6).

---

## 0. How to read this doc

- **§1** — the six docs and who owns what (authority map). Read first.
- **§2** — what's stale in the two older contract specs, with fixes. (The thing
  the operator explicitly asked to be reviewed.)
- **§3** — decisions this strategy bakes in.
- **§4** — the full reconciled phase plan (Phase 0 → 8).
- **§5** — critical path + parallel tracks.
- **§6** — **where we start:** Phase F (frontend on fixtures), broken into
  buildable sub-steps. This is the next spec.
- **§7** — consolidated open-decisions register, by phase.
- **§8** — ordering + delegation notes.
- **§9** — immediate next action.

---

## 1. The doc set & authority map

Six documents inform this program. Each is authoritative for its slice; this
strategy only orders and reconciles. **Do not re-litigate a decision logged in
the owning spec — fix it there.**

| Doc | Date | Owns (authoritative for) | State |
|---|---|---|---|
| **Blockchain nav** [`…blockchain-plan-navigation.md`](./2026-05-26-blockchain-plan-navigation.md) | 05-26 | Phase ordering, open-question tracking, hard rules | **Live spine.** This strategy elaborates it. |
| **Marketplace design direction** [`…marketplace-design-direction.md`](./2026-05-26-marketplace-design-direction.md) | 05-26 | Persona B buyer/seller **UX**, storage tiers, public-viewer architecture, custody framing, routes not covered by the hifi handoff (`/marketplace/sell`, `/`, leaderboard semantics) | Live. Direction set. |
| **Marketplace design handoff** `docs/design/design_handoff_marketplace_shift/` | 05-26 | Hi-fi **mockups + component spec** for 6 frames: browse, creator, identity closed/open, receipt, OG card | Live. This is Phase 1's "locked mockups" output for those frames only. |
| **Smart-contract surface** [`…smart-contract-surface-design.md`](../specs/2026-05-08-smart-contract-surface-design.md) | 05-08 | On-chain contracts: `ListingRegistry`, `Marketplace`, `LicenseToken`, `EvalAttestationRegistry`, x402, fees, subgraph schema | Accepted; **body has stale conflicts — see §2.** |
| **Marketplace plugin** [`…marketplace-plugin-design.md`](../specs/2026-05-09-marketplace-plugin-design.md) | 05-09 | The Rust crate: lineage-NFT mint, Merkle receipts, in-house attesters, **Persona A operator** surface, CLI | Accepted; **hackathon timeline dead — see §2.** |
| **Non-custodial wallets** [`…non-custodial-agent-wallets-design.md`](../specs/2026-05-09-non-custodial-agent-wallets-design.md) | 05-09 | The **trading rail**: scoped Orderly keys, per-strategy budgets, attribution ledger, kill switches | Draft; **inherits timelock staleness — see §2.** |

Plus: [ADR 0008](../../../decisions/0008-erc8004-deployment.md) (Phase-3 deploy
runbook) and the [testnet venue plan](./2026-05-25-testnet-venue-plan.md)
(Bybit/Orderly testnet detail behind nav Phase 2).

**Two personas, two surfaces — never merge them:**

- **Persona A (operator)** — runs their own XVN, does chain ops (mint / anchor /
  attest). Lives in **self-hosted XVN → Settings → Chain ops**. Owned by the
  plugin spec §7; implemented explicitly in Phase 5/6, not implied by the
  public marketplace work.
- **Persona B (buyer/seller)** — browses, buys, clones, shares. Lives in the
  **public marketplace surfaces** (handoff frames + direction-doc route
  additions). Owned by the direction doc.

---

## 2. Spec staleness audit — fix before building

The two older contract specs (and the non-custodial wallet spec) carried
"Amended 2026-05-26" notes at the top, **but the amendments had not propagated
into the bodies, decision logs, or sequencing sections.** An engineer reading
them as build instructions would have shipped the wrong thing. The fixes below
have been applied to the owning specs; this section remains as the audit trail.

### 2.1 Hard conflicts (would cause wrong implementation)

| # | Where | Stale claim | Correct position | Source |
|---|---|---|---|---|
| H1 | Contract surface §1, §2, §7.2–7.4, §12 decision-log line 786; non-custodial §3.7, §5, §10 | "UUPS proxy behind **7-day timelock + 2-of-3 multisig**" as the **v1** admin model (≥6 occurrences) | **Testnet (all of V2): admin = operator EOA, no timelock, no multisig.** Timelock + multisig land at **Phase 8 / V4 prep only.** | Nav §5 hard rule |
| H2 | Plugin §2 decision #6; §9 schedule (6/11 cutover); §9 milestones; §10 failure-mode #1 | "**Mantle mainnet for the submission**," dated mainnet-cutover schedule, mainnet as primary target | **Testnet-only (Mantle Sepolia) through all of V2.** Mainnet is a separate gated **V4** launch after the V2 exit gate. | Nav §2, §5 |
| H3 | Contract surface §3.1.1 line 135 ("tokenURI renders deterministic SVG…") | Gen-art treated as a **resolved implementation fact** | **Gen-art is unscoped.** No algorithm designed. nav A3 / direction §6.2 **overstate** "resolved." It is a real design task (this strategy: **Phase 4**). Until then, **frontend uses a placeholder.** | Operator, 2026-05-26 |
| H4 | Plugin §7 panel 4 (line 229); non-custodial §3.4 (line 249) | "gated behind a confirmation **modal**" / "confirm **modal**" | **No popups** (CLAUDE.md). Confirmations route / dock / inline-expand. | CLAUDE.md |
| H5 | Design handoff README "Generative art — implementation note"; `bc2-genart.jsx` | Treats the prototype `GenArt` algorithm as production-canonical and contract-bound | **Prototype art is visual reference only until Phase 4.** Phase F ships `GenArtPlaceholder`; Phase 4 locks the real byte-stable algorithm, palette, family rules, encoding, and sample wall before any `tokenURI` deployment. | H3 |
| H6 | Direction §6.4 | Says the perpetual license is held as a non-transferable/transferable **ERC-721** token | Contract surface is **`LicenseToken` ERC-1155**, soulbound by default with transferable opt-in per listing. Keep ERC-1155 unless the contract surface is deliberately reopened. | Contract surface §3.3 |

### 2.2 Soft-stale (correct intent, wrong framing / resolved elsewhere)

| # | Where | Stale claim | Correct position |
|---|---|---|---|
| S1 | Plugin §11 Q1 | "IPFS pinning provider — Pinata vs Web3.Storage vs … resolve Wk 5 day 1" (open) | **Resolved** (nav §4 C1): three-tier = `iroh` install-mesh + viewer gateway + Pinata backstop; **iroh deferred to V3**; V2 ships **Pinata-only behind an `IpfsStore` trait.** |
| S2 | Plugin §3.2 `AnchorDriver` trait | `mint_lineage_nft(… manifest_cid: Cid …)` takes a raw CID | Must thread through an **`IpfsStore` abstraction** (S1) and the inputs the metadata spec defines (incl. gen-art seed once scoped). |
| S3 | Contract surface §3.5 / §4.2 (`api.xvn.dev`, "xvn API (resource server)") | Implies a **central API/website** | **No central API.** Thin, read-only, forkable viewer at a TBD domain (direction §6.1). x402 resource-server host is **open** (A7). |
| S4 | Contract surface §6.3 subgraph entity named `Strategy` | Entity is really the **lineage** | Rename to `Lineage` (or alias) — `agentNftId` = lineage per A4. Avoids engine-`Strategy` confusion (CLAUDE.md terminology lock). |
| S5 | Both specs: "gated on Strategy Creation Engine + Eval Engine being shipped," "hackathon team," `v2` = post-hackathon | Status lines now false; `v2`≠project `V2` | Engines exist. Drop hackathon framing. Disambiguate every `v2` (means **post-hackathon**, not the project's V2 testnet wave). |
| S6 | Contract surface §8.4 config (`fee_recipient = '0x…' # multisig`, `admin_multisig`, `timelock`) | Config assumes treasury multisig + timelock exist at launch | Single operator EOA recipient for V2; multisig/timelock fields are **V4 (D1/D2)**. |
| S7 | Handoff interaction notes: Follow, Tip, Save view | Adds extra social/product actions without contracts, storage, or ownership | Treat as **deferred affordances** in Phase F unless a local-only fixture implementation is explicitly useful. Do not let these actions expand Phase F. |

### 2.3 Data shapes to reconcile in the metadata spec (Phase 1 close-out)

These are the same artifacts described with divergent fields. The **Metadata &
Data-Contract spec** must unify them into one canonical schema:

- **IPFS metadata behind `agentURI`**: plugin `LineageManifest` (6 fields:
  `lineage_id`, `initial_bundle_hash`, `parent_lineage_id`, `born_at`,
  `operator_signature`, `autoresearch_session_id`) **vs.** direction §6.2 Tier 1
  (name, description, perf summary with on-chain hash commit, asset/model/style
  tags, equity-curve file, **required-ingredients list**, license terms, rating
  receipts). → unify.
- **On-chain events / subgraph** (contract surface §3, §6.3): `ListingCreated`,
  `ListingUpdated`, `ListingRevoked`, `Sold`, `AttestationPosted`, +
  `LicenseToken` transfers. These are the read-path for browse + creator
  profile. The frontend seam (§6) must shape its queries to these.
- **Listing struct** (contract surface §3.1): `tier` (0=Open/1=Sealed),
  `priceUSDC`, `transferableLicense` (default false per direction), etc.
- **Purchase / license event shape**: the UI depends on "humans + agents"
  counts everywhere, but bare `LicenseToken` transfers do not encode payer
  class. The schema must expose `payer_kind = Human | Agent`, `purchase_path =
  direct | x402`, and fee split fields from a marketplace event or indexed
  transaction context.
- **Publish/write shape**: the seller flow (§6 F5) needs canonical fields for
  listability checks, required ingredients, verification evidence, pricing,
  accepted payers, and license terms. Do not derive the metadata spec only from
  buyer read screens.
- **Handle and verification shape**: creator resolution (`@handle` vs address)
  and green/gray verification thresholds must become explicit fields or derived
  rules before contracts/subgraph work begins.

---

## 3. Decisions baked into this strategy

| # | Decision | Rationale / source |
|---|---|---|
| D-1 | **Front-end first.** Build the marketplace experience in the existing Vite SPA against a typed `MarketplaceData` seam + fixtures, before any chain backend exists. | Operator direction. Cheapest design validation; no chain dependency; the seam's types prototype the data contract. |
| D-2 | **Gen-art is a placeholder until Phase 4.** Frontend renders a deterministic, seed-keyed `GenArtPlaceholder` with a clean swap-point. | H3. Gen-art is unscoped; do not block the UI on it. |
| D-3 | **Schema preflight before frontend; metadata close-out after frontend.** Start Phase F with a thin seam preflight (IDs, core read/write shapes, route params, fixture keys), then harden the proven seam into the **Metadata & Data-Contract spec** before contracts begin. | Keeps fixtures from inventing impossible shapes while preserving the frontend-first learning loop. |
| D-4 | **Testnet-only through V2; EOA admin; no timelock/multisig.** | H1, H2, nav §5. |
| D-5 | **One NFT per lineage; variants are content-hash records; clone = new lineage NFT with `parent_lineage_id`.** | A4 resolved; both contract specs amended. |
| D-6 | **Non-custodial, self-hosted, no central write API or user database.** A thin forkable read-only viewer/gateway may be hosted for public URLs and OG cards. Fixed USDC, perpetual license, default non-transferable. | Direction §2, §6.1, §6.4; S3. |
| D-7 | **Persona A (Settings → Chain ops) and Persona B (public marketplace) stay separate surfaces.** | Plugin §7 amendment. |
| D-8 | **Embedded-SPA first; public viewer + OG-card SSR is a defined Phase-6 follow-on.** Domain pick (A6) gates it. | Reuses Signal theme/primitives; runnable now. |
| D-9 | **Phase F includes the seller write path.** The hifi handoff has six frames, but the program needs `/marketplace/sell` as a fixture-backed inline flow so publish metadata, listability gates, and Tier A/B behavior are exercised before the metadata spec closes. | Direction §5.5 / §5.5.1. |

---

## 4. The reconciled phase plan

Phases keep the nav doc's numbering where they map cleanly, with a new **Phase F**
inserted as the front-end-first starting point and **gen-art moved into Phase 4**
as real design. Time estimates are single-developer, parallelizable where noted.

### Phase 0 — Harden existing (parallel, ongoing) · 1–2 wks
**Goal:** regression suite pins A/B compare, multi-asset, Alpaca paper ordering
so downstream chain work doesn't silently regress the engine/broker layer.
**Deliverable:** integration tests on all three; CI fails on regression.
**Note:** orthogonal to the marketplace front — runs in the background, does
**not** gate Phase F.
**Exit:** three flows test-pinned in CI.

### Phase F — Marketplace frontend on fixtures (START HERE) · 3–4 wks
**Goal:** the hifi buyer/seller frames plus the missing route surfaces needed
to exercise the product loop, fully demoable in the SPA, reading through a
typed `MarketplaceData` seam backed by fixtures.
**Deliverables:** see §6 (F0–F8). Gen-art placeholders (D-2). OG card as a
component (SSR deferred). Routes under `/marketplace/*`. Seller publish flow
included as a fixture-backed inline flow, because it defines the metadata write
shape.
**Dependencies:** none external. Reuses `tokens.css` (Signal theme), existing
primitives (Card, Pill, Badge, Icon, BrandMark), charts v2 (`MiniSparkline`,
`HeroGradientEquity`), existing clone-to-edit (`POST /api/strategy/:id/clone`).
**Exit:** all hifi frames plus `/marketplace/leaderboard` and
`/marketplace/sell` render from fixtures; gen-art wall demo (~200) renders; OG
card composes; read + write seam types are reviewed at scale.

### Phase 1 (close-out) — Metadata & Data-Contract spec · ~1 wk
**Goal:** harden the Phase-F seam types into the canonical schema (D-3).
Reconcile the divergent data shapes (§2.3). One page each: Tier 0 (`tokenURI`
JSON + how gen-art will be encoded once scoped), Tier 1 (IPFS metadata), Tier 2
(sealed bundle), listing/publish inputs, and the chain events the subgraph
indexes.
**Dependency:** Phase F seam preflight + implemented surfaces (the UI reveals
the field set).
**Exit:** metadata spec written + reviewed; it becomes the contract both the
frontend seam and the contracts/subgraph satisfy.

### Phase 2 — Bybit testnet provider (parallel) · 2–3 wks
**Goal:** crypto paper/testnet rail; stress the broker abstraction before a third
venue. Per the [testnet venue plan](./2026-05-25-testnet-venue-plan.md) T0–T5.
**Dependency:** independent of contracts; runs concurrent with Phase F/1.
**Exit:** the same strategy runs on Alpaca paper and Bybit testnet unchanged at
the strategy layer; hermetic broker-contract tests pass.

### Phase 3 — Foundry + ERC-8004 testnet ready · 2 wks
**Goal:** `contracts/` Foundry tree; mint the **nonce-0 EOA**; deploy
`XvnDeployer` (CREATE2); deploy the 3 ERC-8004 stubs (`IdentityRegistry`,
`ReputationRegistry`, `ValidationRegistry`) to **Sepolia**; wire
`xvision-identity::RegistryAddresses::mantle_testnet()` to real addresses;
un-`#[ignore]` the anvil integration tests against Sepolia.
**Hard rules:** same CREATE2 salts as future mainnet; Foundry deploys never on a
VPS; testnet labels everywhere.
**Dependency:** ADR 0008 runbook. Independent of Phase F.
**Exit:** mint one strategy NFT via CLI, write one reputation receipt, read it
back. "Testnet ready" shipped.

### Phase 4 — Generative art (real design) + identity wiring · 2–3 wks
**Goal:** **scope the gen-art algorithm** (the unscoped piece — H3). Lock input
space, palette set, family rules, lineage-coherence; produce a 200-strategy
sample wall; replace the Phase-F `GenArtPlaceholder` with the real generator;
embed it as `data:` URI in `tokenURI`; wire the identity page to the Sepolia
mint flow from Phase 3.
**Dependency:** Phase 3 (mint flow), Phase 1 (Tier 0 encoding), Phase F (the
swap-point + the wall harness).
**Exit:** any minted strategy has a public identity page with deterministic art
and on-chain provenance; the art holds up on a wall of 200.

### Phase 5 — Marketplace contracts + `xvision-marketplace` crate · 3–4 wks
**Goal:** the 4 marketplace contracts (`ListingRegistry`, `Marketplace`,
`LicenseToken` ERC-1155, `EvalAttestationRegistry`) — UUPS with **EOA admin**
(H1), CREATE2 salts; the `xvision-marketplace` crate (`AnchorDriver` +
`IpfsStore` traits, `MockDriver`, `Erc8004MantleDriver`, `PinataDriver`;
orchestration verbs; CLI); the subgraph (entity `Lineage`, not `Strategy` —
S4); Pinata pinning behind `IpfsStore`; and the Persona-A operator route
contract/API shape for Settings → Chain ops.
**Prerequisite:** **apply the §2 fixes to the contract specs first** so the
implementation follows corrected specs.
**Dependency:** Phase 3 (factory + registries), Phase 1 (metadata schema).
**Exit:** end-to-end testnet: trade → seal → publish → buy → license verified,
from CLI and programmatically; operator can inspect mint/anchor/attester state
through an API shape that the Phase-6 Settings surface can render.

### Phase 6 — Wire frontend to real backends + wallet + notifications · 2–3 wks
**Goal:** swap the Phase-F fixture impl of `MarketplaceData` for real
implementations (subgraph + IPFS gateway + local XVN HTTP API). Wallet-connect
in Settings → Marketplace (A5). Settings → Chain ops for Persona A (mint /
anchor / attester status, tx history, inline confirmations). Chain-native
purchase notifications (XVN listens for `LicenseToken` events on its own
address; share composer). Sealed-bundle relay (A7). Decision on standalone
public viewer + OG-card SSR (A6/domain).
**Dependency:** Phase F (the surfaces), Phase 1 (schema), Phase 4 (real art +
identity wiring), Phase 5 (contracts/subgraph).
**Exit:** a non-operator connects a wallet, browses, buys on testnet, verifies
the license — through the dashboard, no CLI.

### Phase 7 — UI layout pass + popup audit · 1–2 wks
**Goal:** apply layout standards across all pages; migrate every existing
`Dialog`/`Modal`/`Sheet`/`Popover` (incl. the stale modal refs in H4) to
route/dock/rail/inline per the no-popups rule.
**Exit:** UI review passes desktop + mobile; empty/loading/error states
normalized.

### — V2 EXIT GATE —

### Phase 8 — V3 + V4 prep (parallel) · ongoing
- **V3 (autoresearcher):** the mutation→lineage→gate→seal loop; chain-free core,
  marketplace plugin reads its `CycleSeal` artifacts.
- **V4 (real money):** engage audit firm (4–8 wk lead — start day V2 ships);
  pick 2-of-3 multisig signers (D1); implement **timelock + multisig** (now,
  not before — H1); decide Alpaca-live cutover (D4); define testnet→mainnet
  migration scope (D3). **The non-custodial Orderly trading rail
  ([spec](../specs/2026-05-09-non-custodial-agent-wallets-design.md)) lands on
  this track**, gated on the B1/B2 probes (§7).

---

## 5. Critical path & parallel tracks

```
START
  │
  ├─► Phase F  Seam preflight + frontend on fixtures ─► Phase 1  Metadata spec
  │     (seller write path + buyer read surfaces;        (canonical schema)
  │      gen-art = placeholder)                              │
  │                                                          ├─► Phase 4
  └─► Phase 3  Foundry + ERC-8004 testnet ───────────────────┤   Gen-art +
                                                             │   identity
                                                             │
                                                             └─► Phase 5
                                                                 Marketplace
                                                                 contracts +
                                                                 crate +
                                                                 subgraph
                                                                  │
                         Phase 4 + Phase 5 both complete ─────────┘
                                                                  ▼
Phase 6  Wire frontend seam → real backends + wallet + notifications
   │
   ▼
Phase 7  Layout pass + popup audit ──► V2 EXIT ──► Phase 8 (V3 ∥ V4)

PARALLEL, anytime:  Phase 0 (harden existing)   ·   Phase 2 (Bybit testnet)
DEFERRED to V4:     non-custodial Orderly trading rail (probe B1/B2 early, cheap)
```

**The unlock:** the frontend does not need the chain. Phase F delivers a
demoable marketplace and validates the design at scale immediately; the chain
tracks (3→5) proceed against the same schema and get wired in at Phase 6 by
swapping one implementation of the seam.

---

## 6. Where we start — Phase F (the next spec)

Build the hifi handoff frames plus the missing direction-doc route surfaces in
`frontend/web/src` under `/marketplace/*`, reading through one typed seam.
Sub-steps:

- **F0 — Seam preflight + foundations.** Before rendering pages, define the
  first-pass `MarketplaceData` contract: stable IDs, route params, listing,
  lineage, creator, receipt, leaderboard, publish draft, ingredient-check,
  purchase/clone intent, and notification event shapes. Keep it thin; this is a
  guardrail, not the final metadata spec. Reconcile handoff tokens against
  `tokens.css` (they
  already match — green `--gold`, Geist/Geist Mono, 6px radius). Add the new
  primitives the handoff names: `GenArtPlaceholder` (D-2), `Sparkline`
  (reuse/extend `MiniSparkline`), `AgentIcon`, `VerifiedBadge`, `X402Badge`,
  `AssetPill`, `RemovableChip`, `FilterDrawer` (right-edge panel, **not** a
  popover), `ShareableCard`.
- **F1 — `/marketplace` browse.** Header strip, toolbar (segmented + search +
  sort + Filters), applied-filter chips, leaderboard rail, list rows, filter
  drawer. URL-synced filter state.
- **F2 — `/marketplace/lineage/<name>` identity** (closed + on-chain-receipts
  drawer-open). The viral artifact. Equity curve via charts v2; ingredient-check
  banner; lineage tree; trade-history card.
- **F3 — `/marketplace/creator/<handle>` profile.** Chain-derived shape; lineage
  forest; earnings chart; reputation feed.
- **F4 — `/marketplace/leaderboard`.** Curated slices with stable URLs. No hifi
  frame exists, so reuse browse/list-card primitives and make scope explicit in
  the Phase-F spec.
- **F5 — `/marketplace/sell` seller onboarding.** Three-step inline flow from
  direction §5.5: pick listable local strategy, choose Tier A/B + price +
  accepted payers, preview/mint. Fixture-backed listability failures must be
  typed and specific. This is required for the metadata write shape.
- **F6 — `/marketplace/receipts/<tx>` receipt.** Post-buy install steps + share
  composer.
- **F7 — Shareable OG card** (1200×630) as a composable React component (SSR
  generation deferred to Phase 6 / A6).
- **F8 — Fixture implementation + hook layer.** A
  `FixtureMarketplaceData` implementation + realistic fixtures (incl. a 200-row
  set for the gen-art wall). Hooks: `useFilterState`, `useStrategy`,
  `useCreatorProfile`, `useReceiptsDrawer`, `usePublishDraft`,
  `useIngredientCheck`, `usePurchaseIntent`, `useCloneIntent`, etc. (per
  handoff "State Management"). **The seam's types are the draft data contract
  that Phase 1 formalizes.**

**Deferred from Phase F unless explicitly pulled in:** public `/` landing page,
real wallet connect, real tx submission, real SSR OG generation, Follow, Tip,
Save view persistence, and production gen-art.

**Constraints:** no popups (drawer = panel, receipts = inline-expand, seller flow
= inline). Testnet labeling on anything that will become a chain action.
Gen-art = placeholder. Reuse existing primitives; don't fork the theme. Treat
the handoff's `bc2-genart.jsx` as visual reference only until Phase 4.

A dedicated **Phase F implementation spec** (the brainstorming → writing-plans
output) will detail components, props, fixtures, routes, and acceptance per
sub-step. That is the immediate next document.

---

## 7. Open-decisions register (by phase)

Consolidated from nav §4 + the surface spec §11. Each must be decided before
its dependent phase.

| ID | Decision | Needed by | Status |
|---|---|---|---|
| A5 | Wallet-connect UX (Privy / WalletConnect / MetaMask) | Phase 6 | Open |
| A6 | Public-viewer domain + whether to stand up SSR viewer | Phase 6 (Phase 4 for shareable URLs) | Open |
| A7 | Sealed-bundle decryption relay host | Phase 5/6 (Tier B) | Open |
| A8 | Handle resolution (`@handle` via ENS / on-chain registry / `agentURI` display name / address fallback) | Phase F preflight / Phase 1 | Open |
| A9 | Verification badge threshold (green vs gray) | Phase F preflight / Phase 1 | Open; suggested = backtest + ≥30d live-paper + ≥1 positive closed cycle hash |
| A10 | Clone semantics for Tier B sealed listings (license check timing, parent-edge write authority) | Phase 1 / Phase 5 | Direction says purchase first; contract check still needs locking |
| A11 | Notification UX inside self-hosted XVN (toast, sidebar feed, share composer dock) | Phase 6 | Open; no modal-equivalent |
| A12 | Public landing `/` for cold visitors | Phase 6 / public viewer | Open; explicitly deferred from Phase F |
| B1 | Orderly trading-only key scope (no withdraw) | non-custodial rail (V4) | Probe early — cheap |
| B2 | Orderly per-position isolated margin | non-custodial rail (V4) | Probe early — cheap |
| B3 | USDC.e on Mantle supports EIP-3009 `transferWithAuthorization` | Phase 5 (x402 buy) | Open — verify before contract finalize |
| C2 | Subgraph host (Goldsky / The Graph / Alchemy) | Phase 5 | Open |
| C3 | `xvision.dev`/`xvn.market` domain provisioning | Phase 4/6 | Open (= A6) |
| C7 | `iroh` integration timing | Phase 5 | Resolved: defer to V3; V2 = Pinata behind `IpfsStore` |
| D1 | Multisig 3rd signer | Phase 8 / V4 | Open |
| D2 | Fee recipient address at launch | Phase 8 / V4 | Open (EOA for V2) |
| D3 | Testnet→mainnet migration scope | Phase 8 / V4 | Open |
| D4 | Alpaca live at V4 cutover | Phase 8 / V4 | Open |
| E1–E5 | Sealed-bundle delivery fingerprint, license revocation, off-chain refund, attribution-ledger UX, settlement-wallet ops | Phase 5/6 / V4 | Open (edge flows) |
| E6 | Human-vs-agent buyer count source | Phase 1 / Phase 5 | Must be explicit in marketplace event/subgraph; `LicenseToken` transfer alone is insufficient |
| — | **Gen-art algorithm** (input space, palette, family rules, lineage-coherence) | Phase 4 | **Open — unscoped (H3)** |

### 7.1 Deferred items register (every deferral lands here → owning phase)

> **Rule:** nothing gets deferred with only a chat mention. When an item is cut,
> stubbed, or postponed, record it here against the phase that picks it up. Tick
> it off (or migrate it into the owning spec) when done. Built 2026-05-26 after
> Phase F; sourced from the F0 review + the F1–F6 route-plan open questions.

| Item | Owning phase | Source / note |
|---|---|---|
| `tier` filter on `FilterState` + Tier-A "free" slice | ✅ DONE (F-route follow-up) | F0 review M2 / F4 OQ |
| Creator profile resolves by address + ENS (not just `@handle`) | ✅ DONE (F-route follow-up) | F3 OQ |
| `publishedAt`/`mintedAt` on `ListingRow` (so `newest` sort isn't an id proxy) | Phase 1 (schema field) → frontend rewire | F1 OQ |
| `transferableLicense` surfaced on receipt/license UI (absent on `Receipt.license` today) | Phase 1 (enrich Tier-1/receipt metadata) | F6 OQ |
| Human-vs-agent payer-class source in marketplace event/subgraph | Phase 1 (schema) + Phase 5 (contract event) | E6 |
| Handle resolution (ENS / on-chain registry / `agentURI`) | Phase 1 decision | A8 |
| Verification badge thresholds (green vs gray criteria) | Phase 1 decision | A9 |
| Tier-B clone license-check semantics (gate timing) | Phase 1 (read model) + Phase 5 (contract) | A10 |
| `auditedOnly` filter has no `ListingRow` field (no-op today) | Phase 1 (schema field) | F0 review M4 |
| Equity curve real backtest+live two-layer data | Phase 4 (real chart data) | F2 OQ-1 (placeholder today) |
| Production gen-art (replace `GenArtPlaceholder`) | Phase 4 | H3 |
| Save view (persist a filter slice) | Phase 6 (needs storage) | F1 deferred affordance |
| Follow / Tip creator | Phase 6+ (on-chain follow/tip) | F3 deferred affordance |
| Decrypt-now sealed-bundle relay (the "Decrypt" step) | Phase 5/6 (relay) | F6 / A7 |
| Install-missing ingredients action (MCP/skill install) | Phase 6 (local XVN API) | F6 / F2 affordance |
| Real wallet-connect (Buy/Clone/Mint behind it) | Phase 6 | A5 |
| Real on-chain tx submission (replace fixture `TxRef`) | Phase 6 (contracts live) | spec deferred |
| Chain-native purchase notifications + share-composer wiring | Phase 6 | direction §3.3 |
| OG-card server-side PNG render | Phase 6 / public viewer | A6 |
| Public `/` landing page | Phase 6 / public viewer | spec deferred |
| Discord share-intent URL (confirm format) | Phase 6 (product decision) | F6 OQ |

---

## 8. Ordering + delegation notes

### 8.1 Ordering risks

- **Do not start contracts from old specs until §2 fixes land.** The old specs
  still call themselves authoritative; leaving stale bodies in place is the
  highest-risk implementation trap.
- **Do not let Phase F drift into a full public-viewer build.** The SPA embeds
  the surfaces first. Domain, SSR, crawler behavior, and public deployment land
  in Phase 6 after A6/A12.
- **Do not write the final metadata spec entirely up-front.** Only do the F0
  seam preflight first. The close-out spec comes after the seller/buyer fixture
  surfaces reveal the real field set.
- **Do not implement production gen-art from the handoff prototype.** It is
  visual reference until Phase 4 locks byte-stable generation and encoding.
- **Do not defer the seller flow out of Phase F.** Without `/marketplace/sell`,
  the metadata spec misses publish inputs, listability refusals, and Tier A/B
  write semantics.
- **Do not hide Persona A in "later polish."** Chain ops is separate from the
  public marketplace, but it is still required for V2 testnet operations.

### 8.2 Delegation guidance

Good parallel work:

- **Phase 0 hardening** and **Phase 2 Bybit** can run independently of the
  marketplace UI and contracts.
- **§2 staleness edits** can be delegated as a narrow docs chore now, before the
  Phase-F spec.
- **Phase F component implementation** can split by route after F0 locks shared
  seam types and fixtures.
- **Phase 3 Foundry/ERC-8004** can proceed in parallel with Phase F after the
  staleness edits, because it does not depend on marketplace metadata details
  beyond the basic `agentURI` path.

Work that should not be delegated independently:

- **F0 seam preflight and Phase 1 metadata close-out** need one owner. These are
  the cross-doc integration points; splitting them creates schema drift.
- **Seller flow + metadata spec** should share an owner or review loop. The
  publish checklist is the write side of the schema.
- **Gen-art algorithm + tokenURI encoding** should share an owner in Phase 4.
  Art changes after deployment are contract-visible.
- **Wallet, purchase, sealed relay, and receipt install flow** should be
  reviewed together in Phase 6. They are one buyer transaction path, not four
  independent widgets.

---

## 9. Immediate next action

1. **Operator review of this strategy** (esp. §2 fixes, §3 decisions, the
   front-end-first reorder, gen-art demotion).
2. On approval, **write the Phase F implementation spec** (§6) via the normal
   brainstorming → writing-plans flow, then build F0→F8.
3. **Done in this revision:** apply the §2 staleness fixes to the three older
   specs so no one builds from stale instructions.

---

## 10. Maintenance

- Resolve open decisions in §7 → move the detail to the owning spec/ADR, leave a
  one-line pointer here.
- When a phase exits, strike it in §4 and update §5.
- Keep §2 as the staleness audit trail; if future stale rows are added, patch
  the owning spec in the same change set whenever possible.
- This doc is the program front door. The nav doc remains the phase-ordering
  reference; the per-slice specs remain authoritative for their slices.

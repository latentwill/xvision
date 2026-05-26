# Marketplace Phase F — Frontend on Fixtures (Design Spec)

> **Purpose:** Implementable design for **Phase F** of the marketplace program:
> the buyer/seller-facing surfaces built in the existing Vite SPA against a typed
> `MarketplaceData` seam backed by **fixtures**, with **no chain dependency**.
> The seam's types are the *draft data contract* that Phase 1's Metadata &
> Data-Contract spec later formalizes.
>
> **Parent:** [`2026-05-26-marketplace-program-strategy.md`](../plans/2026-05-26-marketplace-program-strategy.md)
> (§6 names F0–F8; §8 sets the ownership rules). **Visual source:** the hi-fi
> handoff `docs/design/design_handoff_marketplace_shift/` (locked — pixels,
> copy, tokens). **This spec adds the data layer the handoff doesn't define.**
>
> **Status:** Proposed; seam shape + four modeling defaults approved 2026-05-26.
> Pending operator review before the implementation plan (writing-plans).

---

## 1. Scope

**In Phase F (build, against fixtures):**

| Sub-step | Surface | Route |
|---|---|---|
| **F0** | The `MarketplaceData` seam + `FixtureMarketplaceData` + new primitives | — |
| **F1** | Marketplace browse + filter drawer + leaderboard rail | `/marketplace` |
| **F2** | Lineage identity (closed) + on-chain-receipts drawer (open) + trade-history | `/marketplace/lineage/:name` |
| **F3** | Creator profile + lineage forest | `/marketplace/creator/:handleOrAddr` |
| **F4** | Leaderboard (curated slices) | `/marketplace/leaderboard` |
| **F5** | Seller onboarding (3-step inline) | `/marketplace/sell` |
| **F6** | Purchase receipt + install + share composer | `/marketplace/receipts/:tx` |
| **F7** | Shareable OG card (1200×630 React component) | — (rendered; SSR deferred) |
| **F8** | Fixtures + hook layer (`FixtureMarketplaceData`, 200-row wall set) | — |

**Explicitly deferred (NOT in Phase F):** public `/` landing; real wallet
connect; real tx submission; real SSR/PNG OG generation; Follow / Tip / Save-view
persistence; production gen-art; the Persona-A operator chain-ops surface
(Settings → Chain ops, Phase 5/6); real subgraph/IPFS/local-API wiring (Phase 6).

**Constraints (hard):** no popups — the filter drawer is a docked right-edge
panel, the receipts drawer inline-expands, the seller flow is an inline
three-step expansion (CLAUDE.md). `[Testnet]` labeling on anything that will
become a chain action. Gen-art renders as a labeled placeholder. Reuse the
Signal theme + existing primitives; do not fork tokens. Treat handoff
`bc2-*.jsx` as **visual reference only**.

---

## 2. F0 — The `MarketplaceData` seam (keystone)

One typed interface every surface reads through. **Owned and frozen as a single
unit before F1–F8 split** (program strategy §8 — schema-drift risk). Designed
**typed/numeric**, not as the prototype's display strings: the seam carries
numbers + ISO timestamps; components format. This is what hardens into the
metadata spec, so it pre-commits the right shape.

### 2.1 Scalars & shared records

```ts
type Id          = string;            // listing slug, e.g. "btc-momentum-v3"
type LineageId   = string;            // "btc-momentum"
type GenArtSeed  = string;            // placeholder seed until Phase 4
type IsoDateTime = string;
type PayerKind   = 'human' | 'agent';
type Tier        = 'open' | 'sealed'; // Tier A (open/free) | Tier B (sealed/paid)
type Verification    = 'verified' | 'unverified';   // A9: signal only; threshold = Phase 1
type IngredientKind  = 'model' | 'mcp' | 'skill';
type Verdict         = 'endorse' | 'question' | 'reject';

interface Creator {                   // A8: address required, display name optional
  address: string;                    // full address; components truncate
  handle?: string;                    // "@ed"
  ens?: string;                       // "ed.xvn"
}
interface BuyerCounts { humans: number; agents: number; }   // E6: payer class explicit
interface Ingredient  { name: string; kind: IngredientKind; installed: boolean; }
interface TxRef { txHash: string; network: string; }       // network drives the [Testnet] label
```

### 2.2 Listing (browse row ⊂ detail)

```ts
interface ListingRow {
  id: Id; lineageId: LineageId; version: string;       // "v4.2"
  creator: Creator; model: string; style: string;       // "Claude · Haiku 4.5", "Day"
  assets: string[];                                      // ["SOL"]
  return30dPct: number;                                  // 89.4
  sharpe: number;                                         // 1.84
  buyers: BuyerCounts;
  priceUsdc: number | null;                              // null ⇒ Tier A open/free
  tier: Tier;
  transferableLicense: boolean;                          // default false (direction); opt-in per listing
  verification: Verification;
  acceptsX402: boolean;
  clones: number;
  genArtSeed: GenArtSeed;
  // sparkline is derived component-side from genArtSeed + sign(return30dPct)
}

interface MetricSet {
  return30dPct: number; sharpe: number; winRatePct: number;
  maxDrawdownPct: number; avgDurationDays: number;
}

interface Variant {                                       // lineage mini-tree node
  version: string; parent: string | null; genArtSeed: GenArtSeed;
  sharpe: number; current: boolean;
}
interface RecentBuyer {
  label: string;            // "0x7c2e…aa07" | "agent #14"
  payerKind: PayerKind;
  outcome: string;          // opaque display: "+12.4% · 6d" | "running · 2 trades"
  at: IsoDateTime;
}
interface EquityCurve {
  base: number;                                          // 1000
  points: { value: number; phase: 'backtest' | 'live' }[];
}

interface ListingDetail extends ListingRow {
  promise: string;
  metrics: MetricSet;
  paidToCreatorUsd: number;
  platformFeeBps: number;                                // 500 = 5%
  ingredients: Ingredient[];
  variants: Variant[];
  clonesOfYours?: { count: number; upstreamEarningsUsd: number };
  recentBuyers: RecentBuyer[];
  creatorOther: ListingRow[];
  equityCurve: EquityCurve;
  whatYouGet: string[];
  whatYouDont: string[];
  onChain: OnChainReceipts;
}
```

### 2.3 On-chain receipts (drawer)

```ts
interface OnChainReceipts {
  nft: {
    tokenId: string; lineageId: LineageId; agentURI: string; manifestHash: string;
    parentLineage: string | null; bornAt: IsoDateTime; operatorSig: string;
    contract: string; network: string;                  // network drives [Testnet] label
  };
  attestations: { attester: string; verdict: Verdict; targetVersion: string; at: IsoDateTime }[];
  anchors:      { kind: 'merkle' | 'mint' | 'commit'; label: string; tx: string; at: IsoDateTime; gasEth: string }[];
  trades:       TradeRecord[];
  tradesMeta:   { totalOnChain: number; lastAnchorAt: IsoDateTime; receiptKind: string;
                  netPnlUsd: number; window: string; anchorTx: string };
}
interface TradeRecord {
  at: IsoDateTime; action: 'buy' | 'sell' | 'close'; symbol: string; qty: string;
  entry: number | null; exit: number | null; pnlUsd: number | null; pnlPct: number | null;
  runner: string; runnerKind: PayerKind; tx: string; anchorTx: string;
}
```

### 2.4 Creator profile

```ts
interface CreatorProfile {
  creator: Creator; joinedAt: IsoDateTime; reputation: number; notableTag?: string;
  counters: {
    strategies: number; lifetimeEarnedUsd: number; totalBuyers: BuyerCounts;
    clonesSpawned: number; clonesUpstreamUsd: number; attestationsIssued: number;
  };
  strategies: (ListingRow & { status: 'live' | 'archived' })[];
  earningsWeekly: number[];                               // oldest-first weekly USDC
  earningsSummary: { last7dUsd: number; last30dUsd: number };
  forest: { nodes: ForestNode[]; edges: ForestEdge[] };
  reputationFeed: AttestationActivity[];
  clonedBy: CloneByEntry[];
}
interface ForestNode {
  id: string; x: number; y: number; label: string; strategy: string;
  current?: boolean; genArtSeed?: string; external?: boolean; more?: boolean;
}
interface ForestEdge { from: string; to: string; kind?: 'clone' }   // normalized from tuple
interface AttestationActivity { direction: 'received' | 'issued'; verdict: Verdict; attester: string; on: string; at: IsoDateTime }
interface CloneByEntry { handle: string; from: string; made: string; earnedUsd: number; at: IsoDateTime; more?: boolean }
```

### 2.5 Receipt + share + OG card

```ts
interface Receipt {
  txHash: string; network: string; at: IsoDateTime; buyer: string;   // network ⇒ [Testnet] label
  listing: { id: Id; version: string; creator: Creator; genArtSeed: GenArtSeed; return30dPct: number; buyers: BuyerCounts };
  license: {
    tokenId: string; contract: string; manifestHash: string; bundleCid: string;
    pricePaidUsdc: number; feeUsdc: number; netToCreatorUsdc: number; mintedAt: IsoDateTime;
  };  // NOTE: LicenseToken is ERC-1155 (H6). Prototype's "ERC-721" label is the documented slip.
  install: { xvnDetected: boolean; xvnEndpoint: string; ingredients: Ingredient[] };
  share: ShareComposerData;
}
interface ShareComposerData { ogCard: ShareableCardData; buyerStamp: string; caption: string; variants: string[]; notificationHint: string }
interface ShareableCardData {
  id: Id; version: string; creator: Creator; genArtSeed: GenArtSeed;
  return30dPct: number; return30dLabel?: string; buyers: BuyerCounts;
  paidToCreatorUsd: number; priceUsdc: number; verification: Verification;
  acceptsX402: boolean; promise?: string; url: string;
}
```

### 2.6 Filters, slices, seller, stats, notifications

```ts
interface FilterState {
  segment: 'trending' | 'new' | 'mine';
  search: string;
  sort: 'return30d' | 'sharpe' | 'buyers' | 'mostCloned' | 'newest';
  assets: string[]; models: string[]; styles: string[];
  trust: { verifiedOnly: boolean; acceptsAgents: boolean; auditedOnly: boolean };
  priceUsdc: { from: number; to: number };
  minBuyers: number;
  slice?: SliceId;
}
type SliceId = string;
interface Slice { id: SliceId; label: string; hint: string; count: number; filter: Partial<FilterState> }

interface ListableStrategy { id: string; name: string; version: string; assets: string[] }  // F5
interface ListabilityCheck { ok: boolean; label: string; reason?: string }
interface PublishDraft {
  strategyId: string; listable: ListabilityCheck[];
  tier: Tier; priceUsdc: number | null;
  acceptedPayers: { humans: boolean; agents: boolean };
  ingredients: Ingredient[];
  preview: ListingRow;                                    // mint-preview projection
}

interface MarketplaceStats { totalStrategies: number; paidThisWeekUsd: number; agentPurchases: number; mintedLast24h: number }
interface PurchaseEvent { listingId: Id; version: string; buyer: string; payerKind: PayerKind; amountUsdc: number; netToCreatorUsdc: number; at: IsoDateTime }

// Fixture viewer/account state. Wallet-connect is Phase 6 (A5); until then the
// fixture supplies a connected demo account. Components derive, not store:
//   canClone(listing)   = listing.tier === 'open' || ownedListingIds.includes(listing.id)   // A10
//   hasPurchased(id)    = ownedListingIds.includes(id)
//   "Mine" segment rows = createdListingIds
interface Viewer {
  isConnected: boolean;        // fixture: true; real wallet = Phase 6
  address?: string;
  handle?: string;
  createdListingIds: Id[];
  ownedListingIds: Id[];
}
```

### 2.7 The interface

```ts
interface MarketplaceData {
  // reads
  getStats(): Promise<MarketplaceStats>;
  listListings(f: FilterState): Promise<{ rows: ListingRow[]; total: number; matched: number }>;
  getSlices(): Promise<Slice[]>;
  getListing(idOrName: string): Promise<ListingDetail>;
  getCreator(handleOrAddress: string): Promise<CreatorProfile>;
  getLeaderboard(sliceId: SliceId): Promise<{ slice: Slice; rows: ListingRow[] }>;
  getReceipt(txHash: string): Promise<Receipt>;
  getViewer(): Promise<Viewer>;          // fixture account; drives "Mine" + clone gate (A5/A10)
  // seller write path (F5) — fixture now; real impl reads local /api/strategies later
  listListableStrategies(): Promise<ListableStrategy[]>;
  createPublishDraft(strategyId: string): Promise<PublishDraft>;
  submitListing(d: PublishDraft): Promise<TxRef>;   // fixture: synthesized tx + network
  // intents — fixture: synthesized TxRef (txHash + network), route to /marketplace/receipts/:tx
  purchaseIntent(listingId: Id): Promise<TxRef>;
  cloneIntent(listingId: Id): Promise<TxRef>;
  // notifications — fixture: timer-driven demo events
  subscribePurchases(cb: (e: PurchaseEvent) => void): () => void;
}
```

### 2.8 Four modeling defaults (approved 2026-05-26)

1. **Payer class explicit (E6).** `BuyerCounts {humans, agents}`, `RecentBuyer.payerKind`,
   `TradeRecord.runnerKind`, `PurchaseEvent.payerKind`. The fixture pre-commits
   what the real marketplace event/subgraph must expose — bare token transfers
   can't encode it.
2. **Handle optional (A8).** `Creator { address; handle?; ens? }`. Resolution
   mechanism (ENS vs registry vs `agentURI`) stays a Phase-1 concern.
3. **Verification is a carried signal (A9).** `Verification` enum; threshold
   logic is Phase 1.
4. **Seller flow on fixtures, shaped for real.** `listListableStrategies()` /
   `createPublishDraft()` return fixtures now; their real impl later reads the
   existing local `/api/strategies`. Keeps Phase F decoupled.
5. **Fixture viewer context (A5/A10).** `getViewer()` returns a connected demo
   account. Wallet-connect is Phase 6; until then `createdListingIds` drives the
   "Mine" segment and `ownedListingIds` drives owned stamps + the Tier-B clone
   gate. No component reads a wallet directly.
6. **`TxRef` carries `network` (testnet labeling).** Every tx-producing call
   (`purchaseIntent` / `cloneIntent` / `submitListing`) and `Receipt` carry a
   `network`, so `TxChip` and chain-bound CTAs render `[Testnet]` without guessing.

---

## 3. F0 — Primitives

Reuse existing repo primitives where present (`frontend/web/src/components/primitives`:
`Card`, `Pill`, `Badge`, `Icon`, `BrandMark`; charts v2: `MiniSparkline`,
`HeroGradientEquity`). New, marketplace-scoped:

| Primitive | Maps to / built on | Notes |
|---|---|---|
| `GenArtPlaceholder` | new | deterministic, seed-keyed block (palette from seed hash); **labeled placeholder**; clean swap-point for Phase-4 real generator. Scales 32→1200px. |
| `Sparkline` | new (lightweight SVG) | 30-pt seeded SVG (handoff bc2 approach); gold/danger tint. uPlot `MiniSparkline` stays for chart-grid use. |
| `AgentIcon` | new | bot SVG (no emoji) wherever agent counts appear |
| `AssetPill` | `Pill` | ticker + tone map (BTC amber, ETH sky, SOL violet, DOGE pink) |
| `VerifiedBadge` | `Badge` | green-check; title "backtested + live-paper committed on chain" |
| `X402Badge` | `Badge` | `AgentIcon` + "x402" |
| `RemovableChip` | `Pill` | applied-filter chip with × |
| `FilterDrawer` | new (docked panel) | right-edge slide-in; **not** a Popover; covers list area, rail/sidebar stay |
| `TxChip` | new | mono tx pill + external-link; `[Testnet]`-aware |
| `ShareableCard` | new | 1200×630 OG composition; no app chrome |

`Button` (handoff `Btn`) — reuse the repo's button pattern; add variants
(`primary | ghost | danger | chip`) if missing.

---

## 4. F1–F8 — per-surface acceptance

Each renders entirely from `FixtureMarketplaceData`. Layouts/copy/spacing per the
handoff README + frame files. Acceptance = renders from fixtures, URL state
round-trips, no-popups honored, `[Testnet]` shown on chain-bound actions.

- **F1 `/marketplace`** — header (promise H1 + `MarketplaceStats` counter flex +
  Share/Share-your-strategy CTAs), toolbar (`Segmented` trending/new/mine, search
  with `/`, sort, `Filters [n]`), applied `RemovableChip` row, 232px leaderboard
  rail (`getSlices`), `ListingRow` list (`listListings`), `FilterDrawer`
  (sort/assets/models/style/trust/price/min-buyers). `FilterState` ⇄ URL query.
  Sort + filter re-query. The **Mine** segment filters to
  `getViewer().createdListingIds`. **Accept:** filtered/sorted rows match fixture
  expectations; Mine shows only the viewer's listings; drawer applies; slice
  click loads its filter; chips remove.
- **F2 `/marketplace/lineage/:name`** — above-fold 3-col (hero
  `GenArtPlaceholder` + NFT stamp | info stack: title/badges, creator line,
  promise, big `return30dPct`, `MetricSet` row, buyer card | purchase column:
  price card + `Buy`/`Run free` + `Clone to edit` + `Share`). Ingredient-check
  banner (`ingredients`, installed count). Below: `EquityCurve` (charts v2,
  backtest faded/live solid, "If I bought at mint" toggle), what-you-get/don't,
  `Variant` mini-tree, `RecentBuyers`, creator-other. **On-chain receipts
  drawer** (`?receipts=open`) inline-expands `OnChainReceipts` (NFT/manifest,
  attestations, anchors, trade-history table with action/runner filters).
  **Accept:** drawer toggles via URL; ingredient banner reflects fixture install
  state; `Buy` → `purchaseIntent` → route to `/marketplace/receipts/:tx`;
  `Clone to edit` gate uses `getViewer()`: enabled when `tier === 'open'` or
  `ownedListingIds.includes(id)` (Tier-B-needs-purchase, A10).
- **F3 `/marketplace/creator/:handleOrAddr`** — hero (identicon
  `GenArtPlaceholder` from address, handle/ENS/address/rep), 6 `counters`,
  strategies grid, `EarningsChart` (`earningsWeekly`), `LineageForest`
  (`forest` SVG: solid variant edges, dashed clone edges, gold HEAD,
  ghosted external + "+N more"), reputation feed, cloned-by. **Accept:** forest
  node click routes to that lineage; reputation filter tabs filter in place.
- **F4 `/marketplace/leaderboard`** (index = list of canonical slices) and
  **`/marketplace/leaderboard/:sliceId`** (one slice — the stable, shareable URL
  + later OG card). Reuse F1 row/card primitives. The browse rail's ad-hoc slices
  use `/marketplace?slice=<id>` (`FilterState.slice`); canonical ones use the path
  param. No hi-fi frame → scope explicit here. **Accept:**
  `/marketplace/leaderboard/:sliceId` loads that slice's rows; index lists slices.
- **F5 `/marketplace/sell`** — inline 3-step: (1) pick `ListableStrategy`
  (`listListableStrategies`), (2) choose `Tier`/price/`acceptedPayers` with typed
  `ListabilityCheck` failures, (3) preview (`PublishDraft.preview` as a
  `ListingRow`) + mint (`submitListing` → fake tx). Inline-expanded from the
  browse "Share your strategy" CTA; **no modal.** **Accept:** listability
  failures are specific and typed; Tier A omits price; mint routes to receipt.
- **F6 `/marketplace/receipts/:tx`** — success strip (fee breakdown), license
  card (`GenArtPlaceholder` + overlays + `license` meta), install steps
  (detected → decrypt → install-missing-ingredients → add-to-strategies), share
  composer (`ShareableCardMini` preview + caption editor + variants + post
  targets + notification hint). **Accept:** steps reflect `install` state;
  post-to-X opens intent URL; composer pre-loads `share`.
- **F7 `ShareableCard`** — 1200×630 component from `ShareableCardData`; left
  full-bleed `GenArtPlaceholder`, right info composition + QR placeholder.
  **Accept:** composes at exact ratio; SSR/PNG explicitly deferred (Phase 6/A6).
- **F8 fixtures + hooks** — `FixtureMarketplaceData` implementing §2.7; realistic
  fixtures from the handoff sample values; a **200-row set** for the gen-art
  wall (validates placeholder at scale + later real-art swap). Hooks:
  `useFilterState` (URL-synced), `useListings`, `useListing`, `useCreatorProfile`,
  `useLeaderboard`, `useReceipt`, `useReceiptsDrawer`, `usePublishDraft`,
  `useIngredientCheck`, `usePurchaseIntent`, `useCloneIntent`,
  `usePurchaseNotifications`. **Accept:** one swap-point (`MarketplaceDataProvider`)
  to replace fixtures with real impls in Phase 6.

---

## 5. Routing & wiring

Add under the SPA's React-Router v6 tree (`frontend/web/src/routes.tsx`), lazy
+ Suspense like existing routes:

```
/marketplace                         → F1   (?slice=<id> applies a rail slice)
/marketplace/leaderboard             → F4   (index: list of canonical slices)
/marketplace/leaderboard/:sliceId    → F4   (one canonical slice; stable share URL)
/marketplace/lineage/:name           → F2  (?receipts=open toggles drawer)
/marketplace/creator/:handleOrAddr   → F3
/marketplace/sell                    → F5
/marketplace/receipts/:tx            → F6
```

A `MarketplaceDataProvider` (React context) supplies the `MarketplaceData`
instance; Phase F mounts `FixtureMarketplaceData`. Reuse the existing
clone-to-edit endpoint (`POST /api/strategy/:id/clone`) only behind
`cloneIntent`'s *real* impl (Phase 6) — F5/F2 use fixtures now.

---

## 6. Verification (Phase F exit)

- All six routes render from fixtures with zero network calls.
- `FilterState` round-trips through the URL; sort/filter/slice change rows
  deterministically against fixture expectations (unit-tested).
- The 200-row gen-art wall renders without layout breakage (placeholder at scale).
- The OG card composes at 1200×630.
- No `Dialog`/`Modal`/`Sheet`/`Popover` introduced; drawers/receipts/seller are
  panels/inline.
- `[Testnet]` appears on every chain-bound affordance (Buy, Mint, receipts, tx
  chips).
- A single provider swap is the only change needed to point at real backends.

---

## 7. Hand-off to Phase 1 (metadata spec)

The frozen §2 seam is the draft data contract. Phase 1 must resolve, against
real chain/IPFS shapes: the `agentURI`/Tier-1 field set (reconciled with the
plugin `LineageManifest`), the `tier`/`priceUsdc`/`transferableLicense` listing
fields, the payer-class source (E6) in the marketplace event/subgraph, handle
resolution (A8), verification thresholds (A9), and Tier-B clone semantics (A10).
Each is carried as a typed seam field here so the formalization is a tightening,
not a redesign.

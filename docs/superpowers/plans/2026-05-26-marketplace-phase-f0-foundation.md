# Marketplace Phase F0 — Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the frozen foundation for the marketplace frontend — the typed `MarketplaceData` seam, a fixture implementation (incl. a 200-row gen-art-wall set), the new UI primitives, the React provider, and a `/marketplace/*` routing shell — so F1–F7 route plans build against a stable contract.

**Architecture:** A self-contained `src/features/marketplace/` feature folder. A pure typed data seam (`MarketplaceData`) is implemented over in-memory fixtures (`FixtureMarketplaceData`) with a pure filter/sort function. A React context (`MarketplaceDataProvider`) supplies the instance; Phase F mounts the fixture impl. New presentational primitives mirror the hi-fi handoff. A parent `MarketplaceLayout` route provides the context and renders child routes (stubbed in F0, filled in F1–F7).

**Tech Stack:** React 18, TypeScript, React-Router v6, Vitest 2 + React Testing Library + jsdom, Tailwind (token classes from `tokens.css`), pnpm.

**Source spec:** [`../specs/2026-05-26-marketplace-phase-f-frontend-design.md`](../specs/2026-05-26-marketplace-phase-f-frontend-design.md) · **Program:** [`2026-05-26-marketplace-program-strategy.md`](./2026-05-26-marketplace-program-strategy.md)

**Conventions (verified in repo):**
- Run tests from `frontend/web`: `pnpm exec vitest run <path>`. Typecheck: `pnpm typecheck`.
- Path alias `@/` → `src/`. Colocate tests as `*.test.ts(x)`. Token classes only (no hex, no inline color); respect dark-mode border rules.
- **No popups** — the filter drawer is a docked panel, never `Dialog`/`Modal`/`Sheet`/`Popover`.
- Execute on a feature branch (e.g. `feat/marketplace-f0`), not `main`. Commit per task.

**Scope note:** F0 produces a tested data layer + primitive library + navigable (stubbed) route tree. It is the unit the program strategy §8 says to **freeze** before F1–F7 split. Route surfaces are intentionally stubs here.

---

## File map

```
src/features/marketplace/
  data/
    types.ts                     # all seam types (Task 1)
    filter.ts                    # pure filter+sort over ListingRow[] (Task 2)
    filter.test.ts
    fixtures/
      listings.ts                # named ListingRow/ListingDetail + 200-row wall generator (Task 3)
      creators.ts                # CreatorProfile fixtures (Task 3)
      slices.ts                  # Slice fixtures (Task 3)
      receipts.ts                # Receipt fixtures (Task 3)
      seller.ts                  # ListableStrategy + publish-draft builder (Task 3)
      viewer.ts                  # fixture Viewer account (Task 3)
    fixtures/fixtures.test.ts    # fixture-integrity tests (Task 3)
    MarketplaceData.ts           # interface + FixtureMarketplaceData (Tasks 1,4)
    FixtureMarketplaceData.test.ts
    provider.tsx                 # context + Provider + useMarketplaceData (Task 5)
    provider.test.tsx
  hooks/
    useFilterState.ts            # URL-synced FilterState (Task 6)
    useFilterState.test.tsx
  components/
    GenArtPlaceholder.tsx        # Task 7
    GenArtPlaceholder.test.tsx
    Sparkline.tsx                # Task 8
    Sparkline.test.tsx
    AgentIcon.tsx                # Task 8
    AssetPill.tsx                # Task 9
    AssetPill.test.tsx
    VerifiedBadge.tsx            # Task 9
    X402Badge.tsx                # Task 9
    badges.test.tsx
    RemovableChip.tsx            # Task 10
    TxChip.tsx                   # Task 10
    chips.test.tsx
    FilterDrawer.tsx             # Task 11
    FilterDrawer.test.tsx
    ShareableCard.tsx            # Task 12
    ShareableCard.test.tsx
  routes/
    MarketplaceLayout.tsx        # provider + <Outlet/> (Task 13)
    stubs.tsx                    # F0 route stubs (Task 13)
  marketplace-routes.test.tsx    # routing smoke (Task 13)
```
`src/routes.tsx` is modified once (Task 13) to mount the subtree.

---

## Task 1: Seam types

**Files:**
- Create: `src/features/marketplace/data/types.ts`

- [ ] **Step 1: Write the types module**

```ts
// src/features/marketplace/data/types.ts
// The MarketplaceData seam types. Typed/numeric (not display strings);
// components format. This is the draft data contract Phase 1 formalizes.

export type Id = string;            // listing slug, e.g. "btc-momentum-v3"
export type LineageId = string;     // "btc-momentum"
export type GenArtSeed = string;    // placeholder seed until Phase 4
export type IsoDateTime = string;
export type PayerKind = "human" | "agent";
export type Tier = "open" | "sealed";          // Tier A | Tier B
export type Verification = "verified" | "unverified";
export type IngredientKind = "model" | "mcp" | "skill";
export type Verdict = "endorse" | "question" | "reject";

export interface Creator {
  address: string;
  handle?: string;
  ens?: string;
}
export interface BuyerCounts {
  humans: number;
  agents: number;
}
export interface Ingredient {
  name: string;
  kind: IngredientKind;
  installed: boolean;
}
export interface TxRef {
  txHash: string;
  network: string; // drives the [Testnet] label on tx chips + chain-bound CTAs
}

export interface ListingRow {
  id: Id;
  lineageId: LineageId;
  version: string;
  creator: Creator;
  model: string;
  style: string;
  assets: string[];
  return30dPct: number;
  sharpe: number;
  buyers: BuyerCounts;
  priceUsdc: number | null; // null => Tier A open/free
  tier: Tier;
  transferableLicense: boolean; // default false (direction); opt-in per listing
  verification: Verification;
  acceptsX402: boolean;
  clones: number;
  genArtSeed: GenArtSeed;
}

export interface MetricSet {
  return30dPct: number;
  sharpe: number;
  winRatePct: number;
  maxDrawdownPct: number;
  avgDurationDays: number;
}
export interface Variant {
  version: string;
  parent: string | null;
  genArtSeed: GenArtSeed;
  sharpe: number;
  current: boolean;
}
export interface RecentBuyer {
  label: string;
  payerKind: PayerKind;
  outcome: string;
  at: IsoDateTime;
}
export interface EquityCurve {
  base: number;
  points: { value: number; phase: "backtest" | "live" }[];
}
export interface TradeRecord {
  at: IsoDateTime;
  action: "buy" | "sell" | "close";
  symbol: string;
  qty: string;
  entry: number | null;
  exit: number | null;
  pnlUsd: number | null;
  pnlPct: number | null;
  runner: string;
  runnerKind: PayerKind;
  tx: string;
  anchorTx: string;
}
export interface OnChainReceipts {
  nft: {
    tokenId: string;
    lineageId: LineageId;
    agentURI: string;
    manifestHash: string;
    parentLineage: string | null;
    bornAt: IsoDateTime;
    operatorSig: string;
    contract: string;
    network: string;
  };
  attestations: { attester: string; verdict: Verdict; targetVersion: string; at: IsoDateTime }[];
  anchors: { kind: "merkle" | "mint" | "commit"; label: string; tx: string; at: IsoDateTime; gasEth: string }[];
  trades: TradeRecord[];
  tradesMeta: {
    totalOnChain: number;
    lastAnchorAt: IsoDateTime;
    receiptKind: string;
    netPnlUsd: number;
    window: string;
    anchorTx: string;
  };
}
export interface ListingDetail extends ListingRow {
  promise: string;
  metrics: MetricSet;
  paidToCreatorUsd: number;
  platformFeeBps: number;
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

export interface ForestNode {
  id: string;
  x: number;
  y: number;
  label: string;
  strategy: string;
  current?: boolean;
  genArtSeed?: string;
  external?: boolean;
  more?: boolean;
}
export interface ForestEdge {
  from: string;
  to: string;
  kind?: "clone";
}
export interface AttestationActivity {
  direction: "received" | "issued";
  verdict: Verdict;
  attester: string;
  on: string;
  at: IsoDateTime;
}
export interface CloneByEntry {
  handle: string;
  from: string;
  made: string;
  earnedUsd: number;
  at: IsoDateTime;
  more?: boolean;
}
export interface CreatorProfile {
  creator: Creator;
  joinedAt: IsoDateTime;
  reputation: number;
  notableTag?: string;
  counters: {
    strategies: number;
    lifetimeEarnedUsd: number;
    totalBuyers: BuyerCounts;
    clonesSpawned: number;
    clonesUpstreamUsd: number;
    attestationsIssued: number;
  };
  strategies: (ListingRow & { status: "live" | "archived" })[];
  earningsWeekly: number[];
  earningsSummary: { last7dUsd: number; last30dUsd: number };
  forest: { nodes: ForestNode[]; edges: ForestEdge[] };
  reputationFeed: AttestationActivity[];
  clonedBy: CloneByEntry[];
}

export interface ShareableCardData {
  id: Id;
  version: string;
  creator: Creator;
  genArtSeed: GenArtSeed;
  return30dPct: number;
  return30dLabel?: string;
  buyers: BuyerCounts;
  paidToCreatorUsd: number;
  priceUsdc: number;
  verification: Verification;
  acceptsX402: boolean;
  promise?: string;
  url: string;
}
export interface ShareComposerData {
  ogCard: ShareableCardData;
  buyerStamp: string;
  caption: string;
  variants: string[];
  notificationHint: string;
}
export interface Receipt {
  txHash: string;
  network: string; // drives [Testnet] label on the receipt
  at: IsoDateTime;
  buyer: string;
  listing: { id: Id; version: string; creator: Creator; genArtSeed: GenArtSeed; return30dPct: number; buyers: BuyerCounts };
  license: {
    tokenId: string; // ERC-1155 (H6); prototype "ERC-721" label is the documented slip
    contract: string;
    manifestHash: string;
    bundleCid: string;
    pricePaidUsdc: number;
    feeUsdc: number;
    netToCreatorUsdc: number;
    mintedAt: IsoDateTime;
  };
  install: { xvnDetected: boolean; xvnEndpoint: string; ingredients: Ingredient[] };
  share: ShareComposerData;
}

export type SortKey = "return30d" | "sharpe" | "buyers" | "mostCloned" | "newest";
export interface FilterState {
  segment: "trending" | "new" | "mine";
  search: string;
  sort: SortKey;
  assets: string[];
  models: string[];
  styles: string[];
  trust: { verifiedOnly: boolean; acceptsAgents: boolean; auditedOnly: boolean };
  priceUsdc: { from: number; to: number };
  minBuyers: number;
  slice?: SliceId;
}
export type SliceId = string;
export interface Slice {
  id: SliceId;
  label: string;
  hint: string;
  count: number;
  filter: Partial<FilterState>;
}

export interface ListableStrategy {
  id: string;
  name: string;
  version: string;
  assets: string[];
}
export interface ListabilityCheck {
  ok: boolean;
  label: string;
  reason?: string;
}
export interface PublishDraft {
  strategyId: string;
  listable: ListabilityCheck[];
  tier: Tier;
  priceUsdc: number | null;
  acceptedPayers: { humans: boolean; agents: boolean };
  ingredients: Ingredient[];
  preview: ListingRow;
}

export interface MarketplaceStats {
  totalStrategies: number;
  paidThisWeekUsd: number;
  agentPurchases: number;
  mintedLast24h: number;
}
export interface PurchaseEvent {
  listingId: Id;
  version: string;
  buyer: string;
  payerKind: PayerKind;
  amountUsdc: number;
  netToCreatorUsdc: number;
  at: IsoDateTime;
}

// Fixture viewer/account state. Wallet-connect is Phase 6 (A5). Components
// derive: canClone = tier === "open" || ownedListingIds.includes(id) (A10);
// "Mine" segment rows = createdListingIds.
export interface Viewer {
  isConnected: boolean;
  address?: string;
  handle?: string;
  createdListingIds: Id[];
  ownedListingIds: Id[];
}
```

- [ ] **Step 2: Verify it typechecks**

Run (from `frontend/web`): `pnpm typecheck`
Expected: PASS (no errors; the file is types-only).

- [ ] **Step 3: Commit**

```bash
git add src/features/marketplace/data/types.ts
git commit -m "feat(marketplace): F0 seam types"
```

---

## Task 2: Pure filter + sort

**Files:**
- Create: `src/features/marketplace/data/filter.ts`
- Test: `src/features/marketplace/data/filter.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// src/features/marketplace/data/filter.test.ts
import { describe, expect, it } from "vitest";
import { applyFilter, defaultFilterState } from "./filter";
import type { ListingRow } from "./types";

function row(p: Partial<ListingRow>): ListingRow {
  return {
    id: "x", lineageId: "x", version: "v1.0",
    creator: { address: "0xabc" }, model: "Claude", style: "Day",
    assets: ["BTC"], return30dPct: 10, sharpe: 1, buyers: { humans: 5, agents: 0 },
    priceUsdc: 49, tier: "sealed", verification: "unverified",
    acceptsX402: false, clones: 0, genArtSeed: "x", ...p,
  };
}

describe("applyFilter", () => {
  const rows = [
    row({ id: "a", assets: ["BTC"], return30dPct: 50, verification: "verified", buyers: { humans: 100, agents: 4 } }),
    row({ id: "b", assets: ["SOL"], return30dPct: 90, verification: "unverified", buyers: { humans: 10, agents: 0 } }),
    row({ id: "c", assets: ["BTC", "ETH"], return30dPct: 20, verification: "verified", buyers: { humans: 300, agents: 1 } }),
  ];

  it("filters by asset", () => {
    const out = applyFilter(rows, { ...defaultFilterState(), assets: ["SOL"] });
    expect(out.rows.map((r) => r.id)).toEqual(["b"]);
    expect(out.matched).toBe(1);
    expect(out.total).toBe(3);
  });

  it("filters verified-only", () => {
    const out = applyFilter(rows, { ...defaultFilterState(), trust: { verifiedOnly: true, acceptsAgents: false, auditedOnly: false } });
    expect(out.rows.map((r) => r.id).sort()).toEqual(["a", "c"]);
  });

  it("sorts by 30d return desc by default", () => {
    const out = applyFilter(rows, defaultFilterState());
    expect(out.rows.map((r) => r.id)).toEqual(["b", "a", "c"]);
  });

  it("sorts by buyers (humans+agents) desc", () => {
    const out = applyFilter(rows, { ...defaultFilterState(), sort: "buyers" });
    expect(out.rows[0].id).toBe("c");
  });

  it("matches search over id and creator handle", () => {
    const withHandle = [row({ id: "btc-momentum", creator: { address: "0x1", handle: "@ed" } })];
    expect(applyFilter(withHandle, { ...defaultFilterState(), search: "mom" }).rows).toHaveLength(1);
    expect(applyFilter(withHandle, { ...defaultFilterState(), search: "@ed" }).rows).toHaveLength(1);
    expect(applyFilter(withHandle, { ...defaultFilterState(), search: "zzz" }).rows).toHaveLength(0);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/data/filter.test.ts`
Expected: FAIL — `applyFilter`/`defaultFilterState` not exported.

- [ ] **Step 3: Implement**

```ts
// src/features/marketplace/data/filter.ts
import type { FilterState, ListingRow, SortKey } from "./types";

export function defaultFilterState(): FilterState {
  return {
    segment: "trending",
    search: "",
    sort: "return30d",
    assets: [],
    models: [],
    styles: [],
    trust: { verifiedOnly: false, acceptsAgents: false, auditedOnly: false },
    priceUsdc: { from: 0, to: 500 },
    minBuyers: 0,
  };
}

const totalBuyers = (r: ListingRow) => r.buyers.humans + r.buyers.agents;

const SORTERS: Record<SortKey, (a: ListingRow, b: ListingRow) => number> = {
  return30d: (a, b) => b.return30dPct - a.return30dPct,
  sharpe: (a, b) => b.sharpe - a.sharpe,
  buyers: (a, b) => totalBuyers(b) - totalBuyers(a),
  mostCloned: (a, b) => b.clones - a.clones,
  newest: (a, b) => b.id.localeCompare(a.id), // fixture proxy; real impl uses publishedAt
};

export function applyFilter(
  rows: ListingRow[],
  f: FilterState,
): { rows: ListingRow[]; total: number; matched: number } {
  const q = f.search.trim().toLowerCase();
  const matched = rows.filter((r) => {
    if (f.assets.length && !f.assets.some((a) => r.assets.includes(a))) return false;
    if (f.models.length && !f.models.includes(r.model)) return false;
    if (f.styles.length && !f.styles.includes(r.style)) return false;
    if (f.trust.verifiedOnly && r.verification !== "verified") return false;
    if (f.trust.acceptsAgents && !r.acceptsX402) return false;
    if (totalBuyers(r) < f.minBuyers) return false;
    const price = r.priceUsdc ?? 0;
    if (price < f.priceUsdc.from || price > f.priceUsdc.to) return false;
    if (q && !`${r.id} ${r.creator.handle ?? ""}`.toLowerCase().includes(q)) return false;
    return true;
  });
  const sorted = [...matched].sort(SORTERS[f.sort]);
  return { rows: sorted, total: rows.length, matched: matched.length };
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/data/filter.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/data/filter.ts src/features/marketplace/data/filter.test.ts
git commit -m "feat(marketplace): F0 pure filter+sort"
```

---

## Task 3: Fixtures (+ 200-row gen-art wall)

**Files:**
- Create: `src/features/marketplace/data/fixtures/listings.ts`, `creators.ts`, `slices.ts`, `receipts.ts`, `seller.ts`
- Test: `src/features/marketplace/data/fixtures/fixtures.test.ts`

- [ ] **Step 1: Write `listings.ts`** (named rows + a full detail + a deterministic 200-row wall generator)

```ts
// src/features/marketplace/data/fixtures/listings.ts
import type { ListingDetail, ListingRow } from "../types";

const ed = { address: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4", handle: "@ed", ens: "ed.xvn" };
const vibe = { address: "0x9f12aa55bb77cc88dd99ee00ff11223344556677", handle: "@vibesharpe" };

export const NAMED_LISTINGS: ListingRow[] = [
  {
    id: "sol-strategist-pro", lineageId: "sol-strategist", version: "v4.2", creator: vibe,
    model: "Claude · Haiku 4.5", style: "Day", assets: ["SOL"], return30dPct: 89.4, sharpe: 1.84,
    buyers: { humans: 412, agents: 38 }, priceUsdc: 79, tier: "sealed", verification: "verified",
    acceptsX402: true, clones: 21, transferableLicense: false, genArtSeed: "sol-strategist-12fa",
  },
  {
    id: "meme-radar", lineageId: "meme-radar", version: "v1.0", creator: { address: "0xdead00beef", handle: "@degenray" },
    model: "GPT-5", style: "Momentum", assets: ["DOGE", "SOL"], return30dPct: 124.8, sharpe: 0.92,
    buyers: { humans: 88, agents: 12 }, priceUsdc: null, tier: "open", verification: "unverified",
    acceptsX402: true, clones: 9, transferableLicense: true, genArtSeed: "meme-radar-77aa",
  },
  {
    id: "doge-vol", lineageId: "doge-vol", version: "v1.1", creator: { address: "0xc0a4f3b2", handle: "@quantnext" },
    model: "Gemini 3 Pro", style: "Swing", assets: ["DOGE"], return30dPct: -2.3, sharpe: -0.18,
    buyers: { humans: 12, agents: 0 }, priceUsdc: 29, tier: "sealed", verification: "unverified",
    acceptsX402: false, clones: 0, transferableLicense: false, genArtSeed: "doge-vol-3b22",
  },
  {
    id: "btc-momentum-v3", lineageId: "btc-momentum", version: "v3.0", creator: ed,
    model: "Claude · Haiku 4.5", style: "Day", assets: ["BTC"], return30dPct: 47.2, sharpe: 1.31,
    buyers: { humans: 247, agents: 14 }, priceUsdc: 49, tier: "sealed", verification: "verified",
    acceptsX402: true, clones: 8, transferableLicense: false, genArtSeed: "btc-momentum-7a91-v3",
  },
  {
    id: "btc-grid-v2", lineageId: "btc-grid", version: "v2.0", creator: ed,
    model: "Claude · Haiku 4.5", style: "Mean-reversion", assets: ["BTC"], return30dPct: 31.4, sharpe: 1.12,
    buyers: { humans: 134, agents: 9 }, priceUsdc: 39, tier: "sealed", verification: "verified",
    acceptsX402: false, clones: 3, transferableLicense: false, genArtSeed: "btc-grid-6f5b",
  },
  {
    id: "eth-mr-v2", lineageId: "eth-mr", version: "v2.0", creator: ed,
    model: "Claude · Haiku 4.5", style: "Mean-reversion", assets: ["ETH"], return30dPct: 12.8, sharpe: 0.74,
    buyers: { humans: 88, agents: 3 }, priceUsdc: 0, tier: "open", verification: "unverified",
    acceptsX402: false, clones: 0, transferableLicense: true, genArtSeed: "eth-mr-3b22",
  },
];

const WALL_ASSETS = ["BTC", "ETH", "SOL", "DOGE", "MNT", "AVAX"];
const WALL_MODELS = ["Claude · Haiku 4.5", "GPT-5", "Gemini 3 Pro", "Llama 4"];
const WALL_STYLES = ["Day", "Swing", "Momentum", "Mean-reversion", "Long/Short"];

// Deterministic 200-row set for the gen-art wall + at-scale validation.
export function makeWallListings(n = 200): ListingRow[] {
  const out: ListingRow[] = [];
  for (let i = 0; i < n; i++) {
    const ret = ((i * 37) % 220) - 20; // -20..+199
    out.push({
      id: `wall-strat-${i}`, lineageId: `wall-${i % 40}`, version: `v${(i % 4) + 1}.0`,
      creator: { address: `0x${(i * 2654435761 >>> 0).toString(16)}`, handle: `@maker${i % 30}` },
      model: WALL_MODELS[i % WALL_MODELS.length], style: WALL_STYLES[i % WALL_STYLES.length],
      assets: [WALL_ASSETS[i % WALL_ASSETS.length]], return30dPct: ret, sharpe: ((i % 30) - 5) / 10,
      buyers: { humans: (i * 7) % 500, agents: i % 25 },
      priceUsdc: i % 5 === 0 ? null : 10 + (i % 20) * 5,
      tier: i % 5 === 0 ? "open" : "sealed", verification: i % 3 === 0 ? "verified" : "unverified",
      acceptsX402: i % 2 === 0, clones: i % 12, transferableLicense: i % 7 === 0, genArtSeed: `wall-${i}-${(i * 2246822507 >>> 0).toString(36)}`,
    });
  }
  return out;
}

export const ALL_LISTINGS: ListingRow[] = [...NAMED_LISTINGS, ...makeWallListings()];

export const LISTING_DETAILS: Record<string, ListingDetail> = {
  "btc-momentum-v3": {
    ...NAMED_LISTINGS[3],
    promise: "BTC momentum with Claude regime detection. Holds 1–3 days, 2% risk cap.",
    metrics: { return30dPct: 47.2, sharpe: 1.31, winRatePct: 62, maxDrawdownPct: -8.4, avgDurationDays: 1.8 },
    paidToCreatorUsd: 1240, platformFeeBps: 500,
    ingredients: [
      { name: "Claude Haiku 4.5", kind: "model", installed: true },
      { name: "Birdeye MCP", kind: "mcp", installed: false },
      { name: "SOL Strategist skill", kind: "skill", installed: false },
      { name: "Mantlescan MCP", kind: "mcp", installed: true },
    ],
    variants: [
      { version: "v1.0", parent: null, genArtSeed: "btc-momentum-7a91-v1", sharpe: 0.9, current: false },
      { version: "v2.1", parent: "v1.0", genArtSeed: "btc-momentum-7a91-v2", sharpe: 1.1, current: false },
      { version: "v3.0", parent: "v2.1", genArtSeed: "btc-momentum-7a91-v3", sharpe: 1.31, current: true },
    ],
    clonesOfYours: { count: 8, upstreamEarningsUsd: 2100 },
    recentBuyers: [
      { label: "0x7c2e…aa07", payerKind: "human", outcome: "+12.4% · 6d", at: "2026-05-26T14:39:00Z" },
      { label: "agent #14", payerKind: "agent", outcome: "running · 2 trades", at: "2026-05-26T14:20:00Z" },
      { label: "0xc0a4…f3b2", payerKind: "human", outcome: "-0.6% · 1d", at: "2026-05-26T05:42:00Z" },
    ],
    creatorOther: [NAMED_LISTINGS[4], NAMED_LISTINGS[5]],
    equityCurve: {
      base: 1000,
      points: Array.from({ length: 90 }, (_, i) => ({
        value: 1000 + i * 6 + Math.sin(i / 5) * 40,
        phase: i < 60 ? ("backtest" as const) : ("live" as const),
      })),
    },
    whatYouGet: ["Full prompts", "Agent topology + ordering", "Threshold values", "MCP/skill config", "Creator notes"],
    whatYouDont: ["Creator's data sources", "Future updates without re-purchase", "Broker credentials"],
    onChain: {
      nft: {
        tokenId: "#0043", lineageId: "btc-momentum", agentURI: "ipfs://bafybeib4xjq2y7l",
        manifestHash: "blake3:7f2b1ad91c4", parentLineage: null, bornAt: "2026-05-13T04:12:00Z",
        operatorSig: "ed25519:7f2b1ad91c4", contract: "0xCa5522Be", network: "mantle-sepolia",
      },
      attestations: [
        { attester: "regime-verifier", verdict: "endorse", targetVersion: "v3.0", at: "2026-05-26T13:30:00Z" },
        { attester: "diversity-check", verdict: "endorse", targetVersion: "v3.0", at: "2026-05-26T13:31:00Z" },
        { attester: "diversity-check", verdict: "question", targetVersion: "v3.1", at: "2026-05-26T10:30:00Z" },
      ],
      anchors: [
        { kind: "merkle", label: "Snapshot · btc-momentum-v3", tx: "0x2e1d44a9", at: "2026-05-26T12:30:00Z", gasEth: "0.0024" },
        { kind: "mint", label: "Identity NFT minted", tx: "0xc0a4f3b2", at: "2026-05-22T05:00:00Z", gasEth: "0.0011" },
        { kind: "commit", label: "SessionCommitment 01H8RTZ", tx: "0x4f8aee01", at: "2026-05-21T18:00:00Z", gasEth: "0.0008" },
      ],
      trades: [
        { at: "2026-05-26T12:30:00Z", action: "close", symbol: "BTC", qty: "0.024", entry: 67420, exit: 68840, pnlUsd: 34.08, pnlPct: 2.1, runner: "0x7c2e…aa07", runnerKind: "human", tx: "0xa83ef12d", anchorTx: "0x2e1d44a9" },
        { at: "2026-05-26T09:00:00Z", action: "close", symbol: "BTC", qty: "0.018", entry: 66910, exit: 67820, pnlUsd: 16.38, pnlPct: 1.4, runner: "agent #14", runnerKind: "agent", tx: "0x4f8adc11", anchorTx: "0x2e1d44a9" },
      ],
      tradesMeta: { totalOnChain: 178, lastAnchorAt: "2026-05-26T12:30:00Z", receiptKind: "TradeBatch", netPnlUsd: 94.88, window: "7d", anchorTx: "0x2e1d44a9" },
    },
  },
};
```

- [ ] **Step 2: Write `creators.ts`, `slices.ts`, `receipts.ts`, `seller.ts`**

```ts
// src/features/marketplace/data/fixtures/creators.ts
import type { CreatorProfile } from "../types";
import { NAMED_LISTINGS } from "./listings";

const ed = { address: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4", handle: "@ed", ens: "ed.xvn" };

export const CREATORS: Record<string, CreatorProfile> = {
  "@ed": {
    creator: ed, joinedAt: "2025-08-12T00:00:00Z", reputation: 4.8, notableTag: "agent #0 contributor",
    counters: { strategies: 3, lifetimeEarnedUsd: 4820, totalBuyers: { humans: 469, agents: 27 }, clonesSpawned: 11, clonesUpstreamUsd: 2100, attestationsIssued: 14 },
    strategies: [
      { ...NAMED_LISTINGS[3], status: "live" },
      { ...NAMED_LISTINGS[4], status: "live" },
      { ...NAMED_LISTINGS[5], status: "live" },
    ],
    earningsWeekly: Array.from({ length: 32 }, (_, i) => 40 + i * i * 4),
    earningsSummary: { last7dUsd: 420, last30dUsd: 1180 },
    forest: {
      nodes: [
        { id: "bm-v1", x: 60, y: 50, label: "v1.0", strategy: "btc-momentum", genArtSeed: "btc-momentum-7a91-v1" },
        { id: "bm-v2", x: 160, y: 50, label: "v2.1", strategy: "btc-momentum", genArtSeed: "btc-momentum-7a91-v2" },
        { id: "bm-v3", x: 260, y: 50, label: "v3.0", strategy: "btc-momentum", current: true, genArtSeed: "btc-momentum-7a91-v3" },
        { id: "cb-1", x: 380, y: 30, label: "@solyana", strategy: "clone-by", external: true },
        { id: "cb-more", x: 380, y: 90, label: "+6 more", strategy: "clone-by", external: true, more: true },
      ],
      edges: [
        { from: "bm-v1", to: "bm-v2" },
        { from: "bm-v2", to: "bm-v3" },
        { from: "bm-v3", to: "cb-1", kind: "clone" },
      ],
    },
    reputationFeed: [
      { direction: "received", verdict: "endorse", attester: "regime-verifier", on: "btc-momentum-v3", at: "2026-05-26T13:30:00Z" },
      { direction: "issued", verdict: "endorse", attester: "@ed", on: "sol-strategist-pro", at: "2026-05-26T06:00:00Z" },
      { direction: "received", verdict: "question", attester: "diversity-check", on: "btc-momentum-v3.1", at: "2026-05-26T10:30:00Z" },
    ],
    clonedBy: [
      { handle: "@solyana", from: "btc-momentum-v3", made: "sol-momentum-v1", earnedUsd: 680, at: "2026-05-24T00:00:00Z" },
      { handle: "@quantnext", from: "btc-momentum-v3", made: "multi-asset-rotation", earnedUsd: 420, at: "2026-05-21T00:00:00Z" },
      { handle: "+7 more", from: "…", made: "—", earnedUsd: 310, at: "2026-04-26T00:00:00Z", more: true },
    ],
  },
};
```

```ts
// src/features/marketplace/data/fixtures/slices.ts
import type { Slice } from "../types";

export const SLICES: Slice[] = [
  { id: "trending", label: "Trending", hint: "weighted by 24h velocity × return", count: 1247, filter: { segment: "trending", sort: "return30d" } },
  { id: "sol-7d", label: "Top on SOL · 7d", hint: "asset=SOL · 7d", count: 142, filter: { assets: ["SOL"], sort: "return30d" } },
  { id: "claude", label: "Top with Claude", hint: "model=Claude", count: 431, filter: { models: ["Claude · Haiku 4.5"], sort: "return30d" } },
  { id: "agents", label: "Most agent-bought", hint: "sort by agent purchases", count: 88, filter: { sort: "buyers", trust: { verifiedOnly: false, acceptsAgents: true, auditedOnly: false } } },
  { id: "newest", label: "Newest 24h", hint: "recently minted", count: 23, filter: { sort: "newest" } },
  { id: "cloned", label: "Most cloned", hint: "sort by clones", count: 64, filter: { sort: "mostCloned" } },
  { id: "free", label: "Free-tier breakouts", hint: "Tier A · ret > 25%", count: 17, filter: { sort: "return30d" } },
];
```

```ts
// src/features/marketplace/data/fixtures/receipts.ts
import type { Receipt } from "../types";

export const RECEIPTS: Record<string, Receipt> = {
  "0xdemo-tx": {
    txHash: "0xdemo-tx", network: "mantle-sepolia", at: "2026-05-26T14:42:00Z", buyer: "0x7c2e…aa07",
    listing: { id: "btc-momentum-v3", version: "v3.0", creator: { address: "0xa83e", handle: "@ed" }, genArtSeed: "btc-momentum-7a91-v3", return30dPct: 47.2, buyers: { humans: 247, agents: 14 } },
    license: { tokenId: "#0184", contract: "0xCa5522Be", manifestHash: "blake3:7f2b1ad91c4", bundleCid: "bafybeib4xjq2y7l", pricePaidUsdc: 49, feeUsdc: 2.45, netToCreatorUsdc: 46.55, mintedAt: "2026-05-26T14:42:00Z" },
    install: {
      xvnDetected: true, xvnEndpoint: "localhost:3000",
      ingredients: [
        { name: "Claude Haiku 4.5", kind: "model", installed: true },
        { name: "Birdeye MCP", kind: "mcp", installed: false },
        { name: "SOL Strategist skill", kind: "skill", installed: false },
        { name: "Mantlescan MCP", kind: "mcp", installed: true },
      ],
    },
    share: {
      ogCard: { id: "btc-momentum-v3", version: "v3.0", creator: { address: "0xa83e", handle: "@ed" }, genArtSeed: "btc-momentum-7a91-v3", return30dPct: 47.2, return30dLabel: "30D", buyers: { humans: 247, agents: 14 }, paidToCreatorUsd: 1240, priceUsdc: 49, verification: "verified", acceptsX402: true, promise: "BTC momentum with Claude regime detection.", url: "xvn.market/lineage/btc-momentum-v3" },
      buyerStamp: "just bought by 0x7c…aa07",
      caption: "I just bought btc-momentum-v3 by @ed — running it now. +47.2% in 30d · 247 humans + 14 agents already running it.",
      variants: ["Just got handed +47% by an autonomous agent", "247 humans run this. Me too now", "@ed's btc-momentum is real. screenshot proof:"],
      notificationHint: "@ed's XVN just got a +$46.55 notification",
    },
  },
};
```

```ts
// src/features/marketplace/data/fixtures/seller.ts
import type { ListableStrategy, PublishDraft } from "../types";

export const LISTABLE_STRATEGIES: ListableStrategy[] = [
  { id: "local-btc-momentum", name: "btc-momentum", version: "v3.0", assets: ["BTC"] },
  { id: "local-eth-mr", name: "eth-mr", version: "v2.0", assets: ["ETH"] },
  { id: "local-wip-draft", name: "wip-draft", version: "v0.1", assets: [] },
];

export function buildPublishDraft(strategyId: string): PublishDraft {
  const s = LISTABLE_STRATEGIES.find((x) => x.id === strategyId);
  const hasAssets = !!s && s.assets.length > 0;
  return {
    strategyId,
    listable: [
      { ok: !!s, label: "Strategy exists in your XVN", reason: s ? undefined : "Strategy not found" },
      { ok: hasAssets, label: "Declares an asset universe", reason: hasAssets ? undefined : "No assets configured" },
      { ok: true, label: "Has a backtest on record" },
    ],
    tier: "sealed",
    priceUsdc: 49,
    acceptedPayers: { humans: true, agents: true },
    ingredients: [
      { name: "Claude Haiku 4.5", kind: "model", installed: true },
      { name: "Birdeye MCP", kind: "mcp", installed: true },
    ],
    preview: {
      id: s?.name ?? strategyId, lineageId: s?.name ?? strategyId, version: s?.version ?? "v0.1",
      creator: { address: "0xa83e", handle: "@ed" }, model: "Claude · Haiku 4.5", style: "Day",
      assets: s?.assets ?? [], return30dPct: 0, sharpe: 0, buyers: { humans: 0, agents: 0 },
      priceUsdc: 49, tier: "sealed", verification: "unverified", acceptsX402: true, clones: 0,
      transferableLicense: false, genArtSeed: `${s?.name ?? strategyId}-preview`,
    },
  };
}
```

```ts
// src/features/marketplace/data/fixtures/viewer.ts
import type { Viewer } from "../types";

// Fixture demo account. Wallet-connect (real viewer) is Phase 6 (A5).
export const VIEWER: Viewer = {
  isConnected: true,
  address: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4",
  handle: "@ed",
  createdListingIds: ["btc-momentum-v3", "btc-grid-v2", "eth-mr-v2"],
  ownedListingIds: ["sol-strategist-pro"],
};
```

- [ ] **Step 3: Write the failing fixture-integrity test**

```ts
// src/features/marketplace/data/fixtures/fixtures.test.ts
import { describe, expect, it } from "vitest";
import { ALL_LISTINGS, LISTING_DETAILS, NAMED_LISTINGS, makeWallListings } from "./listings";
import { CREATORS } from "./creators";
import { SLICES } from "./slices";
import { RECEIPTS } from "./receipts";
import { buildPublishDraft } from "./seller";

describe("fixtures", () => {
  it("wall generator is deterministic and 200 rows", () => {
    expect(makeWallListings()).toHaveLength(200);
    expect(makeWallListings()[5]).toEqual(makeWallListings()[5]);
  });
  it("ALL_LISTINGS includes named + wall", () => {
    expect(ALL_LISTINGS.length).toBe(NAMED_LISTINGS.length + 200);
  });
  it("every detail extends a known row", () => {
    for (const [id, d] of Object.entries(LISTING_DETAILS)) {
      expect(d.id).toBe(id);
      expect(NAMED_LISTINGS.some((r) => r.id === id)).toBe(true);
    }
  });
  it("creator strategies reference real listing fields", () => {
    expect(CREATORS["@ed"].strategies.every((s) => "status" in s)).toBe(true);
  });
  it("slices + receipts present", () => {
    expect(SLICES.length).toBeGreaterThanOrEqual(7);
    expect(RECEIPTS["0xdemo-tx"].license.netToCreatorUsdc).toBeCloseTo(46.55);
  });
  it("publish draft flags missing assets", () => {
    const d = buildPublishDraft("local-wip-draft");
    expect(d.listable.find((c) => c.label.includes("asset"))?.ok).toBe(false);
  });
});
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/data/fixtures/fixtures.test.ts`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/data/fixtures
git commit -m "feat(marketplace): F0 fixtures + 200-row gen-art wall"
```

---

## Task 4: `MarketplaceData` interface + `FixtureMarketplaceData`

**Files:**
- Create: `src/features/marketplace/data/MarketplaceData.ts`
- Test: `src/features/marketplace/data/FixtureMarketplaceData.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// src/features/marketplace/data/FixtureMarketplaceData.test.ts
import { describe, expect, it, vi } from "vitest";
import { FixtureMarketplaceData } from "./MarketplaceData";
import { defaultFilterState } from "./filter";

const mp = new FixtureMarketplaceData();

describe("FixtureMarketplaceData", () => {
  it("lists with totals", async () => {
    const { rows, total, matched } = await mp.listListings(defaultFilterState());
    expect(total).toBeGreaterThan(200);
    expect(rows.length).toBe(matched);
  });
  it("gets a known listing detail", async () => {
    const d = await mp.getListing("btc-momentum-v3");
    expect(d.metrics.winRatePct).toBe(62);
    expect(d.onChain.nft.network).toBe("mantle-sepolia");
  });
  it("rejects unknown listing", async () => {
    await expect(mp.getListing("nope")).rejects.toThrow();
  });
  it("gets a creator by handle", async () => {
    const c = await mp.getCreator("@ed");
    expect(c.counters.strategies).toBe(3);
  });
  it("leaderboard returns slice + rows", async () => {
    const { slice, rows } = await mp.getLeaderboard("sol-7d");
    expect(slice.id).toBe("sol-7d");
    expect(rows.every((r) => r.assets.includes("SOL"))).toBe(true);
  });
  it("publish draft + submit returns a tx", async () => {
    const draft = await mp.createPublishDraft("local-btc-momentum");
    const { txHash } = await mp.submitListing(draft);
    expect(txHash).toMatch(/^0x/);
  });
  it("purchaseIntent returns a TxRef with network (testnet label source)", async () => {
    const ref = await mp.purchaseIntent("btc-momentum-v3");
    expect(ref.txHash).toMatch(/^0x/);
    expect(ref.network).toBe("mantle-sepolia");
  });
  it("exposes a fixture viewer (Mine + clone-gate source)", async () => {
    const v = await mp.getViewer();
    expect(v.isConnected).toBe(true);
    expect(v.createdListingIds).toContain("btc-momentum-v3");
  });
  it("Mine segment filters to the viewer's created listings", async () => {
    const v = await mp.getViewer();
    const { rows } = await mp.listListings({ ...defaultFilterState(), segment: "mine" });
    expect(rows.map((r) => r.id).sort()).toEqual([...v.createdListingIds].sort());
  });
  it("subscribePurchases emits and unsubscribes", () => {
    vi.useFakeTimers();
    const cb = vi.fn();
    const off = mp.subscribePurchases(cb);
    vi.advanceTimersByTime(6000);
    expect(cb).toHaveBeenCalled();
    off();
    cb.mockClear();
    vi.advanceTimersByTime(6000);
    expect(cb).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/data/FixtureMarketplaceData.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```ts
// src/features/marketplace/data/MarketplaceData.ts
import { applyFilter } from "./filter";
import { ALL_LISTINGS, LISTING_DETAILS } from "./fixtures/listings";
import { CREATORS } from "./fixtures/creators";
import { SLICES } from "./fixtures/slices";
import { RECEIPTS } from "./fixtures/receipts";
import { LISTABLE_STRATEGIES, buildPublishDraft } from "./fixtures/seller";
import { VIEWER } from "./fixtures/viewer";
import type {
  CreatorProfile, FilterState, Id, ListableStrategy, ListingDetail, ListingRow,
  MarketplaceStats, PublishDraft, PurchaseEvent, Receipt, Slice, SliceId, TxRef, Viewer,
} from "./types";

export interface MarketplaceData {
  getStats(): Promise<MarketplaceStats>;
  listListings(f: FilterState): Promise<{ rows: ListingRow[]; total: number; matched: number }>;
  getSlices(): Promise<Slice[]>;
  getListing(idOrName: string): Promise<ListingDetail>;
  getCreator(handleOrAddress: string): Promise<CreatorProfile>;
  getLeaderboard(sliceId: SliceId): Promise<{ slice: Slice; rows: ListingRow[] }>;
  getReceipt(txHash: string): Promise<Receipt>;
  getViewer(): Promise<Viewer>;
  listListableStrategies(): Promise<ListableStrategy[]>;
  createPublishDraft(strategyId: string): Promise<PublishDraft>;
  submitListing(d: PublishDraft): Promise<TxRef>;
  purchaseIntent(listingId: Id): Promise<TxRef>;
  cloneIntent(listingId: Id): Promise<TxRef>;
  subscribePurchases(cb: (e: PurchaseEvent) => void): () => void;
}

const fakeTx = (): TxRef => ({
  txHash: `0x${Math.random().toString(16).slice(2).padEnd(8, "0")}`,
  network: "mantle-sepolia",
});

export class FixtureMarketplaceData implements MarketplaceData {
  async getStats(): Promise<MarketplaceStats> {
    return { totalStrategies: 1247, paidThisWeekUsd: 34820, agentPurchases: 218, mintedLast24h: 64 };
  }
  async listListings(f: FilterState) {
    const pool =
      f.segment === "mine"
        ? ALL_LISTINGS.filter((r) => VIEWER.createdListingIds.includes(r.id))
        : ALL_LISTINGS;
    return applyFilter(pool, f);
  }
  async getSlices() {
    return SLICES;
  }
  async getListing(idOrName: string): Promise<ListingDetail> {
    const d = LISTING_DETAILS[idOrName];
    if (!d) throw new Error(`listing not found: ${idOrName}`);
    return d;
  }
  async getCreator(handleOrAddress: string): Promise<CreatorProfile> {
    const c = CREATORS[handleOrAddress];
    if (!c) throw new Error(`creator not found: ${handleOrAddress}`);
    return c;
  }
  async getLeaderboard(sliceId: SliceId) {
    const slice = SLICES.find((s) => s.id === sliceId);
    if (!slice) throw new Error(`slice not found: ${sliceId}`);
    const { rows } = applyFilter(ALL_LISTINGS, { ...baseFilter(), ...slice.filter } as FilterState);
    return { slice, rows };
  }
  async getReceipt(txHash: string): Promise<Receipt> {
    return RECEIPTS[txHash] ?? RECEIPTS["0xdemo-tx"];
  }
  async getViewer(): Promise<Viewer> {
    return VIEWER;
  }
  async listListableStrategies() {
    return LISTABLE_STRATEGIES;
  }
  async createPublishDraft(strategyId: string) {
    return buildPublishDraft(strategyId);
  }
  async submitListing(_d: PublishDraft): Promise<TxRef> {
    return fakeTx();
  }
  async purchaseIntent(_listingId: Id): Promise<TxRef> {
    return fakeTx();
  }
  async cloneIntent(_listingId: Id): Promise<TxRef> {
    return fakeTx();
  }
  subscribePurchases(cb: (e: PurchaseEvent) => void): () => void {
    const id = setInterval(() => {
      cb({
        listingId: "btc-momentum-v3", version: "v3.0", buyer: "0x7c2e…aa07",
        payerKind: Math.random() > 0.5 ? "agent" : "human", amountUsdc: 49, netToCreatorUsdc: 46.55,
        at: new Date().toISOString(),
      });
    }, 5000);
    return () => clearInterval(id);
  }
}

function baseFilter(): FilterState {
  return {
    segment: "trending", search: "", sort: "return30d", assets: [], models: [], styles: [],
    trust: { verifiedOnly: false, acceptsAgents: false, auditedOnly: false },
    priceUsdc: { from: 0, to: 500 }, minBuyers: 0,
  };
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/data/FixtureMarketplaceData.test.ts`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/data/MarketplaceData.ts src/features/marketplace/data/FixtureMarketplaceData.test.ts
git commit -m "feat(marketplace): F0 MarketplaceData interface + fixture impl"
```

---

## Task 5: Context provider + `useMarketplaceData`

**Files:**
- Create: `src/features/marketplace/data/provider.tsx`
- Test: `src/features/marketplace/data/provider.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/data/provider.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MarketplaceDataProvider, useMarketplaceData } from "./provider";
import { FixtureMarketplaceData } from "./MarketplaceData";

function Probe() {
  const mp = useMarketplaceData();
  return <span>{mp instanceof FixtureMarketplaceData ? "fixture" : "other"}</span>;
}

describe("MarketplaceDataProvider", () => {
  it("provides the instance to children", () => {
    render(
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        <Probe />
      </MarketplaceDataProvider>,
    );
    expect(screen.getByText("fixture")).toBeInTheDocument();
  });
  it("throws when used outside provider", () => {
    expect(() => render(<Probe />)).toThrow(/MarketplaceDataProvider/);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/data/provider.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/data/provider.tsx
import { createContext, useContext, type ReactNode } from "react";
import type { MarketplaceData } from "./MarketplaceData";

const Ctx = createContext<MarketplaceData | null>(null);

export function MarketplaceDataProvider({
  client,
  children,
}: {
  client: MarketplaceData;
  children: ReactNode;
}) {
  return <Ctx.Provider value={client}>{children}</Ctx.Provider>;
}

export function useMarketplaceData(): MarketplaceData {
  const mp = useContext(Ctx);
  if (!mp) throw new Error("useMarketplaceData must be used within a MarketplaceDataProvider");
  return mp;
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/data/provider.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/data/provider.tsx src/features/marketplace/data/provider.test.tsx
git commit -m "feat(marketplace): F0 data provider + hook"
```

---

## Task 6: `useFilterState` (URL-synced)

**Files:**
- Create: `src/features/marketplace/hooks/useFilterState.ts`
- Test: `src/features/marketplace/hooks/useFilterState.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/hooks/useFilterState.test.tsx
import { render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { useFilterState } from "./useFilterState";

function Probe() {
  const { filter, setFilter } = useFilterState();
  return (
    <div>
      <span data-testid="assets">{filter.assets.join(",")}</span>
      <span data-testid="sort">{filter.sort}</span>
      <button onClick={() => setFilter({ assets: ["SOL"], sort: "sharpe" })}>set</button>
    </div>
  );
}

describe("useFilterState", () => {
  it("reads initial state from the URL query", () => {
    render(
      <MemoryRouter initialEntries={["/marketplace?assets=BTC,SOL&sort=buyers"]}>
        <Probe />
      </MemoryRouter>,
    );
    expect(screen.getByTestId("assets").textContent).toBe("BTC,SOL");
    expect(screen.getByTestId("sort").textContent).toBe("buyers");
  });
  it("writes updates back to the URL", () => {
    render(
      <MemoryRouter initialEntries={["/marketplace"]}>
        <Probe />
      </MemoryRouter>,
    );
    act(() => screen.getByText("set").click());
    expect(screen.getByTestId("assets").textContent).toBe("SOL");
    expect(screen.getByTestId("sort").textContent).toBe("sharpe");
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/hooks/useFilterState.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```ts
// src/features/marketplace/hooks/useFilterState.ts
import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { defaultFilterState } from "@/features/marketplace/data/filter";
import type { FilterState, SortKey } from "@/features/marketplace/data/types";

const SORTS: SortKey[] = ["return30d", "sharpe", "buyers", "mostCloned", "newest"];
const list = (v: string | null) => (v ? v.split(",").filter(Boolean) : []);

function parse(sp: URLSearchParams): FilterState {
  const base = defaultFilterState();
  const sort = sp.get("sort");
  return {
    ...base,
    segment: (sp.get("segment") as FilterState["segment"]) ?? base.segment,
    search: sp.get("q") ?? "",
    sort: sort && (SORTS as string[]).includes(sort) ? (sort as SortKey) : base.sort,
    assets: list(sp.get("assets")),
    models: list(sp.get("models")),
    styles: list(sp.get("styles")),
    trust: {
      verifiedOnly: sp.get("verified") === "1",
      acceptsAgents: sp.get("agents") === "1",
      auditedOnly: sp.get("audited") === "1",
    },
    minBuyers: Number(sp.get("minBuyers") ?? 0) || 0,
    slice: sp.get("slice") ?? undefined,
  };
}

function serialize(f: FilterState): Record<string, string> {
  const out: Record<string, string> = {};
  if (f.segment !== "trending") out.segment = f.segment;
  if (f.search) out.q = f.search;
  if (f.sort !== "return30d") out.sort = f.sort;
  if (f.assets.length) out.assets = f.assets.join(",");
  if (f.models.length) out.models = f.models.join(",");
  if (f.styles.length) out.styles = f.styles.join(",");
  if (f.trust.verifiedOnly) out.verified = "1";
  if (f.trust.acceptsAgents) out.agents = "1";
  if (f.trust.auditedOnly) out.audited = "1";
  if (f.minBuyers) out.minBuyers = String(f.minBuyers);
  if (f.slice) out.slice = f.slice;
  return out;
}

export function useFilterState() {
  const [sp, setSp] = useSearchParams();
  const filter = useMemo(() => parse(sp), [sp]);
  const setFilter = useCallback(
    (patch: Partial<FilterState>) => setSp(serialize({ ...filter, ...patch }), { replace: true }),
    [filter, setSp],
  );
  return { filter, setFilter };
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/hooks/useFilterState.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/hooks/useFilterState.ts src/features/marketplace/hooks/useFilterState.test.tsx
git commit -m "feat(marketplace): F0 URL-synced filter state"
```

---

## Task 7: `GenArtPlaceholder`

**Files:**
- Create: `src/features/marketplace/components/GenArtPlaceholder.tsx`
- Test: `src/features/marketplace/components/GenArtPlaceholder.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/components/GenArtPlaceholder.test.tsx
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { GenArtPlaceholder } from "./GenArtPlaceholder";

describe("GenArtPlaceholder", () => {
  it("is deterministic for a seed (same gradient stops)", () => {
    const { container: a } = render(<GenArtPlaceholder seed="btc-momentum-7a91-v3" size={80} />);
    const { container: b } = render(<GenArtPlaceholder seed="btc-momentum-7a91-v3" size={80} />);
    expect(a.querySelector("svg")?.innerHTML).toBe(b.querySelector("svg")?.innerHTML);
  });
  it("differs across seeds", () => {
    const { container: a } = render(<GenArtPlaceholder seed="aaa" />);
    const { container: b } = render(<GenArtPlaceholder seed="zzz" />);
    expect(a.querySelector("svg")?.innerHTML).not.toBe(b.querySelector("svg")?.innerHTML);
  });
  it("marks itself a placeholder for later swap", () => {
    const { container } = render(<GenArtPlaceholder seed="x" />);
    expect(container.querySelector('[data-genart="placeholder"]')).not.toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/components/GenArtPlaceholder.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/components/GenArtPlaceholder.tsx
// PLACEHOLDER ONLY — gen-art is unscoped until Phase 4 (program strategy H3/D-2).
// Deterministic, seed-keyed gradient block. The real generator replaces this
// behind the same props (seed, size). Do NOT treat as canonical art.

function fnv1a(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

const HUES = [150, 210, 265, 45, 330, 190]; // green, sky, violet, amber, pink, teal

export function GenArtPlaceholder({
  seed,
  size = 80,
  className = "",
}: {
  seed: string;
  size?: number;
  className?: string;
}) {
  const h = fnv1a(seed);
  const hueA = HUES[h % HUES.length];
  const hueB = HUES[(h >>> 3) % HUES.length];
  const id = `gp-${h.toString(36)}`;
  return (
    <svg
      data-genart="placeholder"
      width={size}
      height={size}
      viewBox="0 0 100 100"
      role="img"
      aria-label="strategy art placeholder"
      className={`block rounded-sm ${className}`}
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor={`hsl(${hueA} 70% 22%)`} />
          <stop offset="100%" stopColor={`hsl(${hueB} 65% 12%)`} />
        </linearGradient>
      </defs>
      <rect x="0" y="0" width="100" height="100" fill={`url(#${id})`} />
      <circle cx={20 + (h % 60)} cy={20 + ((h >>> 5) % 60)} r={10 + (h % 18)} fill={`hsl(${hueA} 80% 55% / 0.35)`} />
      <rect x={(h >>> 7) % 70} y={(h >>> 9) % 70} width="26" height="26" fill={`hsl(${hueB} 80% 60% / 0.25)`} />
    </svg>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/components/GenArtPlaceholder.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/components/GenArtPlaceholder.tsx src/features/marketplace/components/GenArtPlaceholder.test.tsx
git commit -m "feat(marketplace): F0 GenArtPlaceholder (deterministic, swappable)"
```

---

## Task 8: `Sparkline` + `AgentIcon`

**Files:**
- Create: `src/features/marketplace/components/Sparkline.tsx`, `src/features/marketplace/components/AgentIcon.tsx`
- Test: `src/features/marketplace/components/Sparkline.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/components/Sparkline.test.tsx
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Sparkline } from "./Sparkline";

describe("Sparkline", () => {
  it("renders a path with 30 points and is seed-deterministic", () => {
    const { container: a } = render(<Sparkline seed="x" positive />);
    const { container: b } = render(<Sparkline seed="x" positive />);
    const path = a.querySelector("path");
    expect(path).not.toBeNull();
    expect((path!.getAttribute("d") ?? "").match(/L/g)?.length).toBe(29);
    expect(a.innerHTML).toBe(b.innerHTML);
  });
  it("uses danger stroke when negative", () => {
    const { container } = render(<Sparkline seed="x" positive={false} />);
    expect(container.querySelector("path")?.getAttribute("stroke")).toContain("danger");
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/components/Sparkline.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/components/Sparkline.tsx
// Lightweight seeded SVG sparkline (handoff bc2 approach). The uPlot
// MiniSparkline stays for chart-grid use; this is for dense list rows.
function rng(seed: string) {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  let s = h >>> 0;
  return () => {
    s = Math.imul(s ^ (s >>> 15), 2246822507);
    s = Math.imul(s ^ (s >>> 13), 3266489909);
    s = (s ^ (s >>> 16)) >>> 0;
    return (s % 1_000_000) / 1_000_000;
  };
}

export function Sparkline({
  seed,
  positive,
  width = 88,
  height = 24,
}: {
  seed: string;
  positive: boolean;
  width?: number;
  height?: number;
}) {
  const r = rng(seed);
  let v = 50;
  const pts: number[] = [];
  for (let i = 0; i < 30; i++) {
    v += (positive ? 0.6 : -0.4) + (r() - 0.5) * 6;
    v = Math.max(8, Math.min(92, v));
    pts.push(v);
  }
  const d = pts
    .map((p, i) => `${i === 0 ? "M" : "L"} ${((i / 29) * width).toFixed(2)} ${(height - (p / 100) * height).toFixed(2)}`)
    .join(" ");
  return (
    <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} className="block">
      <path
        d={d}
        fill="none"
        stroke={positive ? "var(--gold)" : "var(--danger)"}
        strokeWidth="1.3"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
```

```tsx
// src/features/marketplace/components/AgentIcon.tsx
// Bot glyph used wherever an agent (🤖) count appears — no emoji, brand control.
export function AgentIcon({ size = 11, className = "" }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 12 12"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 ${className}`}
      aria-hidden="true"
    >
      <rect x="2" y="3" width="8" height="6.5" rx="1.5" />
      <circle cx="4.5" cy="6.2" r="0.6" fill="currentColor" />
      <circle cx="7.5" cy="6.2" r="0.6" fill="currentColor" />
      <path d="M6 1.5v1.5" />
      <circle cx="6" cy="1.1" r="0.4" fill="currentColor" />
    </svg>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/components/Sparkline.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/components/Sparkline.tsx src/features/marketplace/components/AgentIcon.tsx src/features/marketplace/components/Sparkline.test.tsx
git commit -m "feat(marketplace): F0 Sparkline + AgentIcon"
```

---

## Task 9: Badges — `AssetPill`, `VerifiedBadge`, `X402Badge`

**Files:**
- Create: `src/features/marketplace/components/AssetPill.tsx`, `VerifiedBadge.tsx`, `X402Badge.tsx`
- Test: `src/features/marketplace/components/badges.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/components/badges.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AssetPill } from "./AssetPill";
import { VerifiedBadge } from "./VerifiedBadge";
import { X402Badge } from "./X402Badge";

describe("badges", () => {
  it("AssetPill shows the ticker and applies a per-asset tone class", () => {
    const { container } = render(<AssetPill asset="BTC" />);
    expect(screen.getByText("BTC")).toBeInTheDocument();
    expect(container.firstElementChild?.className).toContain("text-");
  });
  it("AssetPill falls back gracefully for unknown tickers", () => {
    render(<AssetPill asset="WIF" />);
    expect(screen.getByText("WIF")).toBeInTheDocument();
  });
  it("VerifiedBadge has an accessible title", () => {
    render(<VerifiedBadge />);
    expect(screen.getByTitle(/backtested/i)).toBeInTheDocument();
  });
  it("X402Badge labels x402", () => {
    render(<X402Badge />);
    expect(screen.getByText("x402")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/components/badges.test.tsx`
Expected: FAIL — modules not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/components/AssetPill.tsx
const TONE: Record<string, string> = {
  BTC: "text-[#FBBF24] bg-[#FBBF24]/10 border-[#FBBF24]/20",
  ETH: "text-info bg-info/10 border-info/20",
  SOL: "text-[#A78BFA] bg-[#A78BFA]/10 border-[#A78BFA]/20",
  DOGE: "text-[#F472B6] bg-[#F472B6]/10 border-[#F472B6]/20",
};
const FALLBACK = "text-text-2 bg-surface-elev border-border";

export function AssetPill({ asset }: { asset: string }) {
  return (
    <span
      className={`inline-flex items-center px-1.5 py-0.5 rounded-sm border text-[10px] font-medium tracking-wide ${TONE[asset] ?? FALLBACK}`}
    >
      {asset}
    </span>
  );
}
```

```tsx
// src/features/marketplace/components/VerifiedBadge.tsx
export function VerifiedBadge() {
  return (
    <span
      title="Backtested + live-paper data committed on chain"
      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm border border-gold/40 text-gold text-[10px] font-medium"
    >
      <svg width="10" height="10" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
        <path d="M2.5 6.5l2.2 2.2L9.5 3.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
      Verified
    </span>
  );
}
```

```tsx
// src/features/marketplace/components/X402Badge.tsx
import { AgentIcon } from "./AgentIcon";

export function X402Badge() {
  return (
    <span
      title="Accepts agent-paid auto-purchase (x402)"
      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm border border-gold/40 text-gold text-[10px] font-medium"
    >
      <AgentIcon size={10} />
      x402
    </span>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/components/badges.test.tsx`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/components/AssetPill.tsx src/features/marketplace/components/VerifiedBadge.tsx src/features/marketplace/components/X402Badge.tsx src/features/marketplace/components/badges.test.tsx
git commit -m "feat(marketplace): F0 AssetPill + Verified/X402 badges"
```

---

## Task 10: `RemovableChip` + `TxChip`

**Files:**
- Create: `src/features/marketplace/components/RemovableChip.tsx`, `TxChip.tsx`
- Test: `src/features/marketplace/components/chips.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/components/chips.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RemovableChip } from "./RemovableChip";
import { TxChip } from "./TxChip";

describe("RemovableChip", () => {
  it("fires onRemove when the × is clicked", () => {
    const onRemove = vi.fn();
    render(<RemovableChip onRemove={onRemove}>Asset: BTC</RemovableChip>);
    fireEvent.click(screen.getByRole("button", { name: /remove/i }));
    expect(onRemove).toHaveBeenCalledOnce();
  });
});

describe("TxChip", () => {
  it("shows a truncated hash and an external link", () => {
    render(<TxChip hash="0x2e1d…44a9" />);
    expect(screen.getByText("0x2e1d…44a9")).toBeInTheDocument();
    expect(screen.getByRole("link")).toHaveAttribute("href");
  });
  it("renders a Testnet marker when network is a testnet", () => {
    render(<TxChip hash="0x1" network="mantle-sepolia" />);
    expect(screen.getByText(/testnet/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/components/chips.test.tsx`
Expected: FAIL — modules not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/components/RemovableChip.tsx
import type { ReactNode } from "react";

export function RemovableChip({ children, onRemove }: { children: ReactNode; onRemove: () => void }) {
  return (
    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-sm border border-border-strong bg-surface-elev text-[11px] text-text-2">
      {children}
      <button
        type="button"
        aria-label="remove filter"
        onClick={onRemove}
        className="ml-0.5 leading-none text-text-3 hover:text-text"
      >
        ×
      </button>
    </span>
  );
}
```

```tsx
// src/features/marketplace/components/TxChip.tsx
const TESTNETS = ["mantle-sepolia", "sepolia", "testnet"];

export function TxChip({ hash, label, network }: { hash: string; label?: string; network?: string }) {
  const isTestnet = !!network && TESTNETS.some((t) => network.includes(t));
  return (
    <span className="inline-flex items-center gap-1 font-mono text-[11px] text-text-2">
      {label ? <span className="text-text-3 uppercase tracking-wide">{label}</span> : null}
      {isTestnet ? (
        <span className="px-1 rounded-sm border border-warn/40 text-warn text-[9px] uppercase">Testnet</span>
      ) : null}
      <a
        href={`https://sepolia.mantlescan.xyz/tx/${hash}`}
        target="_blank"
        rel="noreferrer"
        className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm border border-border-strong hover:text-text"
      >
        {hash}
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
          <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </a>
    </span>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/components/chips.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/components/RemovableChip.tsx src/features/marketplace/components/TxChip.tsx src/features/marketplace/components/chips.test.tsx
git commit -m "feat(marketplace): F0 RemovableChip + TxChip (testnet-aware)"
```

---

## Task 11: `FilterDrawer` (docked panel — no popup)

**Files:**
- Create: `src/features/marketplace/components/FilterDrawer.tsx`
- Test: `src/features/marketplace/components/FilterDrawer.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/components/FilterDrawer.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FilterDrawer } from "./FilterDrawer";

describe("FilterDrawer", () => {
  it("does not render content when closed", () => {
    render(
      <FilterDrawer open={false} onClose={() => {}}>
        <p>filters</p>
      </FilterDrawer>,
    );
    expect(screen.queryByText("filters")).not.toBeInTheDocument();
  });
  it("renders content and a close affordance when open", () => {
    const onClose = vi.fn();
    render(
      <FilterDrawer open onClose={onClose}>
        <p>filters</p>
      </FilterDrawer>,
    );
    expect(screen.getByText("filters")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /close/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });
  it("is a docked complementary panel, not a dialog (no-popups rule)", () => {
    render(
      <FilterDrawer open onClose={() => {}}>
        <p>filters</p>
      </FilterDrawer>,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("complementary")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/components/FilterDrawer.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/components/FilterDrawer.tsx
// Docked right-edge panel. NOT a Dialog/Modal/Sheet/Popover (CLAUDE.md
// no-popups rule). It does not trap focus or paint a full-screen overlay
// owning the page; it docks over the list area while the rail/sidebar stay.
import type { ReactNode } from "react";

export function FilterDrawer({
  open,
  onClose,
  title = "Filter strategies",
  children,
}: {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: ReactNode;
}) {
  if (!open) return null;
  return (
    <aside
      aria-label={title}
      className="absolute right-0 top-0 h-full w-[400px] bg-surface-panel border-l border-border shadow-xl flex flex-col"
    >
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <span className="font-sans font-medium text-[15px]">{title}</span>
        <button type="button" aria-label="close filters" onClick={onClose} className="text-text-3 hover:text-text">
          ×
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3">{children}</div>
    </aside>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/components/FilterDrawer.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/components/FilterDrawer.tsx src/features/marketplace/components/FilterDrawer.test.tsx
git commit -m "feat(marketplace): F0 FilterDrawer docked panel (no-popups)"
```

---

## Task 12: `ShareableCard` (OG composition)

**Files:**
- Create: `src/features/marketplace/components/ShareableCard.tsx`
- Test: `src/features/marketplace/components/ShareableCard.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/components/ShareableCard.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ShareableCard } from "./ShareableCard";
import type { ShareableCardData } from "../data/types";

const data: ShareableCardData = {
  id: "btc-momentum-v3", version: "v3.0", creator: { address: "0xa83e", handle: "@ed" },
  genArtSeed: "btc-momentum-7a91-v3", return30dPct: 47.2, return30dLabel: "30D",
  buyers: { humans: 247, agents: 14 }, paidToCreatorUsd: 1240, priceUsdc: 49,
  verification: "verified", acceptsX402: true, promise: "BTC momentum with Claude regime detection.",
  url: "xvn.market/lineage/btc-momentum-v3",
};

describe("ShareableCard", () => {
  it("composes at 1200x630 with title, return and url", () => {
    const { container } = render(<ShareableCard data={data} />);
    const root = container.firstElementChild as HTMLElement;
    expect(root.style.width).toBe("1200px");
    expect(root.style.height).toBe("630px");
    expect(screen.getByText("btc-momentum-v3")).toBeInTheDocument();
    expect(screen.getByText(/47.2%/)).toBeInTheDocument();
    expect(screen.getByText(/xvn\.market\/lineage\/btc-momentum-v3/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/components/ShareableCard.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/components/ShareableCard.tsx
// 1200x630 OG composition — no app chrome. SSR/PNG generation is deferred
// to Phase 6 (A6); this renders the same composition in-app for preview.
import { GenArtPlaceholder } from "./GenArtPlaceholder";
import { AgentIcon } from "./AgentIcon";
import { VerifiedBadge } from "./VerifiedBadge";
import { X402Badge } from "./X402Badge";
import type { ShareableCardData } from "../data/types";

export function ShareableCard({ data }: { data: ShareableCardData }) {
  const pos = data.return30dPct >= 0;
  return (
    <div style={{ width: "1200px", height: "630px" }} className="flex bg-bg text-text overflow-hidden">
      <div className="relative w-[600px] h-full">
        <GenArtPlaceholder seed={data.genArtSeed} size={600} className="!rounded-none" />
        <div className="absolute bottom-4 left-4 px-2 py-1 rounded-sm bg-black/40 backdrop-blur text-[12px] font-mono">
          NFT · MANTLE
        </div>
      </div>
      <div className="w-[600px] h-full p-[38px_44px] flex flex-col justify-between">
        <div className="flex items-center gap-2">
          {data.verification === "verified" ? <VerifiedBadge /> : null}
          {data.acceptsX402 ? <X402Badge /> : null}
        </div>
        <div>
          <h1 className="font-mono text-[44px] font-semibold leading-none">{data.id}</h1>
          <p className="mt-2 text-text-2 text-[15px]">by {data.creator.handle ?? data.creator.address} · {data.version}</p>
          {data.promise ? <p className="mt-3 text-[15px] leading-snug">{data.promise}</p> : null}
        </div>
        <div className="flex items-end justify-between border-t border-border pt-4">
          <div>
            <div className="text-text-3 text-[11px] uppercase tracking-wide">{data.return30dLabel ?? "30D"} RETURN</div>
            <div className={`font-mono text-[64px] font-semibold leading-none ${pos ? "text-gold" : "text-danger"}`}>
              {pos ? "+" : ""}{data.return30dPct}%
            </div>
          </div>
          <div className="text-right">
            <div className="text-text-3 text-[11px] uppercase tracking-wide">Run by</div>
            <div className="inline-flex items-center gap-1 text-[15px]">
              {data.buyers.humans} humans + <AgentIcon /> {data.buyers.agents} agents
            </div>
          </div>
        </div>
        <div className="flex items-center justify-between text-[13px]">
          <span>{data.priceUsdc} USDC · perpetual · ${data.paidToCreatorUsd} paid to creator</span>
          <span className="text-gold font-mono">{data.url}</span>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/components/ShareableCard.test.tsx`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/components/ShareableCard.tsx src/features/marketplace/components/ShareableCard.test.tsx
git commit -m "feat(marketplace): F0 ShareableCard OG composition"
```

---

## Task 13: Routing shell — `MarketplaceLayout` + stubs + wire `routes.tsx`

**Files:**
- Create: `src/features/marketplace/routes/MarketplaceLayout.tsx`, `src/features/marketplace/routes/stubs.tsx`
- Modify: `src/routes.tsx` (add lazy imports + the `/marketplace` subtree)
- Test: `src/features/marketplace/marketplace-routes.test.tsx`

- [ ] **Step 1: Write `MarketplaceLayout` + stubs**

```tsx
// src/features/marketplace/routes/MarketplaceLayout.tsx
import { Outlet } from "react-router-dom";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";

// Phase F mounts the fixture client. Phase 6 swaps this one line.
const client = new FixtureMarketplaceData();

export function MarketplaceLayout() {
  return (
    <MarketplaceDataProvider client={client}>
      <div className="relative">
        <Outlet />
      </div>
    </MarketplaceDataProvider>
  );
}
```

```tsx
// src/features/marketplace/routes/stubs.tsx
// F0 stubs — replaced by real surfaces in F1–F7. Each names its route so the
// routing smoke test and manual nav prove the subtree resolves under the provider.
function Stub({ name }: { name: string }) {
  return (
    <div className="px-7 py-8 text-[13px] text-text-3" data-marketplace-stub={name}>
      Marketplace · {name} — coming in Phase F{name === "browse" ? "1" : ""}.
    </div>
  );
}

export const MarketplaceBrowseStub = () => <Stub name="browse" />;
export const MarketplaceLeaderboardStub = () => <Stub name="leaderboard" />;
export const MarketplaceLineageStub = () => <Stub name="lineage" />;
export const MarketplaceCreatorStub = () => <Stub name="creator" />;
export const MarketplaceSellStub = () => <Stub name="sell" />;
export const MarketplaceReceiptStub = () => <Stub name="receipt" />;
```

- [ ] **Step 2: Wire into `src/routes.tsx`**

Add lazy imports near the other route imports (after the Charts imports, ~line 57):

```ts
const MarketplaceLayout = lazy(() => import("./features/marketplace/routes/MarketplaceLayout").then((m) => ({ default: m.MarketplaceLayout })));
const MarketplaceBrowseStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceBrowseStub })));
const MarketplaceLeaderboardStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceLeaderboardStub })));
const MarketplaceLineageStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceLineageStub })));
const MarketplaceCreatorStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceCreatorStub })));
const MarketplaceSellStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceSellStub })));
const MarketplaceReceiptStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceReceiptStub })));
```

Add the subtree inside the `path: "/"` children array (e.g. after the `charts` block, before `docs`):

```tsx
{
  path: "marketplace",
  element: page(<MarketplaceLayout />),
  children: [
    { index: true, element: page(<MarketplaceBrowseStub />) },
    { path: "leaderboard", element: page(<MarketplaceLeaderboardStub />) },
    { path: "leaderboard/:sliceId", element: page(<MarketplaceLeaderboardStub />) },
    { path: "lineage/:name", element: page(<MarketplaceLineageStub />) },
    { path: "creator/:handleOrAddr", element: page(<MarketplaceCreatorStub />) },
    { path: "sell", element: page(<MarketplaceSellStub />) },
    { path: "receipts/:tx", element: page(<MarketplaceReceiptStub />) },
  ],
},
```

- [ ] **Step 3: Write the failing routing test**

```tsx
// src/features/marketplace/marketplace-routes.test.tsx
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { MarketplaceLayout } from "./routes/MarketplaceLayout";
import { MarketplaceBrowseStub, MarketplaceLineageStub } from "./routes/stubs";

function routerAt(path: string) {
  return createMemoryRouter(
    [
      {
        path: "/marketplace",
        element: <MarketplaceLayout />,
        children: [
          { index: true, element: <MarketplaceBrowseStub /> },
          { path: "lineage/:name", element: <MarketplaceLineageStub /> },
        ],
      },
    ],
    { initialEntries: [path] },
  );
}

describe("marketplace routes", () => {
  it("mounts the browse stub under the data provider", async () => {
    render(<RouterProvider router={routerAt("/marketplace")} />);
    expect(await screen.findByText(/Marketplace · browse/)).toBeInTheDocument();
  });
  it("resolves the lineage route", async () => {
    render(<RouterProvider router={routerAt("/marketplace/lineage/btc-momentum-v3")} />);
    expect(await screen.findByText(/Marketplace · lineage/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 4: Run to verify it passes (and the whole suite + typecheck)**

Run: `pnpm exec vitest run src/features/marketplace`
Expected: PASS (all marketplace tests).
Run: `pnpm exec vitest run src/routes.test.tsx src/routes-code-splitting.test.ts`
Expected: PASS (existing routing tests still green with the new subtree).
Run: `pnpm typecheck`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/routes src/features/marketplace/marketplace-routes.test.tsx src/routes.tsx
git commit -m "feat(marketplace): F0 routing shell + provider mount"
```

---

## Done criteria (F0 frozen)

- [ ] `pnpm exec vitest run src/features/marketplace` is green.
- [ ] `pnpm typecheck` passes.
- [ ] Existing `src/routes.test.tsx` + `src/routes-code-splitting.test.ts` still pass.
- [ ] `/marketplace`, `/marketplace/leaderboard`, `/marketplace/lineage/:name`,
      `/marketplace/creator/:handleOrAddr`, `/marketplace/sell`,
      `/marketplace/receipts/:tx` all resolve (stubs) under the provider.
- [ ] No `Dialog`/`Modal`/`Sheet`/`Popover` introduced.
- [ ] The seam (`types.ts` + `MarketplaceData`) is the single contract; swapping
      `FixtureMarketplaceData` in `MarketplaceLayout` is the only change needed to
      point at real backends (Phase 6).

**Next plans (against frozen F0):** F1 browse → F2 lineage identity + receipts
drawer → F3 creator → F4 leaderboard → F5 sell → F6 receipt → F7 OG card page.
Each is its own writing-plans pass consuming `useMarketplaceData()` + the seam.

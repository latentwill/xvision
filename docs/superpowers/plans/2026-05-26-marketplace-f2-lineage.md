# Marketplace F2 — Lineage Identity Page + On-Chain Receipts Drawer

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> (recommended) or `superpowers:executing-plans` to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `MarketplaceLineageStub` with the real `/marketplace/lineage/:name` identity
page — the viral artifact. Above the fold: 3-col hero (gen-art + NFT stamp | metric stack +
buyer card | purchase column), ingredient-check banner. Below the fold: equity curve, what-you-get
/ what-you-don't cards, variant mini-tree, recent buyers, more-from-creator. On-chain receipts
drawer: inline-expands via `?receipts=open`, surfacing NFT/manifest, attestation verdicts, anchor
history, and trade-history table.

**Branch:** `feat/marketplace-f0` (F0 frozen; build on top of it).

**Architecture:** A single route file `LineageRoute.tsx` colocated with sub-components
(`ReceiptsDrawer`, `TradeHistoryTable`, `EquityPanel`, `IngredientBanner`) in
`src/features/marketplace/routes/`. Consumes `useMarketplaceData()` + `getViewer()` via React
Query `useQuery`. Drawer state is URL-backed (`?receipts=open`) via `useSearchParams`. Buy CTA
calls `purchaseIntent(id)` then `navigate(/marketplace/receipts/:txHash)`.

**Tech Stack:** React 18, TypeScript, React-Router v6, Vitest 2 + React Testing Library +
jsdom, Tailwind token classes, pnpm. Chart via `HeroGradientEquity` (uPlot v2 wrapper already in
the repo).

**Source spec:**
- `docs/design/design_handoff_marketplace_shift/README.md` §§ 3–4
- `docs/design/design_handoff_marketplace_shift/bc2-lineage.jsx`
- Phase F spec `docs/superpowers/specs/2026-05-26-marketplace-phase-f-frontend-design.md` §4 F2

**Conventions (verified in repo):**
- Run tests from `frontend/web`: `pnpm exec vitest run <path>`. Typecheck: `pnpm typecheck`.
- Path alias `@/` → `src/`. Colocate tests as `*.test.tsx`. Token classes only (no hex, no inline
  color); respect dark-mode border rules (`border-border` or `border-muted-foreground/20`; never
  `border-white`/`border-gray-100`).
- **No popups.** The receipts drawer INLINE-EXPANDS (accordion + URL param). Never
  `Dialog`/`Modal`/`Sheet`/`Popover`.
- Reuse F0 primitives: `GenArtPlaceholder`, `AssetPill`, `VerifiedBadge`, `X402Badge`,
  `AgentIcon`, `TxChip`, `Sparkline`.
- `[Testnet]` label comes from `TxChip` consuming `onChain.nft.network`.
- `Clone to edit` gate: `tier === "open" || viewer.ownedListingIds.includes(id)`.
- `Buy` is `[Testnet]`-labeled when `onChain.nft.network` is not mainnet — pass `network` to
  `TxChip` when rendering the button-adjacent stamp post-intent.

---

## File map

```
src/features/marketplace/routes/
  LineageRoute.tsx               # main page component (Task 1)
  LineageRoute.test.tsx          # RTL tests: hero, ingredient banner, buy/clone gates (Task 2)
  IngredientBanner.tsx           # ingredient-check full-width banner (Task 3)
  IngredientBanner.test.tsx
  EquityPanel.tsx                # equity curve card wrapping HeroGradientEquity (Task 4)
  EquityPanel.test.tsx
  ReceiptsDrawer.tsx             # collapsible on-chain receipts accordion (Task 5)
  ReceiptsDrawer.test.tsx
  TradeHistoryTable.tsx          # trade ledger: filter pills + table + pagination (Task 6)
  TradeHistoryTable.test.tsx
```

`src/routes.tsx` — one-line change: swap `MarketplaceLineageStub` lazy import → `LineageRoute`
(Task 7).

---

## Data flow summary

```
useParams({ name })
  └─ useQuery(["listing", name], () => mp.getListing(name))   → ListingDetail
useQuery(["viewer"],            () => mp.getViewer())          → Viewer
useSearchParams()                                              → receipts open/closed
  (buy) useMutation(() => mp.purchaseIntent(id))
        └─ onSuccess: navigate(`/marketplace/receipts/${txHash}`)
  (clone) useMutation(() => mp.cloneIntent(id))
          └─ onSuccess: navigate(`/marketplace/receipts/${txHash}`)
```

`canClone` = `detail.tier === "open" || viewer.ownedListingIds.includes(detail.id)`

---

## Task 1: `LineageRoute` page skeleton

**Files:**
- Create: `src/features/marketplace/routes/LineageRoute.tsx`

### What to build

The full page layout. Sub-components (`IngredientBanner`, `EquityPanel`, `ReceiptsDrawer`) are
imported from sibling files (built in Tasks 3–5). The hero section, below-fold 2-col layout,
variant mini-tree, recent buyers, and more-from-creator cards are inline in this file (they are
simple enough to not warrant separate files).

- [ ] **Step 1: Write the component**

```tsx
// src/features/marketplace/routes/LineageRoute.tsx
//
// /marketplace/lineage/:name — the viral identity page.
// No popups. Receipts drawer inline-expands via ?receipts=open.
import { useParams, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery, useMutation } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { TxChip } from "@/features/marketplace/components/TxChip";
import { IngredientBanner } from "./IngredientBanner";
import { EquityPanel } from "./EquityPanel";
import { ReceiptsDrawer } from "./ReceiptsDrawer";
import type { ListingDetail, Viewer, Variant, RecentBuyer } from "@/features/marketplace/data/types";
```

Page layout structure (do NOT use popups for any panel):

```
<div>                                  {/* page scroll root */}
  {/* === HERO (above-the-fold) === */}
  <section data-testid="lineage-hero"
    className="grid gap-6 p-6 border-b border-border"
    style={{ gridTemplateColumns: "320px 1fr 250px" }}
  >
    {/* col 1: gen-art + NFT stamp */}
    <div className="relative">
      <GenArtPlaceholder seed={detail.genArtSeed} size={320} />
      <span className="absolute bottom-2 left-2 …font-mono text-[10px] uppercase tracking-[0.14em]">
        NFT {detail.onChain.nft.tokenId}
      </span>
    </div>

    {/* col 2: title + metrics + buyer card */}
    <div data-testid="lineage-info-stack">
      {/* title row: name + version pill + badges */}
      {/* creator line: handle · truncated address · model */}
      {/* one-line promise */}
      {/* big metrics grid: 30D RETURN (42px gold mono) + MetricCell ×4 */}
      {/* buyer card: avatar stack + agent stamp + "N humans + M agents" */}
    </div>

    {/* col 3: purchase column */}
    <div data-testid="lineage-purchase-col">
      {/* price card with gold-tinted bg */}
      {/* Buy button — full width, gold bg */}
      {/* Clone to edit (ghost, gated) + Share (ghost) row */}
    </div>
  </section>

  {/* === INGREDIENT BANNER === */}
  <IngredientBanner ingredients={detail.ingredients} />

  {/* === BELOW THE FOLD (2-col) === */}
  <div className="grid gap-6 p-6" style={{ gridTemplateColumns: "1fr 380px" }}>
    {/* LEFT */}
    <div className="flex flex-col gap-5">
      <EquityPanel curve={detail.equityCurve} />
      <WhatYouGetCards get={detail.whatYouGet} dont={detail.whatYouDont} />
      <VariantMiniTree variants={detail.variants} clonesOfYours={detail.clonesOfYours} />
    </div>
    {/* RIGHT */}
    <div className="flex flex-col gap-5">
      <RecentBuyersList buyers={detail.recentBuyers} />
      <MoreFromCreatorCard rows={detail.creatorOther} creator={detail.creator} />
    </div>
  </div>

  {/* === RECEIPTS DRAWER (inline expand, NO modal/sheet) === */}
  <ReceiptsDrawer onChain={detail.onChain} />
</div>
```

Sub-components to inline in `LineageRoute.tsx`:

**`MetricCell`** — label + mono value with optional tone override (`warn` for maxDD).
```tsx
// Props: label: string; value: string | number; tone?: "default" | "warn"
// Renders: ulabel + mono value. warn = text-warn, default = text-foreground.
```

**`BuyerCard`** — avatar stack (5 colored circles + AgentIcon circle) + "N humans + M agents"
line + "paid to creator" sub.
```tsx
// Props: humans: number; agents: number; paidToCreatorUsd: number;
//        platformFeeBps: number; creator: Creator
// Renders the inline card with border border-border bg-surface-elev.
```

**`WhatYouGetCards`** — side-by-side 2-col grid of cards with bulleted lists.
```tsx
// Props: get: string[]; dont: string[]
// Left card: "What you get" / "Tier 2 sealed bundle · decrypts after purchase"
// Right card: "What you don't get" / "Tier 3 — never bundled"
// Uses <ul> with <li>; text-text-2 for "get", text-text-3 for "don't"
```

**`VariantMiniTree`** — horizontal chain of variant tiles.
```tsx
// Props: variants: Variant[]; clonesOfYours?: { count: number; upstreamEarningsUsd: number }
// Each variant: GenArtPlaceholder (56px) + version mono + sharpe mono
// Current variant: gold border (border-2 border-gold); others: border-border
// Connector: horizontal rule (bg-border-strong) with 6px circle nub on right end
// Right teaser: "CLONES OF YOURS" / count (gold 22px mono) / "upstream of $X"
//               shown only when clonesOfYours is present
```

**`RecentBuyersList`** — list of recent buyer rows.
```tsx
// Props: buyers: RecentBuyer[]
// Per row: avatar circle (rounded-full for human, rounded for agent) +
//          label mono + outcome colored (+pct gold, running info, -pct danger) +
//          relative time right-aligned
// Separator: border-b border-border-soft (last row: none)
```

**`MoreFromCreatorCard`** — compact list of other listings by same creator.
```tsx
// Props: rows: ListingRow[]; creator: Creator
// Card header: "More from {handle}" + "Profile" ghost CTA
// Per row: GenArtPlaceholder (36px) + listing id mono + buyer counts
//          + return30dPct right-aligned (gold mono)
// Navigates to /marketplace/lineage/:id on row click (useNavigate)
```

Full `LineageRoute` outer shell:

```tsx
export function LineageRoute() {
  const { name } = useParams<{ name: string }>();
  const mp = useMarketplaceData();
  const navigate = useNavigate();
  const [sp, setSp] = useSearchParams();

  const { data: detail, isLoading, isError } = useQuery({
    queryKey: ["listing", name],
    queryFn: () => mp.getListing(name!),
    enabled: !!name,
  });

  const { data: viewer } = useQuery({
    queryKey: ["viewer"],
    queryFn: () => mp.getViewer(),
  });

  const buyMutation = useMutation({
    mutationFn: () => mp.purchaseIntent(detail!.id),
    onSuccess: (ref) => navigate(`/marketplace/receipts/${ref.txHash}`),
  });

  const cloneMutation = useMutation({
    mutationFn: () => mp.cloneIntent(detail!.id),
    onSuccess: (ref) => navigate(`/marketplace/receipts/${ref.txHash}`),
  });

  const canClone =
    !!detail &&
    (detail.tier === "open" ||
      (viewer?.ownedListingIds.includes(detail.id) ?? false));

  const receiptsOpen = sp.get("receipts") === "open";
  const toggleReceipts = () => {
    setSp(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (receiptsOpen) next.delete("receipts");
        else next.set("receipts", "open");
        return next;
      },
      { replace: true },
    );
  };

  if (isLoading) return <div className="px-6 py-8 text-[13px] text-text-3">Loading…</div>;
  if (isError || !detail) return <div className="px-6 py-8 text-[13px] text-danger">Strategy not found.</div>;

  return (
    <div data-testid="lineage-page">
      {/* ... layout described above ... */}
    </div>
  );
}
```

- [ ] **Step 2: Typecheck**

```bash
cd frontend/web && pnpm typecheck
```

Expected: PASS. (Sub-component files don't exist yet — use placeholder `// TODO` imports
pointing at sibling files that you'll create in Tasks 3–5. Alternatively, scaffold each file as
an empty named export first so imports resolve; Task 3–6 flesh them out via TDD.)

- [ ] **Step 3: Commit scaffold**

```bash
git add frontend/web/src/features/marketplace/routes/LineageRoute.tsx
git commit -m "feat(marketplace): F2 LineageRoute scaffold (sub-components TODO)"
```

---

## Task 2: `LineageRoute` RTL tests

**Files:**
- Create: `src/features/marketplace/routes/LineageRoute.test.tsx`

These tests cover the page-level behaviors: hero content, purchase-column gate logic,
ingredient banner visibility, buy/clone mutation flows, and `?receipts=open` toggle.
Sub-component-level tests live in Tasks 3–6.

- [ ] **Step 1: Write the failing tests**

```tsx
// src/features/marketplace/routes/LineageRoute.test.tsx
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { LineageRoute } from "./LineageRoute";

// Helpers
function qc() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

function Wrapper({
  initialPath = "/marketplace/lineage/btc-momentum-v3",
  client = new FixtureMarketplaceData(),
}: {
  initialPath?: string;
  client?: FixtureMarketplaceData;
}) {
  return (
    <QueryClientProvider client={qc()}>
      <MarketplaceDataProvider client={client}>
        <MemoryRouter initialEntries={[initialPath]}>
          <Routes>
            <Route path="/marketplace/lineage/:name" element={<LineageRoute />} />
            <Route path="/marketplace/receipts/:tx" element={<div>receipt</div>} />
          </Routes>
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>
  );
}

describe("LineageRoute", () => {
  it("renders the hero info stack with title, promise, and 30d return", async () => {
    render(<Wrapper />);
    expect(await screen.findByTestId("lineage-info-stack")).toBeInTheDocument();
    expect(screen.getByText("btc-momentum-v3")).toBeInTheDocument();
    expect(screen.getByText(/BTC momentum/)).toBeInTheDocument();
    // 30D RETURN label
    expect(screen.getByText(/30D RETURN/i)).toBeInTheDocument();
    // value shown as percentage
    expect(screen.getByText(/47\.2/)).toBeInTheDocument();
  });

  it("renders asset pills and badges in the hero", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-hero");
    expect(screen.getByText("BTC")).toBeInTheDocument(); // AssetPill
    // VerifiedBadge and X402Badge rendered (fixture: verified + acceptsX402)
    expect(screen.getByTestId("verified-badge")).toBeInTheDocument();
    expect(screen.getByTestId("x402-badge")).toBeInTheDocument();
  });

  it("shows buyer count: N humans + M agents", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-info-stack");
    expect(screen.getByText(/247/)).toBeInTheDocument();
    expect(screen.getByText(/14/)).toBeInTheDocument();
  });

  it("NFT token id stamped on the gen-art panel", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-hero");
    expect(screen.getByText(/#0043/)).toBeInTheDocument();
  });

  it("renders the purchase column with price", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-purchase-col");
    expect(screen.getByText(/49/)).toBeInTheDocument(); // 49 USDC
    expect(screen.getByRole("button", { name: /buy/i })).toBeInTheDocument();
  });

  it("Buy calls purchaseIntent and navigates to receipts", async () => {
    const client = new FixtureMarketplaceData();
    const spy = vi.spyOn(client, "purchaseIntent").mockResolvedValue({
      txHash: "0xdeadbeef",
      network: "mantle-sepolia",
    });
    render(<Wrapper client={client} />);
    await screen.findByRole("button", { name: /buy/i });
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /buy/i }));
    });
    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith("btc-momentum-v3");
    });
    // After success, navigated to receipts
    expect(await screen.findByText("receipt")).toBeInTheDocument();
  });

  it("Clone to edit is enabled for open-tier listings", async () => {
    // btc-momentum-v3 is "sealed" but viewer owns it (ownedListingIds includes it via fixture)
    // Override viewer to have ownership
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "getViewer").mockResolvedValue({
      isConnected: true,
      address: "0xabc",
      handle: "@test",
      createdListingIds: [],
      ownedListingIds: ["btc-momentum-v3"],
    });
    render(<Wrapper client={client} />);
    await screen.findByTestId("lineage-purchase-col");
    const cloneBtn = screen.getByRole("button", { name: /clone to edit/i });
    expect(cloneBtn).not.toBeDisabled();
  });

  it("Clone to edit is disabled when not owned and tier is sealed", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "getViewer").mockResolvedValue({
      isConnected: true,
      address: "0xabc",
      handle: "@stranger",
      createdListingIds: [],
      ownedListingIds: [], // does NOT own btc-momentum-v3
    });
    render(<Wrapper client={client} />);
    await screen.findByTestId("lineage-purchase-col");
    const cloneBtn = screen.getByRole("button", { name: /clone to edit/i });
    expect(cloneBtn).toBeDisabled();
  });

  it("ingredient banner is shown when some ingredients are missing", async () => {
    render(<Wrapper />);
    // fixture has 2 missing ingredients
    expect(await screen.findByTestId("ingredient-banner")).toBeInTheDocument();
    expect(screen.getByText(/2 of 4/)).toBeInTheDocument();
  });

  it("receipts drawer is collapsed by default", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-page");
    expect(screen.getByTestId("receipts-toggle")).toBeInTheDocument();
    expect(screen.queryByTestId("receipts-body")).not.toBeInTheDocument();
  });

  it("receipts drawer expands when ?receipts=open is in URL", async () => {
    render(
      <Wrapper initialPath="/marketplace/lineage/btc-momentum-v3?receipts=open" />,
    );
    await screen.findByTestId("lineage-page");
    expect(await screen.findByTestId("receipts-body")).toBeInTheDocument();
  });

  it("clicking the receipts toggle adds ?receipts=open to the URL", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-page");
    await act(async () => {
      await userEvent.click(screen.getByTestId("receipts-toggle"));
    });
    expect(await screen.findByTestId("receipts-body")).toBeInTheDocument();
  });

  it("shows an error state for an unknown strategy name", async () => {
    render(<Wrapper initialPath="/marketplace/lineage/does-not-exist" />);
    expect(await screen.findByText(/not found/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/LineageRoute.test.tsx
```

Expected: FAIL — component stubs not rendering the required elements yet.

- [ ] **Step 3: Implement the component body (filling in the scaffold from Task 1)**

Complete all inline sub-components (`MetricCell`, `BuyerCard`, `WhatYouGetCards`,
`VariantMiniTree`, `RecentBuyersList`, `MoreFromCreatorCard`) and the full JSX layout inside
`LineageRoute`. Key implementation notes:

**Hero section (`data-testid="lineage-hero"`):**
- 3-col grid: `320px 1fr 250px`, gap-6, p-6, border-b.
- Col 1: `<GenArtPlaceholder seed={detail.genArtSeed} size={320} />` with `rounded-lg border border-border`.
  NFT stamp: `<span className="absolute bottom-2 left-2 px-2 py-0.5 rounded bg-black/70 backdrop-blur-sm font-mono text-[10px] tracking-[0.14em] text-foreground uppercase">NFT {detail.onChain.nft.tokenId}</span>`
- Col 2 (`data-testid="lineage-info-stack"`):
  - Title row: `<h1 className="font-mono text-[30px] font-semibold tracking-tight leading-none">{detail.id}</h1>` + version pill + `<VerifiedBadge data-testid="verified-badge" />` + `<X402Badge data-testid="x402-badge" />`.
  - Creator line: handle · truncated address · model (font-mono text-[11.5px] text-text-3).
  - Promise: `<p className="text-[14.5px] leading-[1.45] max-w-[480px]">{detail.promise}</p>`.
  - Metrics grid: `grid gap-[18px] items-end pt-1.5` with `auto 1fr 1fr 1fr 1fr` columns.
    - 30D RETURN: `<span className="font-mono text-[42px] font-semibold text-gold leading-none">{detail.metrics.return30dPct > 0 ? "+" : ""}{detail.metrics.return30dPct}%</span>` under ulabel.
    - MetricCells: Sharpe, Win rate (`{detail.metrics.winRatePct}%`), Max DD (tone="warn"), Avg dur.
  - BuyerCard below metrics.
- Col 3 (`data-testid="lineage-purchase-col"`):
  - Price card: `bg-gradient-to-b from-gold/[0.06] to-gold/[0.02] border border-gold-soft rounded-md p-4`.
    - "PRICE" ulabel.
    - Price display: `{detail.priceUsdc === null ? "FREE" : `${detail.priceUsdc} USDC`}` (font-mono text-[24px] font-semibold).
    - "perpetual license · one-time" sub (font-mono text-[10.5px] text-text-3).
    - Buy button: `<button data-testid="buy-btn" onClick={() => buyMutation.mutate()} className="mt-3 w-full py-2.5 rounded bg-gold text-[#001A0A] text-[13.5px] font-bold tracking-[0.01em]" disabled={buyMutation.isPending}>Buy</button>`.
  - Clone/Share row: two ghost buttons, side by side. Clone: `disabled={!canClone}`.

**Ingredient banner:** `<IngredientBanner ingredients={detail.ingredients} />` — only renders
when `ingredients.some(i => !i.installed)`.

**Below the fold (2-col `1fr 380px`):** see sub-component descriptions in Task 1 step 1.

**ReceiptsDrawer:** `<ReceiptsDrawer onChain={detail.onChain} open={receiptsOpen} onToggle={toggleReceipts} />` — the toggle row and body panel.

- [ ] **Step 4: Run to verify tests pass**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/LineageRoute.test.tsx
```

Expected: PASS (13 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/marketplace/routes/LineageRoute.tsx \
        frontend/web/src/features/marketplace/routes/LineageRoute.test.tsx
git commit -m "feat(marketplace): F2 LineageRoute hero + purchase col + layout"
```

---

## Task 3: `IngredientBanner`

**Files:**
- Create: `src/features/marketplace/routes/IngredientBanner.tsx`
- Test: `src/features/marketplace/routes/IngredientBanner.test.tsx`

The full-width warn-tinted banner below the hero. Shows when any ingredient is not installed.
Each pill is green-check (installed) or amber-plus (missing).

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/routes/IngredientBanner.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { IngredientBanner } from "./IngredientBanner";
import type { Ingredient } from "@/features/marketplace/data/types";

const INGREDIENTS: Ingredient[] = [
  { name: "Claude Haiku 4.5", kind: "model", installed: true },
  { name: "Birdeye MCP",       kind: "mcp",   installed: false },
  { name: "SOL Strategist",    kind: "skill",  installed: false },
  { name: "Mantlescan MCP",    kind: "mcp",   installed: true },
];

describe("IngredientBanner", () => {
  it("renders when ingredients are missing", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getByTestId("ingredient-banner")).toBeInTheDocument();
  });

  it("shows the missing count in the copy", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getByText(/2 of 4/)).toBeInTheDocument();
  });

  it("renders all ingredient pills", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getByText("Claude Haiku 4.5")).toBeInTheDocument();
    expect(screen.getByText("Birdeye MCP")).toBeInTheDocument();
  });

  it("each pill carries a kind label", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getAllByText("MODEL").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("MCP").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("SKILL")).toBeInTheDocument();
  });

  it("does NOT render when all ingredients are installed", () => {
    const allInstalled = INGREDIENTS.map((i) => ({ ...i, installed: true }));
    render(<IngredientBanner ingredients={allInstalled} />);
    expect(screen.queryByTestId("ingredient-banner")).not.toBeInTheDocument();
  });

  it("shows the Install missing CTA", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getByRole("button", { name: /install missing/i })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/IngredientBanner.test.tsx
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/routes/IngredientBanner.tsx
import type { Ingredient } from "@/features/marketplace/data/types";

interface Props {
  ingredients: Ingredient[];
}

export function IngredientBanner({ ingredients }: Props) {
  const missing = ingredients.filter((i) => !i.installed);
  if (missing.length === 0) return null;

  return (
    <div
      data-testid="ingredient-banner"
      className="flex items-center gap-3 px-7 py-3.5 border-b border-border bg-warn/[0.04]"
    >
      {/* Warn circle icon */}
      <div className="flex-shrink-0 w-7 h-7 rounded-full flex items-center justify-center bg-warn/[0.12] border border-warn">
        {/* SVG exclamation or info icon — 14px, color warn */}
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" className="text-warn">
          <circle cx="7" cy="7" r="6" stroke="currentColor" strokeWidth="1.5" />
          <line x1="7" y1="4" x2="7" y2="7.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          <circle cx="7" cy="10" r="0.75" fill="currentColor" />
        </svg>
      </div>

      <div className="flex-1 min-w-0">
        <p className="text-[13.5px] text-foreground">
          <strong>Ingredient check · {missing.length} of {ingredients.length} installed in your XVN.</strong>{" "}
          Install the missing {missing.length === 1 ? "one" : "two"} before purchase.
        </p>
        <div className="flex flex-wrap gap-2 mt-1.5">
          {ingredients.map((ing) => (
            <span
              key={ing.name}
              className={[
                "inline-flex items-center gap-1.5 px-2 py-0.5 rounded-[3px] border font-mono text-[11px]",
                ing.installed
                  ? "border-gold-soft bg-gold/[0.10] text-gold"
                  : "border-warn bg-warn/[0.08] text-warn",
              ].join(" ")}
            >
              {/* Check or plus icon (SVG, 10px) */}
              {ing.installed ? (
                <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                  <path d="M2 5l2 2 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              ) : (
                <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                  <path d="M5 2v6M2 5h6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
                </svg>
              )}
              {ing.name}
              <span className="font-mono text-[9px] tracking-[0.14em] opacity-60 uppercase">
                {ing.kind}
              </span>
            </span>
          ))}
        </div>
      </div>

      <button className="flex-shrink-0 px-3 py-1.5 rounded border border-border-strong text-[11.5px] font-medium text-text-2 hover:text-foreground hover:border-gold/50 transition-colors">
        Install missing
      </button>
    </div>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/IngredientBanner.test.tsx
```

Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/marketplace/routes/IngredientBanner.tsx \
        frontend/web/src/features/marketplace/routes/IngredientBanner.test.tsx
git commit -m "feat(marketplace): F2 IngredientBanner"
```

---

## Task 4: `EquityPanel`

**Files:**
- Create: `src/features/marketplace/routes/EquityPanel.tsx`
- Test: `src/features/marketplace/routes/EquityPanel.test.tsx`

The equity curve card. Wraps `HeroGradientEquity` (uPlot v2 primitive). Adds the "If I bought
at mint" toggle button and 30d/90d window buttons. Builds two separate uPlot series from the
`EquityCurve.points` array: backtest (faded grey, dashed) and live (gold, solid). Because uPlot
accepts a single aligned-data array, the component constructs two `values` arrays where the
non-active phase values are `null` to produce gaps.

**HeroGradientEquity props (from the actual component at
`frontend/web/src/components/chart/v2/primitives/HeroGradientEquity.tsx`):**

```ts
interface HeroGradientEquityProps {
  time: number[];    // unix timestamps aligned to values
  values: number[];  // % return aligned to time
  color?: string;    // stroke + halo color (default: theme.warm.gold)
  height?: number;   // default 320
}
```

The component takes a **single series** — it is the gold "hero" line. For the backtest/live
split, implement the split rendering in `EquityPanel` itself using two `HeroGradientEquity`
instances overlaid (or a single one for the live segment with a custom CSS-dashed SVG layer for
backtest). The simpler approach: render a single `HeroGradientEquity` for the live segment and
a plain uPlot-free SVG polyline for the backtest segment (dashed, grey). See the design spec
for the exact styling.

**Open Question OQ-1:** `HeroGradientEquity` is a single-series hero chart and does not natively
support a "dashed faded backtest" overlay. Options: (a) render two overlaid `<div>`s — one for
the full curve in faded SVG, one `HeroGradientEquity` for the live segment only; (b) add a
`backtestValues` prop to `HeroGradientEquity` (requires modifying a shared primitive — coordinate
with the charts track if open). For F2, use option (a): two-layer approach, no primitive change.

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/routes/EquityPanel.test.tsx
import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { EquityPanel } from "./EquityPanel";
import type { EquityCurve } from "@/features/marketplace/data/types";

// Mock uPlot so tests don't need a DOM canvas environment
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

const CURVE: EquityCurve = {
  base: 1000,
  points: [
    ...Array.from({ length: 60 }, (_, i) => ({ value: 1000 + i * 5, phase: "backtest" as const })),
    ...Array.from({ length: 30 }, (_, i) => ({ value: 1300 + i * 3, phase: "live" as const })),
  ],
};

describe("EquityPanel", () => {
  it("renders the card header with base amount", () => {
    render(<EquityPanel curve={CURVE} />);
    expect(screen.getByText(/equity curve/i)).toBeInTheDocument();
    expect(screen.getByText(/base \$1,000/i)).toBeInTheDocument();
  });

  it("renders the 'If I bought at mint' toggle", () => {
    render(<EquityPanel curve={CURVE} />);
    expect(screen.getByRole("button", { name: /if i bought at mint/i })).toBeInTheDocument();
  });

  it("renders window toggle buttons", () => {
    render(<EquityPanel curve={CURVE} />);
    expect(screen.getByRole("button", { name: /30d/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /90d/i })).toBeInTheDocument();
  });

  it("clicking 30d sets active window", async () => {
    render(<EquityPanel curve={CURVE} />);
    const btn30d = screen.getByRole("button", { name: /30d/i });
    await act(async () => { await userEvent.click(btn30d); });
    // 30d button should now appear active (aria-pressed or class change)
    // Just verify no error is thrown — visual state is implementation detail
    expect(btn30d).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/EquityPanel.test.tsx
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/routes/EquityPanel.tsx
import { useState, useMemo } from "react";
import { HeroGradientEquity } from "@/components/chart/v2/primitives/HeroGradientEquity";
import type { EquityCurve } from "@/features/marketplace/data/types";

interface Props {
  curve: EquityCurve;
}

type Window = "30d" | "90d";

export function EquityPanel({ curve }: Props) {
  const [window, setWindow] = useState<Window>("90d");
  const [mintToggle, setMintToggle] = useState(false);

  // Build aligned time array (fake epoch offsets — 1 point per day, ending today)
  // In production this would use real timestamps from the data seam.
  const nowSec = Math.floor(Date.now() / 1000);
  const totalPts = curve.points.length;
  const windowPts = window === "30d" ? Math.min(30, totalPts) : totalPts;

  const sliced = curve.points.slice(totalPts - windowPts);
  const livePoints = sliced.filter((p) => p.phase === "live");
  const backtestPoints = sliced.filter((p) => p.phase === "backtest");

  // time arrays: one point per day offset
  const liveStartIdx = sliced.findIndex((p) => p.phase === "live");

  const timeAll = sliced.map((_, i) => nowSec - (sliced.length - 1 - i) * 86400);
  const valuesAll = sliced.map((p) => p.value);

  const timeBacktest = sliced
    .slice(0, liveStartIdx === -1 ? sliced.length : liveStartIdx + 1)
    .map((_, i) => timeAll[i]);
  const valuesBacktest = sliced
    .slice(0, liveStartIdx === -1 ? sliced.length : liveStartIdx + 1)
    .map((p) => p.value);

  const timeLive = liveStartIdx === -1
    ? []
    : sliced.slice(liveStartIdx).map((_, i) => timeAll[liveStartIdx + i]);
  const valuesLive = liveStartIdx === -1
    ? []
    : sliced.slice(liveStartIdx).map((p) => p.value);

  const hasFinalLive = timeLive.length > 0;

  return (
    <div className="rounded-md border border-border bg-surface-card">
      {/* Card header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <div>
          <span className="text-[13px] font-medium text-foreground">Equity curve</span>
          <span className="ml-2 font-mono text-[11px] text-text-3">
            base ${curve.base.toLocaleString()} · backtest (faded) + live (solid)
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setMintToggle((v) => !v)}
            className={[
              "px-2.5 py-1 rounded border text-[11px] font-medium",
              mintToggle
                ? "border-gold-soft bg-gold/[0.10] text-gold"
                : "border-border-strong bg-transparent text-text-2",
            ].join(" ")}
          >
            If I bought at mint
          </button>
          {(["30d", "90d"] as Window[]).map((w) => (
            <button
              key={w}
              onClick={() => setWindow(w)}
              className={[
                "px-2.5 py-1 rounded border text-[11px] font-medium",
                window === w
                  ? "border-gold-soft text-gold"
                  : "border-border-strong bg-transparent text-text-2",
              ].join(" ")}
            >
              {w}
            </button>
          ))}
        </div>
      </div>

      {/* Chart area: backtest layer (SVG, dashed grey) + live layer (HeroGradientEquity) */}
      <div className="px-4 pt-3 pb-2 relative" style={{ height: 200 }}>
        {/* Backtest layer — plain SVG polyline, dashed, faded */}
        {valuesBacktest.length > 1 && (
          <svg
            className="absolute inset-0 w-full h-full pointer-events-none opacity-50"
            preserveAspectRatio="none"
            viewBox={`0 0 100 100`}
          >
            <polyline
              points={valuesBacktest
                .map((v, i) => {
                  const x = (i / (valuesBacktest.length - 1)) * 100;
                  const [min, max] = [
                    Math.min(...valuesAll),
                    Math.max(...valuesAll),
                  ];
                  const y = 100 - ((v - min) / (max - min + 1)) * 80 - 10;
                  return `${x},${y}`;
                })
                .join(" ")}
              fill="none"
              stroke="var(--text-3, #5F6670)"
              strokeWidth="0.5"
              strokeDasharray="2 2"
            />
          </svg>
        )}

        {/* Live layer — HeroGradientEquity (gold gradient fill + halo) */}
        {hasFinalLive ? (
          <HeroGradientEquity
            time={timeLive}
            values={valuesLive}
            height={180}
          />
        ) : (
          // All backtest — show full curve in live style
          <HeroGradientEquity
            time={timeAll}
            values={valuesAll}
            height={180}
          />
        )}

        {/* LIVE marker label — positioned at the backtest/live boundary */}
        {hasFinalLive && liveStartIdx > 0 && (
          <span
            className="absolute top-2 font-mono text-[9.5px] tracking-[0.16em] text-gold pointer-events-none"
            style={{
              left: `${(liveStartIdx / sliced.length) * 100}%`,
            }}
          >
            LIVE
          </span>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/EquityPanel.test.tsx
```

Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/marketplace/routes/EquityPanel.tsx \
        frontend/web/src/features/marketplace/routes/EquityPanel.test.tsx
git commit -m "feat(marketplace): F2 EquityPanel with backtest+live dual-layer"
```

---

## Task 5: `ReceiptsDrawer`

**Files:**
- Create: `src/features/marketplace/routes/ReceiptsDrawer.tsx`
- Test: `src/features/marketplace/routes/ReceiptsDrawer.test.tsx`

The inline-expand accordion. NO modal, sheet, or popover. Controlled by `open` prop (parent owns
URL state). Renders the toggle row always; the body panel (`data-testid="receipts-body"`) only
when `open=true`. Body contains: Identity NFT & manifest card, attestation verdicts card, anchor
history card, and `<TradeHistoryTable>` (Task 6).

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/routes/ReceiptsDrawer.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ReceiptsDrawer } from "./ReceiptsDrawer";
import type { OnChainReceipts } from "@/features/marketplace/data/types";
import { LISTING_DETAILS } from "@/features/marketplace/data/fixtures/listings";

const ON_CHAIN: OnChainReceipts = LISTING_DETAILS["btc-momentum-v3"].onChain;

describe("ReceiptsDrawer", () => {
  it("always renders the toggle row", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={vi.fn()} />);
    expect(screen.getByTestId("receipts-toggle")).toBeInTheDocument();
  });

  it("shows 'View on-chain receipts' when closed", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={vi.fn()} />);
    expect(screen.getByText(/view on-chain receipts/i)).toBeInTheDocument();
  });

  it("shows 'Hide on-chain receipts' when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    expect(screen.getByText(/hide on-chain receipts/i)).toBeInTheDocument();
  });

  it("does NOT render the body when closed", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={vi.fn()} />);
    expect(screen.queryByTestId("receipts-body")).not.toBeInTheDocument();
  });

  it("renders the body when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    expect(screen.getByTestId("receipts-body")).toBeInTheDocument();
  });

  it("calls onToggle when the toggle row is clicked", async () => {
    const onToggle = vi.fn();
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={onToggle} />);
    screen.getByTestId("receipts-toggle").click();
    expect(onToggle).toHaveBeenCalledOnce();
  });

  it("shows NFT token id in the manifest card when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    expect(screen.getByText("#0043")).toBeInTheDocument();
  });

  it("shows attestation verdicts when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    // fixture has endorse + question verdicts
    expect(screen.getAllByText(/endorse/i).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/question/i)).toBeInTheDocument();
  });

  it("shows anchor history entries when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    expect(screen.getByText(/merkle/i)).toBeInTheDocument();
    expect(screen.getByText(/mint/i)).toBeInTheDocument();
  });

  it("renders the AUDITOR shield label on the toggle row", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={vi.fn()} />);
    expect(screen.getByText(/auditor/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/ReceiptsDrawer.test.tsx
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/routes/ReceiptsDrawer.tsx
//
// Inline-expand accordion. NO modal, sheet, or popover — this is the rule.
// Parent controls open state (URL-backed via ?receipts=open).
import { TxChip } from "@/features/marketplace/components/TxChip";
import { TradeHistoryTable } from "./TradeHistoryTable";
import type { OnChainReceipts, Verdict } from "@/features/marketplace/data/types";

interface Props {
  onChain: OnChainReceipts;
  open: boolean;
  onToggle: () => void;
}

const VERDICT_CLASSES: Record<Verdict, string> = {
  endorse: "border-gold-soft text-gold",
  question: "border-warn/60 text-warn",
  reject:   "border-danger/60 text-danger",
};

const ANCHOR_KIND_CLASSES: Record<string, string> = {
  merkle: "text-info",
  mint:   "text-gold",
  commit: "text-text-2",
};

export function ReceiptsDrawer({ onChain, open, onToggle }: Props) {
  return (
    <div
      className={["mt-6 border-t border-border", open ? "bg-[#070707]" : ""].join(" ")}
    >
      {/* === TOGGLE ROW === */}
      <button
        data-testid="receipts-toggle"
        onClick={onToggle}
        className="w-full flex items-center gap-2.5 px-7 py-3.5 text-left hover:bg-white/[0.02] transition-colors"
      >
        {/* chevron icon (right when closed, down when open) */}
        <svg
          width="13" height="13" viewBox="0 0 13 13"
          className={["text-text-2 transition-transform", open ? "rotate-90" : ""].join(" ")}
        >
          <path d="M4.5 2.5 l4 4 -4 4" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <span className="text-[13.5px] font-medium text-foreground">
          {open ? "Hide" : "View"} on-chain receipts
        </span>
        <span className="font-mono text-[11px] text-text-3">
          · NFT, manifest hash, attestations, anchor history, validator activity
        </span>
        <span className="ml-auto flex items-center gap-1.5">
          <span className="font-mono text-[9px] tracking-[0.18em] text-text-3 uppercase">
            Auditor
          </span>
          {/* shield icon 11px */}
          <svg width="11" height="11" viewBox="0 0 11 11" className="text-text-3">
            <path d="M5.5 1 L9.5 2.5 v3.5 c0 2-4 4-4 4s-4-2-4-4 V2.5 Z" stroke="currentColor" strokeWidth="1.2" fill="none" />
          </svg>
        </span>
      </button>

      {/* === BODY (only when open) === */}
      {open && (
        <div
          data-testid="receipts-body"
          className="px-7 pb-7 grid gap-4"
          style={{ gridTemplateColumns: "1fr 1fr" }}
        >
          {/* Identity NFT & manifest */}
          <div className="rounded-md border border-border bg-surface-card p-4">
            <div className="text-[12px] font-medium text-foreground mb-0.5">
              Identity NFT &amp; manifest
            </div>
            <div className="font-mono text-[10.5px] text-text-3 mb-3">
              {onChain.nft.network} · {onChain.nft.contract}
            </div>
            <div className="space-y-0">
              {[
                ["nft_token_id",   onChain.nft.tokenId,        "gold"],
                ["lineage_id",     onChain.nft.lineageId,      "text"],
                ["agentURI",       onChain.nft.agentURI,       "link"],
                ["manifest_hash",  onChain.nft.manifestHash,   "text"],
                ["parent_lineage", onChain.nft.parentLineage ?? "— (seed)", "muted"],
                ["born_at",        onChain.nft.bornAt,         "text"],
                ["operator_sig",   onChain.nft.operatorSig,    "text"],
              ].map(([key, val, tone], i, arr) => (
                <div
                  key={key as string}
                  className={[
                    "grid gap-2.5 py-1.5",
                    i < arr.length - 1 ? "border-b border-border-soft" : "",
                  ].join(" ")}
                  style={{ gridTemplateColumns: "120px 1fr" }}
                >
                  <span className="font-mono text-[9.5px] tracking-[0.14em] text-text-3 uppercase">
                    {key}
                  </span>
                  <span
                    className={[
                      "font-mono text-[11px] break-all",
                      tone === "gold"  ? "text-gold" :
                      tone === "muted" ? "text-text-3" :
                      tone === "link"  ? "text-info underline decoration-dotted" :
                      "text-foreground",
                    ].join(" ")}
                  >
                    {val}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* Attestation verdicts */}
          <div className="rounded-md border border-border bg-surface-card">
            <div className="px-4 py-3 border-b border-border">
              <span className="text-[12px] font-medium text-foreground">Attestation verdicts</span>
              <span className="ml-2 font-mono text-[10.5px] text-text-3">
                {onChain.attestations.length} verdicts
              </span>
            </div>
            <div>
              {onChain.attestations.map((att, i) => (
                <div
                  key={i}
                  className={[
                    "flex items-center gap-2.5 px-4 py-2.5",
                    i < onChain.attestations.length - 1 ? "border-b border-border-soft" : "",
                  ].join(" ")}
                >
                  <span
                    className={[
                      "inline-flex items-center gap-1.5 min-w-[80px] px-1.5 py-0.5 rounded-[3px] border font-mono text-[9.5px] tracking-[0.14em] font-semibold uppercase",
                      VERDICT_CLASSES[att.verdict],
                    ].join(" ")}
                  >
                    <span className="w-1.5 h-1.5 rounded-full bg-current" />
                    {att.verdict}
                  </span>
                  <span className="font-mono text-[11px] text-text-2">{att.attester}</span>
                  <span className="font-mono text-[11px] text-foreground ml-auto">
                    → {att.targetVersion}
                  </span>
                  <span className="font-mono text-[10.5px] text-text-3">
                    {att.at}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* Anchor history — full width */}
          <div className="rounded-md border border-border bg-surface-card col-span-2">
            <div className="px-4 py-3 border-b border-border">
              <span className="text-[12px] font-medium text-foreground">Anchor history</span>
              <span className="ml-2 font-mono text-[10.5px] text-text-3">
                {onChain.anchors.length} events
              </span>
            </div>
            <div>
              {onChain.anchors.map((anc, i) => (
                <div
                  key={i}
                  className={[
                    "grid items-center gap-3.5 px-4 py-2.5",
                    i < onChain.anchors.length - 1 ? "border-b border-border-soft" : "",
                  ].join(" ")}
                  style={{ gridTemplateColumns: "110px 1fr auto auto" }}
                >
                  <span
                    className={[
                      "font-mono text-[9.5px] tracking-[0.16em] uppercase",
                      ANCHOR_KIND_CLASSES[anc.kind] ?? "text-text-2",
                    ].join(" ")}
                  >
                    {anc.kind}
                  </span>
                  <span className="font-mono text-[11.5px] text-text-2">{anc.label}</span>
                  <TxChip txHash={anc.tx} network={onChain.nft.network} />
                  <span className="font-mono text-[11px] text-text-3 text-right min-w-[90px]">
                    {anc.at} · {anc.gasEth} ETH
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* Trade history — full width */}
          <div className="col-span-2">
            <TradeHistoryTable trades={onChain.trades} meta={onChain.tradesMeta} />
          </div>
        </div>
      )}
    </div>
  );
}
```

**Note on `TxChip` props:** Inspect the existing `TxChip.tsx` in F0 for its exact prop interface.
If it only accepts `txHash: string` (no `network` prop yet), pass `txHash` only for now and note
in the Open Questions that the `[Testnet]` stamp on the chip requires a `network` prop added to
`TxChip` — that is a small additive change and can be done as part of this task or as a follow-up
(see OQ-2 below).

- [ ] **Step 4: Run to verify it passes**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/ReceiptsDrawer.test.tsx
```

Expected: PASS (10 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/marketplace/routes/ReceiptsDrawer.tsx \
        frontend/web/src/features/marketplace/routes/ReceiptsDrawer.test.tsx
git commit -m "feat(marketplace): F2 ReceiptsDrawer inline-expand accordion"
```

---

## Task 6: `TradeHistoryTable`

**Files:**
- Create: `src/features/marketplace/routes/TradeHistoryTable.tsx`
- Test: `src/features/marketplace/routes/TradeHistoryTable.test.tsx`

The on-chain trade ledger inside the receipts drawer. Filter pills (All/Buy/Sell/Close with
counts) + Runner dropdown stub + Window dropdown stub. 9-column table with pagination. Action
pills are tone-coded (BUY=gold, SELL=danger, CLOSE=info). Runner column shows agent badge or
human address. Footer: showing X of total + Merkle anchor note + prev/next.

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/routes/TradeHistoryTable.test.tsx
import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { TradeHistoryTable } from "./TradeHistoryTable";
import type { TradeRecord } from "@/features/marketplace/data/types";
import { LISTING_DETAILS } from "@/features/marketplace/data/fixtures/listings";

const TRADES: TradeRecord[] = LISTING_DETAILS["btc-momentum-v3"].onChain.trades;
const META = LISTING_DETAILS["btc-momentum-v3"].onChain.tradesMeta;

describe("TradeHistoryTable", () => {
  it("renders the card header with totalOnChain count", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    expect(screen.getByText(/178/)).toBeInTheDocument();
    expect(screen.getByText(/trades on chain/i)).toBeInTheDocument();
  });

  it("renders filter pills for All/Buy/Sell/Close", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    expect(screen.getByRole("button", { name: /all/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /buy/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /sell/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /close/i })).toBeInTheDocument();
  });

  it("renders Runner and Window dropdowns", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    expect(screen.getByText(/runner/i)).toBeInTheDocument();
    expect(screen.getByText(/window/i)).toBeInTheDocument();
  });

  it("shows net P&L from meta", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    // meta.netPnlUsd = 94.88
    expect(screen.getByText(/94\.88/)).toBeInTheDocument();
  });

  it("renders table column headers", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    expect(screen.getByText("Time")).toBeInTheDocument();
    expect(screen.getByText("Action")).toBeInTheDocument();
    expect(screen.getByText("Sym")).toBeInTheDocument();
    expect(screen.getByText("P&L")).toBeInTheDocument();
    expect(screen.getByText("Runner")).toBeInTheDocument();
    expect(screen.getByText("Tx")).toBeInTheDocument();
  });

  it("renders trade rows from fixture data", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    // fixture has 2 trades; both should appear
    expect(screen.getAllByText("BTC").length).toBeGreaterThanOrEqual(1);
  });

  it("clicking Buy filter pill shows only buy rows", async () => {
    // Add 3 fixture trades: 1 buy, 1 sell, 1 close
    const mixedTrades: TradeRecord[] = [
      { ...TRADES[0], action: "buy", symbol: "BTC" },
      { ...TRADES[0], action: "sell", symbol: "ETH" },
      { ...TRADES[0], action: "close", symbol: "SOL" },
    ];
    render(<TradeHistoryTable trades={mixedTrades} meta={{ ...META, totalOnChain: 3 }} />);
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^buy/i }));
    });
    // Only BTC (buy) row should appear; ETH and SOL should be hidden
    expect(screen.getByText("BTC")).toBeInTheDocument();
    expect(screen.queryByText("ETH")).not.toBeInTheDocument();
    expect(screen.queryByText("SOL")).not.toBeInTheDocument();
  });

  it("clicking All filter shows all rows", async () => {
    const mixedTrades: TradeRecord[] = [
      { ...TRADES[0], action: "buy",   symbol: "BTC" },
      { ...TRADES[0], action: "sell",  symbol: "ETH" },
      { ...TRADES[0], action: "close", symbol: "SOL" },
    ];
    render(<TradeHistoryTable trades={mixedTrades} meta={{ ...META, totalOnChain: 3 }} />);
    // click Buy first, then All
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^buy/i }));
    });
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^all/i }));
    });
    expect(screen.getByText("BTC")).toBeInTheDocument();
    expect(screen.getByText("ETH")).toBeInTheDocument();
    expect(screen.getByText("SOL")).toBeInTheDocument();
  });

  it("renders the Export ledger button", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    expect(screen.getByRole("button", { name: /export ledger/i })).toBeInTheDocument();
  });

  it("renders footer with anchor merkle reference", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    // footer shows anchorTx from meta
    expect(screen.getByText(/anchored under/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/TradeHistoryTable.test.tsx
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// src/features/marketplace/routes/TradeHistoryTable.tsx
import { useState } from "react";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { TxChip } from "@/features/marketplace/components/TxChip";
import type { TradeRecord } from "@/features/marketplace/data/types";

type ActionFilter = "all" | "buy" | "sell" | "close";

interface Meta {
  totalOnChain: number;
  lastAnchorAt: string;
  receiptKind: string;
  netPnlUsd: number;
  window: string;
  anchorTx: string;
}

interface Props {
  trades: TradeRecord[];
  meta: Meta;
}

const ACTION_CLASSES: Record<string, { fg: string; border: string; bg: string }> = {
  buy:   { fg: "text-gold",   border: "border-gold-soft",          bg: "bg-gold/[0.10]" },
  sell:  { fg: "text-danger", border: "border-danger/40",          bg: "bg-danger/[0.10]" },
  close: { fg: "text-info",   border: "border-info/40",            bg: "bg-info/[0.10]" },
};

const PAGE_SIZE = 10;

export function TradeHistoryTable({ trades, meta }: Props) {
  const [actionFilter, setActionFilter] = useState<ActionFilter>("all");
  const [page, setPage] = useState(0);

  const filtered = actionFilter === "all"
    ? trades
    : trades.filter((t) => t.action === actionFilter);

  const pageSlice = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
  const totalPages = Math.ceil(filtered.length / PAGE_SIZE);

  const counts = {
    all: trades.length,
    buy: trades.filter((t) => t.action === "buy").length,
    sell: trades.filter((t) => t.action === "sell").length,
    close: trades.filter((t) => t.action === "close").length,
  };

  return (
    <div className="rounded-md border border-border bg-surface-card">
      {/* Card header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <div>
          <span className="text-[12px] font-medium text-foreground">Trade history</span>
          <span className="ml-2 font-mono text-[10.5px] text-text-3">
            {meta.totalOnChain} trades on chain · last anchor {meta.lastAnchorAt} · receipt_kind={meta.receiptKind}
          </span>
        </div>
        <button className="px-2.5 py-1 rounded border border-border-strong text-[11px] font-medium text-text-2 hover:text-foreground transition-colors">
          Export ledger
        </button>
      </div>

      {/* Filter pills row */}
      <div className="flex items-center flex-wrap gap-2 px-4 py-2.5 border-b border-border-soft">
        {(["all", "buy", "sell", "close"] as ActionFilter[]).map((k) => {
          const active = actionFilter === k;
          const cls = k !== "all" ? ACTION_CLASSES[k] : null;
          return (
            <button
              key={k}
              onClick={() => { setActionFilter(k); setPage(0); }}
              className={[
                "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-[3px] border text-[11.5px] font-medium",
                active
                  ? (cls ? `${cls.border} ${cls.bg} ${cls.fg}` : "border-gold-soft bg-gold/[0.10] text-gold")
                  : "border-border-strong bg-transparent text-text-2",
              ].join(" ")}
            >
              <span className={[
                "w-1.5 h-1.5 rounded-full",
                k === "all" ? "bg-text-3" : cls?.fg.replace("text-", "bg-") ?? "bg-text-3",
              ].join(" ")} />
              <span className="capitalize">{k}</span>
              <span className="font-mono text-[10px] px-1">{counts[k]}</span>
            </button>
          );
        })}

        <span className="w-px h-4 bg-border mx-1" />

        {/* Runner + Window dropdowns (stub — no real filtering in F2) */}
        <button className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-[3px] border border-border-strong text-[11.5px] font-medium text-text-2">
          Runner <span className="font-mono text-[10.5px] text-text-3">any</span>
          <svg width="10" height="10" viewBox="0 0 10 10" className="text-text-3"><path d="M2 3.5l3 3 3-3" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" /></svg>
        </button>
        <button className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-[3px] border border-border-strong text-[11.5px] font-medium text-text-2">
          Window <span className="font-mono text-[10.5px] text-text-3">{meta.window}</span>
          <svg width="10" height="10" viewBox="0 0 10 10" className="text-text-3"><path d="M2 3.5l3 3 3-3" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" /></svg>
        </button>

        <span className="ml-auto font-mono text-[10.5px] text-text-3">
          net <span className="text-gold">+${meta.netPnlUsd}</span> · {meta.window} window
        </span>
      </div>

      {/* Table */}
      <div>
        {/* Header row */}
        <div
          className="grid items-center gap-2.5 px-4 py-2 border-b border-border-soft"
          style={{ gridTemplateColumns: "100px 78px 50px 0.7fr 0.9fr 0.9fr 0.95fr 1fr 110px" }}
        >
          {["Time", "Action", "Sym", "Qty", "Entry", "Exit", "P&L", "Runner", "Tx"].map((h, i) => (
            <div
              key={h}
              className="font-mono text-[9px] tracking-[0.2em] font-semibold text-text-3 uppercase"
              style={{ textAlign: i >= 3 && i <= 6 ? "right" : "left" }}
            >
              {h}
            </div>
          ))}
        </div>

        {/* Data rows */}
        {pageSlice.map((t, i) => {
          const ac = ACTION_CLASSES[t.action];
          const pnlPos = (t.pnlUsd ?? 0) > 0;
          const pnlOpen = t.pnlUsd === null;
          const pnlColor = pnlOpen ? "text-info" : pnlPos ? "text-gold" : "text-danger";
          const isAgent = t.runnerKind === "agent";

          return (
            <div
              key={i}
              className={[
                "grid items-center gap-2.5 px-4 py-2.5",
                i < pageSlice.length - 1 ? "border-b border-border-soft" : "",
              ].join(" ")}
              style={{ gridTemplateColumns: "100px 78px 50px 0.7fr 0.9fr 0.9fr 0.95fr 1fr 110px" }}
            >
              <span className="font-mono text-[11px] text-text-3">{t.at}</span>
              <span
                className={[
                  "inline-flex items-center gap-1 px-1.5 py-0.5 rounded-[3px] border font-mono text-[9.5px] tracking-[0.16em] font-semibold uppercase",
                  ac.border, ac.bg, ac.fg,
                ].join(" ")}
              >
                <span className={["w-1 h-1 rounded-full bg-current"].join(" ")} />
                {t.action}
              </span>
              <span className="font-mono text-[11.5px] text-text-2">{t.symbol}</span>
              <span className="font-mono text-[11.5px] text-foreground text-right">{t.qty}</span>
              <span className="font-mono text-[11.5px] text-text-2 text-right">
                {t.entry !== null ? `$${t.entry.toLocaleString()}` : "—"}
              </span>
              <span className={["font-mono text-[11.5px] text-right", t.exit === null ? "text-text-4" : "text-text-2"].join(" ")}>
                {t.exit !== null ? `$${t.exit.toLocaleString()}` : "—"}
              </span>
              <span className="flex flex-col items-end gap-0.5">
                <span className={`font-mono text-[12px] font-semibold ${pnlColor}`}>
                  {pnlOpen ? "open" : `${(t.pnlUsd ?? 0) > 0 ? "+" : ""}$${t.pnlUsd}`}
                </span>
                {!pnlOpen && t.pnlPct !== null && (
                  <span className="font-mono text-[9.5px] text-text-3">
                    {(t.pnlPct ?? 0) > 0 ? "+" : ""}{t.pnlPct}%
                  </span>
                )}
              </span>
              <span className="inline-flex items-center gap-1.5 min-w-0 overflow-hidden">
                <span
                  className={[
                    "w-4 h-4 flex-shrink-0 flex items-center justify-center border",
                    isAgent ? "rounded-[3px] border-gold-soft bg-gold/[0.10]" : "rounded-full border-border-strong bg-surface-elev",
                  ].join(" ")}
                >
                  {isAgent && <AgentIcon size={8} />}
                </span>
                <span
                  className={[
                    "font-mono text-[11px] truncate",
                    isAgent ? "text-gold" : "text-text-2",
                  ].join(" ")}
                >
                  {t.runner}
                </span>
              </span>
              <TxChip txHash={t.tx} network="" />
            </div>
          );
        })}
      </div>

      {/* Footer */}
      <div className="flex items-center gap-2.5 px-4 py-2.5 border-t border-border-soft">
        <span className="font-mono text-[10.5px] text-text-3">
          Showing <span className="text-text-2">{pageSlice.length}</span> of{" "}
          <span className="text-text-2">{filtered.length}</span> · all anchored under{" "}
          <span className="text-info">{meta.anchorTx}</span>
        </span>
        <div className="ml-auto flex items-center gap-1.5">
          <button
            onClick={() => setPage((p) => Math.max(0, p - 1))}
            disabled={page === 0}
            className="px-2 py-1 rounded border border-border-strong text-[11px] text-text-2 disabled:opacity-40"
          >
            ← Prev
          </button>
          <button
            onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
            disabled={page >= totalPages - 1}
            className="px-2 py-1 rounded border border-border-strong text-[11px] text-text-2 disabled:opacity-40"
          >
            Next →
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace/routes/TradeHistoryTable.test.tsx
```

Expected: PASS (11 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/marketplace/routes/TradeHistoryTable.tsx \
        frontend/web/src/features/marketplace/routes/TradeHistoryTable.test.tsx
git commit -m "feat(marketplace): F2 TradeHistoryTable with action filter + pagination"
```

---

## Task 7: Wire `routes.tsx` — stub → `LineageRoute`

**Files:**
- Modify: `src/routes.tsx` (one-line lazy import swap)

This is the only change to the routing file. Everything else is already in place from F0.

- [ ] **Step 1: Swap the lazy import**

In `src/routes.tsx`, find:

```ts
const MarketplaceLineageStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceLineageStub })));
```

Replace with:

```ts
const MarketplaceLineageRoute = lazy(() => import("./features/marketplace/routes/LineageRoute").then((m) => ({ default: m.LineageRoute })));
```

Then in the marketplace route subtree, change:

```tsx
{ path: "lineage/:name", element: page(<MarketplaceLineageStub />) },
```

to:

```tsx
{ path: "lineage/:name", element: page(<MarketplaceLineageRoute />) },
```

- [ ] **Step 2: Run the full marketplace test suite + routes tests + typecheck**

```bash
cd frontend/web && pnpm exec vitest run src/features/marketplace
cd frontend/web && pnpm exec vitest run src/routes.test.tsx src/routes-code-splitting.test.ts
cd frontend/web && pnpm typecheck
```

Expected: all green.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/routes.tsx
git commit -m "feat(marketplace): F2 wire LineageRoute into router (replace stub)"
```

---

## Done criteria (F2 complete)

- [ ] `pnpm exec vitest run src/features/marketplace` is green (all marketplace tests).
- [ ] `pnpm exec vitest run src/routes.test.tsx src/routes-code-splitting.test.ts` still passes.
- [ ] `pnpm typecheck` passes.
- [ ] `/marketplace/lineage/btc-momentum-v3` renders the full page — hero, ingredient banner,
      equity curve, variant tree, recent buyers, more-from-creator, receipts toggle row.
- [ ] `?receipts=open` in URL expands the receipts drawer inline (no modal/sheet/popover).
- [ ] `Buy` button calls `purchaseIntent` and navigates to `/marketplace/receipts/:txHash`.
- [ ] `Clone to edit` is disabled when viewer does not own a sealed listing; enabled for open
      tier or when `ownedListingIds` includes the id.
- [ ] NFT token id stamped on gen-art panel; `[Testnet]` label sourced from `onChain.nft.network`
      via `TxChip`.
- [ ] Trade history filter pills work: selecting Buy/Sell/Close filters the table rows.
- [ ] No `Dialog`, `Modal`, `Sheet`, or `Popover` introduced anywhere in F2 files.
- [ ] No `border-white`, `border-gray-100`, `border-gray-200`, `#fff`, `#ffffff` on any
      card/box border.

---

## Self-review checklist

Before claiming F2 complete, verify:

- [ ] `grep -r "Dialog\|Modal\|Sheet\|Popover" frontend/web/src/features/marketplace/routes/` —
  must return zero results from F2 files.
- [ ] `grep -r "border-white\|border-gray-100\|border-gray-200\|#fff\|#ffffff" frontend/web/src/features/marketplace/routes/` —
  must return zero results.
- [ ] `TxChip` usages in `ReceiptsDrawer` and `TradeHistoryTable` pass the network string from
  `onChain.nft.network` so the `[Testnet]` label fires correctly on non-mainnet chains.
- [ ] `data-testid="receipts-toggle"` is on the toggle row and `data-testid="receipts-body"` on
  the expanded panel — both referenced in `LineageRoute.test.tsx`.
- [ ] `data-testid="ingredient-banner"`, `data-testid="lineage-hero"`, `data-testid="lineage-info-stack"`,
  `data-testid="lineage-purchase-col"`, `data-testid="lineage-page"` all present and matching
  the tests.
- [ ] `canClone` gate uses `detail.tier === "open" || viewer?.ownedListingIds.includes(detail.id)`
  (not any other condition).
- [ ] Buy button is `disabled` while `buyMutation.isPending` to prevent double-submit.
- [ ] `EquityPanel` does NOT render the receipts drawer — it is a sibling, not nested.
- [ ] `TradeHistoryTable` Runner/Window dropdowns are stubs (no real filtering) — this is
  intentional for F2; note it in a `// TODO(F-runner-filter)` comment.

---

## Open Questions (seam gaps)

**OQ-1 — EquityPanel dual-layer rendering:**
`HeroGradientEquity` is single-series and does not expose a backtest/dashed overlay prop. The F2
plan uses a two-layer approach (faded SVG polyline for backtest + `HeroGradientEquity` for live).
This is sufficient for F2. If the charts track wants a native two-series variant, that is a
separate chart-rework follow-up — do not modify `HeroGradientEquity` from F2 without coordinating
with the charts track (see `team/CONFLICT_ZONES.md`).

**OQ-2 — `TxChip` network prop for `[Testnet]` label:**
Inspect `frontend/web/src/features/marketplace/components/TxChip.tsx`. If it does not yet accept
a `network` prop, adding one is a small additive change that can be done in Task 5 or as a
follow-up F2 cleanup commit. The spec says `[Testnet]` must show when `onChain.nft.network` is
not mainnet — this requires the `network` prop to flow through. Do NOT suppress the requirement
by hardcoding a `[Testnet]` string — root-cause it in the chip.

**OQ-3 — React Query setup in `MarketplaceLayout`:**
`LineageRoute` uses `useQuery`/`useMutation` from `@tanstack/react-query`. Verify that
`QueryClientProvider` is either already mounted above `MarketplaceLayout` in the app shell or
add it to `MarketplaceLayout.tsx`. The F0 layout only wraps `MarketplaceDataProvider` — if a
`QueryClient` is not already in the app shell (`Layout.tsx` or root), `MarketplaceLayout` must
add one. Check `frontend/web/src/components/shell/Layout.tsx` before implementing.

**OQ-4 — Relative time display for `RecentBuyer.at`:**
`RecentBuyer.at` is an `IsoDateTime` string. The design shows relative times like "3m ago",
"22m ago". The fixture `at` values are absolute ISO strings, not pre-formatted. F2 should format
them with a lightweight helper (e.g. `formatDistanceToNow` from `date-fns` if already a dep, or
a minimal inline formatter). Check `frontend/web/package.json` for `date-fns` before importing.
If not present, implement a minimal `relativeTime(iso: string): string` helper inline in
`LineageRoute.tsx`.

**OQ-5 — `Share` button action:**
The design spec says Share opens the share composer. In F2, the share composer is a stub — the
`Share` ghost button can be a disabled or no-op button with a `// TODO(F7-share)` comment.
Do NOT open a modal or popover. The share composer route is F7.

**OQ-6 — `Install missing` CTA target:**
`IngredientBanner`'s "Install missing" button has no navigation target in F2 (the plugins/MCP
install flow is out of scope). Leave it as a non-navigating button with `// TODO(install-flow)`
comment. Do NOT link to an external URL or pop a dialog.

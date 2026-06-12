# Marketplace UI Overhaul — "The Catalogue"

**Date:** 2026-06-12
**Status:** Authoritative design spec. Single source of truth for implementers.
**Worktree:** `/Users/edkennedy/Code/xvision/.worktrees/marketplace-overhaul`
**Surface root:** `frontend/web/src/features/marketplace/`

This spec merges three competing design proposals. The winning direction is
**THE CATALOGUE** (editorial gallery). The strongest concrete mechanisms from the
runner-up proposals are grafted in and called out inline:

- **From "Terminal Luxe":** the `cloneIntent` free-vs-paid receipt branching, the
  `xvnTradeMarkers` uPlot draw-hook approach, the "List your strategy →" single
  mint entry-point, the honest StatStrip pattern (absent cell, not zero).
- **From "Neo-Industrial Exchange":** the shared grid-column constant
  (de-duplicate header/row width definitions), the inline-accordion filter pattern
  with `SignalMenu` as the safe fallback, the segmented sort control, the "designed
  empty 404" treatment, the explicit `getSlices()` live-count override recipe.

---

## 0. HARD CONSTRAINTS (every implementer must obey)

1. **NO popups/modals/sheets/popovers on desktop.** Everything routes, docks,
   inline-expands, or uses accordions. Toasts allowed. `MListSheet` allowed ONLY
   for mobile list filters. The filter panel becomes an **inline accordion** (in
   normal document flow, pushes content down) — never an absolute overlay.
2. **NO right-side boxes / fourth column** on any route rendering `<Layout>` (the
   chat rail owns the right edge). Marketplace renders inside `<Layout>` via
   `MarketplaceLayout`. Detail/receipt pages are **single full-width columns**;
   auxiliary content goes inline, full-width, above/below the center content.
3. **Dark mode borders:** never `border-white`, `border-gray-100/200`, `#fff`,
   `#ffffff`. Use `border-border`, `border-[var(--ink-rule)]`,
   `border-muted-foreground/20`, or `dark:*-500/30`. Colored badges need `dark:`
   variants with low-opacity bg + lighter text.
4. **Terminology lock:** "Strategy" (the artifact), "Agent" (reusable template),
   `cycle_id`. Never `StrategyBundle` / `setup_id` in new code. (No backend schema
   work in this overhaul; this is a frontend + data-layer-mapping pass.)
5. **Honest-data discipline (the spec's spine):** NO fake numbers, NO seeded-RNG
   charts, NO fixture data leaking into the real client. Every unbacked field is a
   **designed empty state**, never a `0` or a fabricated value masquerading as
   real. When the active data client is the fixture client, a visible
   `DEMO CATALOGUE` marker must say so.

**Stack:** React 18, react-router-dom, Tailwind, Radix/shadcn, TanStack Query,
zustand, uplot + klinecharts for charts, viem for chain. Fonts: Geist Sans/Mono
via `@fontsource`; this spec adds Fraunces.

---

## 1. CONCEPT — THE CATALOGUE

The marketplace is reframed from a *trading-terminal listing wall* into a
**curated exhibition catalogue** — the way an auction house presents a collection.
Every strategy is a **catalogue entry** with a **plate number** (`№ 0043`), a
**plate** (the gen-art NFT, large and reverent), a **display-serif title**, and a
**caption block** where performance data reads like museum wall text: precise,
quiet, authoritative.

**The unforgettable device: the PLATE NUMBER.** Every strategy gets a permanent,
monospaced catalogue index (`№ 0043`) rendered in a hairline-ruled editorial frame.
It appears identically on the browse entry, the detail hero, and the receipt. It
makes 9 strategies feel like *a deliberately small, important collection* rather
than an empty database. **Scarcity becomes curation** — which directly solves the
existential problem (not enough NFTs minted yet).

Why it fits a trading-strategy marketplace:
1. **Trust through provenance.** Auction catalogues are the original trust machine
   — they certify authenticity, lineage, and value of objects you can't fully
   inspect before buying. That is exactly the buyer's problem here.
2. **Honest data, beautifully.** Wall-text captions are comfortable with absence.
   "Performance record · pending first live cycle" is dignified, not broken —
   unlike a green "+0%" with a fake sparkline.
3. **Differentiation.** No crypto marketplace applies auction-catalogue editorial
   typography to algorithmic finance. The collision of serif gravitas over Geist
   Mono numerals over generative art over on-chain trade markers is nameable,
   novel, and screenshot-legible.

---

## 2. DESIGN SYSTEM

### 2.1 Typography

Keep **Geist Mono** for all numerals/data and **Geist Sans** for prose. Add ONE
characterful display serif for titles, plate-number prefixes, section heads, and
the hero.

**Display face: `@fontsource/fraunces`** — a soft-contrast, high-optical-size
serif with literary/editorial character. Pairs beautifully with a geometric mono.
It is NOT Inter/Roboto/Arial/Space Grotesk.

Install (identical infra to how Geist was added):

```bash
cd frontend/web && npm i @fontsource/fraunces
```

`src/main.tsx` (add alongside the existing Geist imports, before the tokens.css
import):

```ts
import "@fontsource/fraunces/400.css";
import "@fontsource/fraunces/500.css";
import "@fontsource/fraunces/600.css";
import "@fontsource/fraunces/400-italic.css"; // editorial flourish (empty-state captions)
```

> NOTE: `@fontsource/fraunces` ships per-weight CSS files. Use the exact filenames
> above (`400.css`, `500.css`, `600.css`, `400-italic.css`). Do not use a
> `index.css` barrel (it pulls every weight and bloats the bundle).

`tailwind.config.ts` — add to `theme.extend.fontFamily` (keep `sans`/`mono`/`serif`
entries unchanged):

```ts
display: ["Fraunces", "Georgia", "serif"],
```

`src/styles/tokens.css` — add one variable inside `:root` (and it auto-inherits in
both data-theme blocks since they don't override it):

```css
--font-display: "Fraunces", Georgia, serif;
```

**Type scale (the editorial hierarchy):**

| Token name (informal) | Face | Size / leading / tracking | Use |
|---|---|---|---|
| display-hero | Fraunces 500 | 40px / 1.04 / -0.02em | Browse hero ("The Catalogue") |
| title-plate | Fraunces 600 | 26px / 1.1 / -0.015em | Detail hero strategy title |
| title-entry | Fraunces 500 | 17px / 1.15 / -0.01em | Browse entry title |
| eyebrow | Geist Mono 600 | 11px / 1 / 0.18em uppercase | Section labels, "PLATE", "PROVENANCE" |
| plate-no | Geist Mono 500 | 12px / 1 / 0.1em | `№ 0043` index (in `--gilt`) |
| caption | Geist Mono 400 | 11.5px / 1.4 | Wall-text data captions |
| metric-lg | Geist Mono 600 | 22px / 1 / -0.01em tabular | Headline return on detail |
| body | Geist Sans 400 | 13.5px / 1.55 | Description prose |

**Rule:** serif never sets data, mono never sets prose. The tension between the two
IS the aesthetic. All numerals get `font-variant-numeric: tabular-nums` (apply via
a `tabular-nums` utility or inline style).

### 2.2 Color tokens

Reuse the existing Signal token system entirely. The current page is a green wall
(`--gold` #00e676 on every return, button, and pill). The catalogue **rations
green to signal only** and introduces a quiet antique-gold editorial accent.

Add to `src/styles/tokens.css`. Put the shared dark values in BOTH `:root` and
`[data-theme="dark"]` (they are kept identical in this file by convention), and the
light values in `[data-theme="light"]`:

```css
/* :root  AND  [data-theme="dark"] — add these lines */
--paper:          #0d0f12;   /* catalogue page ground — a hair lighter than --bg */
--ink-rule:       #2a2f38;   /* hairline rules between entries */
--ink-rule-faint: #1b1f26;   /* faint inter-entry divider */
--gilt:           #c9a96a;   /* muted antique-gold: plate numbers, seals, hover underlines */
--gilt-bg:        rgba(201, 169, 106, 0.10);

/* [data-theme="light"] — add these lines */
--paper:          #faf8f3;
--ink-rule:       #d8d2c4;
--ink-rule-faint: #ece8de;
--gilt:           #9a7b3f;
--gilt-bg:        rgba(154, 123, 63, 0.10);
```

`tailwind.config.ts` — register the new color tokens (use the existing `cv()`
helper so opacity modifiers like `border-gilt/30` work):

```ts
paper: cv("--paper"),
"ink-rule": cv("--ink-rule"),
"ink-rule-faint": cv("--ink-rule-faint"),
gilt: cv("--gilt"),
"gilt-bg": cv("--gilt-bg"),
```

**Color discipline (the bold commitment):**
- **`--gilt`** is the catalogue's signature accent: plate numbers, the verified
  wax-seal, section-head hairlines, hover underlines. Quiet, expensive, editorial.
- **`--gold` (green)** is RATIONED to: (a) live *positive* return numbers, (b) the
  single primary buy/run CTA per surface, (c) the "live" provenance dot. Nowhere
  else. This inversion fixes the green-wall problem.
- **`--danger`** for negative returns and destructive confirmations only.
- Borders: always `border-border` / `border-[var(--ink-rule)]` /
  `border-[var(--ink-rule-faint)]`, never white. Colored badges keep the project
  rule (`dark:` low-opacity bg + lighter text, e.g. `bg-gilt-bg text-gilt
  border-gilt/30`).

> **`text-foreground` bug (cross-cutting):** `text-foreground` is used 28× in
> marketplace files but is undefined in this theme (no `foreground` token in
> tokens.css or tailwind.config). It renders unpredictably. **Find-and-replace all
> `text-foreground` → `text-text` across `src/features/marketplace/`.** This is a
> Wave-1 mechanical fix owned by the foundation work item.

### 2.3 Border / radius / spacing / texture

- **Radius: sharper than app default.** Catalogue chrome is rectilinear. Plates and
  entries use `rounded-[2px]`; grouping cards keep `rounded-card` (6px). CTA buttons
  `rounded-[3px]`. The gen-art plate itself is square (no radius) — it is a framed
  work.
- **Hairline rules, not boxes.** Browse entries are separated by a single 1px
  `border-b border-[var(--ink-rule-faint)]` rule — no per-row card borders, no
  shadows. Section heads get a full-width `border-[var(--ink-rule)]` rule with the
  eyebrow label sitting on it.
- **Generous vertical rhythm.** Entries are `py-5` (vs the cramped `py-3` baseline).
  Whitespace is the luxury signal.
- **The plate frame.** Each gen-art plate sits inside a literal picture frame:
  `p-[3px] border-2 border-[var(--ink-rule)] ring-1 ring-gilt/15`. On hover the
  inner gilt ring brightens to `ring-gilt/40`.
- **Texture.** Hero + detail surfaces get a subtle `GrainOverlay`
  (`@/components/chart/v2/primitives/GrainOverlay`) at very low opacity for paper
  texture. No drop shadows anywhere — editorial print has no shadows.

### 2.4 Motion vocabulary

CSS-first, one orchestrated load, a few surprising hovers. Reuse existing keyframes
(`xvn-row-in`, `xvn-card-in`, `xvn-num-pop`). Add TWO catalogue keyframes to
`src/styles/globals.css` at top level (outside `@layer`, near the other
`@keyframes`). The global `prefers-reduced-motion` catch-all (globals.css ~line
565) already collapses all of them — no extra work needed.

```css
@keyframes xvn-plate-develop {
  /* gen-art "develops" like a photographic plate on load */
  from { opacity: 0; filter: blur(6px) saturate(0.4); transform: scale(0.985); }
  to   { opacity: 1; filter: blur(0)   saturate(1);   transform: scale(1); }
}
@keyframes xvn-rule-draw {
  /* section hairlines draw in from the left like a pen stroke */
  from { transform: scaleX(0); }
  to   { transform: scaleX(1); }
}
```

| Moment | Animation | Duration / easing |
|---|---|---|
| Browse load | entries stagger in `xvn-row-in`, each `index*45ms`; plates run `xvn-plate-develop` | `--duration-base` (200ms) / `--ease-out` |
| Section rules | `xvn-rule-draw`, `transform-origin:left` | `--duration-slow` (320ms) |
| Number reveal | return metrics `xvn-num-pop` on first paint | 200ms |
| Entry hover | plate frame inner gilt ring brightens; title gains a `--gilt` underline that wipes L→R; row bg lifts to `surface-hover` | `--duration-fast` (120ms) |
| Buy confirm | CTA fills with `--gold`, label crossfades to "Confirmed" | 200ms |

Apply via `motion-safe:animate-[xvn-plate-develop_var(--duration-base)_var(--ease-out)_both]`
with per-entry inline `animationDelay`. Pure CSS, zero JS.

---

## 3. SURFACE-BY-SURFACE SPEC

> **Shared grid constant (graft from Neo-Industrial).** The browse list column
> widths are currently defined TWICE (in `ListHeader` inside `BrowseRoute.tsx:47`
> and in `ListingCard.tsx:28`). The new design replaces the dense grid with an
> editorial entry, but where any shared metric is needed, define it ONCE and import
> it. No duplicated `gridTemplateColumns` strings across two files.

### 3.1 BROWSE — `/marketplace` (route component `BrowseRoute.tsx`)

#### Layout
Single full-width center column. **Remove the `232px 1fr` grid and the
`LeaderboardRail`** from `BrowseRoute.tsx:156-163` (fixes QA6 and unsqueezes
QA7/9/14 simultaneously). The center column's `max-w-[960px]` (from
`MarketplaceLayout`/`Layout`) stays. Body stacks top→bottom:

```
HERO STRIP  →  TOOLBAR  →  APPLIED CHIPS (when active)  →  SLICE CHIP STRIP (gated)  →  CATALOGUE LIST
```

#### A. Hero Strip (replaces `HeaderStrip.tsx`)
- Eyebrow (Geist Mono, gilt): `XVISION · STRATEGY CATALOGUE · MANTLE TESTNET`.
- `display-hero` (Fraunces): **"The Catalogue"**.
- Body (Geist Sans, `text-text-2`): "Algorithmic trading strategies, minted as
  on-chain works. Inspect freely. Acquire selectively."
- **Honest stat ledger** (graft "absent cell" from Terminal Luxe). A single inline
  mono row. Only render a cell when its value is real for the active client; render
  an em-dash for fields with no real backing. Compute from `stats` and
  `listingsResult.total`. **Display only:** `ENTRIES {totalStrategies}` ·
  `CREATORS {n}` (count distinct `creator.address` from the loaded rows) · `PAID TO
  CREATORS {—|$x}`. **Remove `paidThisWeekUsd`, `agentPurchases`, `mintedLast24h`
  from display entirely** (all fixture-or-zero — QA1).
- **`DEMO CATALOGUE` marker:** when the active data client is the fixture client,
  show a quiet `bg-gilt-bg text-gilt border border-gilt/30 rounded-[2px]` chip
  reading `DEMO CATALOGUE`. (Detect via a `dataSource` field — see §6.)
- **TESTNET badge once here** (not per-row): a single `TestnetBadge` chip in the
  hero (fixes the per-row crowding feeding QA14).
- **Single mint CTA** (graft "List your strategy →" from Terminal Luxe): a primary
  button **`List your strategy →`** (`bg-gold text-[#001A0A]`) routing to
  `/marketplace/sell` (QA2). **No "Share" button** (QA3 — delete the plain Share
  button at `HeaderStrip.tsx:64-69`). Keep the `Wallet` link.

#### B. Toolbar (`Toolbar.tsx`)
A single horizontal row: `[ search ] [ Sort ▾ ] [ Filters ] [ segmented: Trending |
New | Mine ] [ view-toggle: Catalogue | Index ]`.

- **Sort (fixes QA5 / dead Sort button).** Wire the Sort button to the existing
  `SignalSelectMenu` from `@/components/primitives/SignalMenu` — it has
  click-outside + Escape dismiss built in and is the project-approved dropdown
  primitive (portal-positioned, NOT a focus-stealing modal). Pass `SORT_LABELS` as
  options. **When the active client is real (non-fixture) and `return30dPct`/
  `sharpe` are all 0, omit those two sort options** (they would sort on zeros) —
  show Newest / Price / Buyers only.
  - *Graft alternative (Neo-Industrial) if `SignalSelectMenu` proves awkward:* an
    inline segmented control of radio-style sort buttons. Either is acceptable;
    `SignalSelectMenu` is the default.
- **View toggle.** A segmented `Catalogue / Index` control. **Catalogue** = the
  editorial entry rows (default). **Index** = an opt-in dense mono table for power
  users (real fields only, hairline rules, NO sparkline). The Index view is
  optional and cuttable under time pressure — Catalogue is the thesis.

#### C. Filters (no-popup — fixes QA4)
**Delete the absolute `FilterDrawer` `<aside>`** and render `FilterDrawerContent`
as an **inline accordion** that expands in document flow *below the toolbar,
pushing the list down* (the `ChartFrame` / `ReceiptsDrawer` inline-expand pattern).
Because it is in-flow, there is no "stuck open" problem and no click-outside needed.
Still add an `Escape`-key `useEffect` that closes it and an explicit "Done" button.
Animate height via `grid-template-rows: 0fr → 1fr` or a max-height transition.

- **Match-count literal (QA1).** Add a `totalCount: number` prop to
  `FilterDrawerContent` and pass `listingsResult.total` from `BrowseRoute`. Replace
  the literal `1,247` at `FilterDrawerContent.tsx:343` (`of 1,247 match`) with
  `of {totalCount.toLocaleString()} match`.

#### D. Slice navigation / leaderboards (fixes QA6, QA18)
- **Delete `LeaderboardRail.tsx` from the browse body.** Replace with a
  **SLICE CHIP STRIP** — a horizontal row of gilt-outline chips below the toolbar
  (`Trending · Top return · Free-tier · Most owned`), each applying a
  `Slice.filter`. **Render the strip only when at least one slice has a real
  `count > 0`.** With < 5 real listings it does not render at all — the catalogue is
  complete without it. Reuse the existing `handleSliceClick`/`filter.slice` toggle
  logic from `BrowseRoute`.
- **Computed slice counts (fixes the hardcoded `SLICES` counts).** Override
  `getSlices()` in `ApiMarketplaceData` and `SubgraphMarketplaceData` so each
  slice's `count` is computed live: `count = applyFilter(realRows,
  {...defaultFilterState(), ...slice.filter}).length`. (Recipe grafted from
  Neo-Industrial.) The fixture `SLICES` static counts are never shown by the real
  clients.
- **CHAIN OPS strip — DELETE from browse entirely (QA18).** Remove
  `LeaderboardRail.tsx:62-71`. It is operator plumbing ("anchor · mint missing ·
  attesters"), not a buyer affordance. If an operator needs it, it lives in
  Settings → Identity, gated behind an operator check — never in the public
  catalogue.

#### E. Catalogue list — editorial entry rows (replaces the 8-col grid `ListingCard`)
Each listing is **one clickable `<Link to={/marketplace/lineage/${row.id}}>`** — the
WHOLE entry navigates to the inspector (fixes QA10; row-level nav was entirely
absent). Separated from the next entry by a single hairline rule.

Per-entry anatomy (`grid` with columns `120px 1fr auto`, `py-5`, `gap-6`,
`border-b border-[var(--ink-rule-faint)]`):

**Zone A — PLATE (120px fixed):**
- `GenArtPlaceholder seed={row.genArtSeed} size={104}` inside the editorial frame
  (`p-[3px] border-2 border-[var(--ink-rule)] ring-1 ring-gilt/15`, square, no
  radius). Reuse `GenArtPlaceholder` verbatim — it renders the EXACT on-chain art.
- Below the plate: `plate-no` → `№ {listingNo}` in `--gilt`. `listingNo` =
  zero-padded numeric listing id (`String(id).padStart(4, "0")`), or for fixtures a
  stable hash-derived 4-digit index of `row.id`.
- The plate runs `xvn-plate-develop` on load.

**Zone B — CAPTION BLOCK (1fr):**
- **Line 1 — title:** `title-entry` (Fraunces) rendering `row.name ?? humanize(row.id)`.
  - **Add `name?: string` to `ListingRow`** in `data/types.ts` (root cause of QA9:
    no name field). Populate from `meta?.name` (subgraph) /
    `IndexedListing.name` (api — already present!) / fixture label. The Api mapper's
    `toDetail` already has `l.name`; thread the same into `toRow`'s new `name`.
  - **`humanize(id)` helper:** slug → Title Case (`btc-momentum-v3` → "BTC Momentum
    v3"); bare numeric id → `Strategy #{id}`. Never render `wall-strat-0` raw —
    and the wall fixtures are removed from prod anyway (QA1).
  - `title` attribute = full name (hover tooltip). Wraps to 2 lines max, then
    ellipsizes — NO mid-word truncation (fixes QA9).
  - Inline after the title: `VerifiedBadge` (re-skinned as a gilt wax-seal, §5) when
    `verification === "verified"`; small `version` in mono `text-text-3`.
- **Line 2 — provenance caption:** mono `text-text-2`, `· `-separated: creator
  handle (or `0x…` short) · tier label (`Open edition` / `Sealed`) · `On-chain`
  (gilt dot if attested). Model/style **only if present** (real subgraph/api
  listings have none → segment omitted, not rendered blank). When `row.assets`
  is non-empty, render gilt-outline `AssetPill`s inline here; when empty, the assets
  segment is simply absent (captions are comfortable with absence — this is the
  system; fixes QA7's invisible-blank-cell by removing the column entirely).
- **Line 3 — performance caption (HONEST; replaces the fake sparkline column,
  fixes QA8):**
  - If real return + an equity micro-series exist: `30-DAY RETURN  +47.2%` (mono,
    `text-gold` if positive / `text-danger` if negative) followed by a real
    `MiniSparkline` (`@/components/chart/v2/primitives/MiniSparkline`) fed the
    equity points. (NOTE: `ListingRow` does not carry equity; for the catalogue,
    show the sparkline only for fixture/demo data where it's real, and for real
    on-chain rows fall through to the empty caption below.)
  - If `return30dPct === 0 && no equity series` (every real on-chain listing
    today): render the dignified empty caption `PERFORMANCE RECORD · pending first
    live cycle` in `text-text-3` italic Fraunces. **No number, no sparkline, no
    zero.**
  - **Never** render the seeded-RNG `Sparkline` component here. Remove its usage.

**Zone C — ACQUISITION (auto width, right-aligned, vertical):**
- **Price** as a caption: `PRICE` eyebrow over the value. Paid: `49 USDC` (mono, **no
  fee shown** — fixes QA15 at source: never embed fee in price). Open: a
  gilt-outline `OPEN EDITION` seal (NOT a loud green pill).
- **One primary action button (graft free-vs-paid from Terminal Luxe; fixes QA12).**
  The safest, catalogue-native behavior: **both buttons navigate to the detail
  page** (`/marketplace/lineage/${row.id}`) rather than firing a tx from the list.
  Inspect-before-buy is the catalogue ethos and removes the QA12 mislabel risk
  entirely.
  - Paid listing → button label `Acquire` → routes to detail.
  - Open listing → button label `Run free` → routes to detail.
  - The actual purchase/clone happens on the detail page where price/license/
    provenance are visible. **Do NOT call `purchaseIntent`/`cloneIntent` from the
    list row.** (The current `handleBuy` → `purchaseIntent` → receipt flow on the
    list is removed.)
- No per-row `TestnetBadge` (it lives once in the hero).

#### F. Empty / scarce / loading states
- **Few entries (`total < 5`):** this is the *feature*. Hero copy adapts to "A
  small, curated collection." The catalogue renders its 3–9 entries with full
  generous spacing. No "no results" sadness, no padding with fixtures.
- **`total === 0`:** the `EmptyState` primitive
  (`@/components/chart/v2/primitives/EmptyState`): title "The catalogue is empty",
  message "No strategies minted yet." with a `List your strategy →` link.
- **Loading:** entries render as plate skeletons — the editorial frame with a
  shimmering `surface-elev` fill where the gen-art develops, captions as hairline
  placeholder bars. Staggered `xvn-row-in`.

---

### 3.2 STRATEGY DETAIL / INSPECTOR — `/marketplace/lineage/:name` (`LineageRoute.tsx`)

#### Layout: single full-width column (fixes the `1fr 380px` right-box violation)
**Replace the below-fold `1fr 380px` grid (`LineageRoute.tsx:704`) with a single
full-width column (`space-y-6`).** `RecentBuyersList` + `MoreFromCreatorCard` become
a full-width inline `grid-cols-2` strip near the bottom — NOT a persistent 380px
right sidebar. No fourth column anywhere.

Section order:

```
PROVENANCE EYEBROW
HERO (grid: 360px 1fr — plate + title/price/acquire INLINE, no third column)
PERFORMANCE (full-width, FIRST-CLASS — see B)
PROVENANCE & TRADE LEDGER (full-width)
ABOUT / WHAT YOU GET / WON'T GET (existing sections, full-width)
VERSION LINEAGE (existing VariantMiniTree, full-width)
RECENT BUYERS + MORE FROM CREATOR (inline grid-cols-2, full-width)
RECEIPTS DRAWER (existing inline accordion — keep)
```

#### A. Hero
`grid-template-columns: 360px 1fr` (TWO zones, NOT the current three-column
`320px 1fr 250px`; the purchase block folds into zone 2's right edge to avoid a
third right-aligned box):
- **Zone A (360px):** the gen-art plate at `size={340}` in the full editorial frame
  (`p-[3px] border-2 border-[var(--ink-rule)] ring-1 ring-gilt/15` + `GrainOverlay`).
  Seed = `detail.genArtSeed` (same field/value as the row — see QA11 below).
  - **Clickable → inline-expand inspector accordion (fixes QA10).** Wrap the plate in
    a `<button>`. Clicking toggles an "Artifact & provenance" accordion *below the
    hero* (reuse the `ReceiptsDrawer` toggle pattern; deep-link via `?inspect=art`)
    showing the on-chain metadata table from `detail.onChain.nft`: `tokenId`,
    `lineageId`, `manifestHash`, `contract`, `bornAt`, and a **working Mantlescan
    link** (see QA16). No modal.
- **Zone B (1fr):** `title-plate` (Fraunces) title + version/verified/x402 badges,
  creator line, description prose. Below: a 5-cell metric strip (return / sharpe /
  win / maxDD / avg-dur) using `KpiCard`
  (`@/components/chart/v2/primitives/KpiCard`) — each shows its real value or a
  designed "—" when zeroed (real on-chain). Right edge of Zone B holds the
  **purchase block inline** (not a separate column):
  - Price as wall-caption `PRICE · 49 USDC` (display). Tier chip.
  - **Fee shown separately, low-emphasis (fixes QA15):** a distinct muted line
    `Platform fee 5% · creator receives {net} USDC`. The fee is NEVER parenthesized
    into the paid figure.
  - Primary CTA: `Connect wallet to acquire` / `Acquire` (paid → `purchaseIntent` →
    receipt) / `Run free` (open → see §3.3).
  - **Remove the disabled Share button entirely (fixes QA3):** delete
    `LineageRoute.tsx:650-657` (the `<button disabled>Share</button>`). Keep the
    "Clone to edit" button.

#### B. PERFORMANCE — first-class citizen (fixes QA17)
The centerpiece, directly below the hero, **full-width, min-height 360px** (vs the
current cramped 200px in `EquityPanel`). Built from the existing chart stack — NO
new chart infrastructure. **Replace `EquityPanel.tsx`** with a new
`PerformanceSection` (or rewrite `EquityPanel` in place; it is owned by one work
item). Wire `detail.onChain.trades` from `LineageRoute` into it (currently NOT
passed — the data already exists in the `ListingDetail` shape).

- Wrap in **`ChartFrame`** (`@/components/chart/v2/primitives/ChartFrame`) — gives
  range presets, inline Layers expand, Data-table expand (all no-popup).
- **Equity curve:** use **`HeroGradientEquity`**
  (`@/components/chart/v2/primitives/HeroGradientEquity`) fed `time: number[]` +
  `values: number[]` derived from `detail.equityCurve.points`. **Eliminate the
  broken raw-SVG backtest polyline** (`EquityPanel.tsx:89-111`) — that two-layer
  coordinate mismatch is deleted. If a backtest/live phase split is needed, render
  it as a single time-aligned series (null-fill the inactive segment) so uPlot
  aligns them on one axis.
  - *Alternative (Terminal Luxe):* `UplotEquityPane` takes `points: EquityPoint[]`
    (`{time, value}`) directly and renders the green/red zero-split fill — it is a
    cleaner fit if you want the % axis labels. Either primitive is acceptable;
    `HeroGradientEquity` is the hero default. **Do not hand-roll SVG.**
- **On-chain buy/sell markers — the headline (graft `xvnTradeMarkers` approach from
  Terminal Luxe).** Add a new uPlot draw-hook plugin `xvnTradeMarkers` to
  `@/components/chart/v2/adapters/uplot-plugins.ts`, modeled line-for-line on the
  existing `xvnLastDot` (same `u.valToPos(time, "x", true)` / `valToPos(value, "y")`
  pattern, same `allFinite` guard). It accepts `V2Marker[]` and draws a gold ▲ for
  `kind:"buy"` and a red ▼ for `kind:"sell"` at each marker's time/price. Guard: skip
  any marker whose `time` falls outside `u.scales.x.min/max`. Attach it to the
  equity pane's `plugins:` array.
  - Map `detail.onChain.trades[]` → `V2Marker[]`: `action:"buy"` → `kind:"buy"`;
    `action:"sell"|"close"` → `kind:"sell"`; `time = epoch(trade.at)`;
    `price = trade.entry ?? trade.exit ?? undefined`; `text = "${symbol} ${pnlPct ??
    ''}%"`.
- **`MarkerDock`** (`@/components/chart/v2/primitives/MarkerDock`) rendered
  **inline below the chart, full-width** (NOT a side rail) listing each trade
  marker. Clicking a dock entry highlights its marker via `activeId`.
- **Honest empty state (the real on-chain default).** When `equityCurve.points` is
  `[]` AND `onChain.trades` is `[]` (every real on-chain listing today), render the
  `EmptyState` primitive: title "No live performance record yet", message "This
  strategy hasn't completed a trading cycle on-chain." with a `Run a backtest →`
  link. **Never the fixture 90-point sine curve, never fake candles.** The empty
  state IS the shipping default for real listings; the chart lights up the moment
  the eval link lands.
- **Price candles (optional).** `KlineCandlePane`
  (`@/components/chart/v2/primitives/KlineCandlePane`, takes `candles:
  CandleColumns` + `markers?: V2Marker[]`) can render the strategy's primary asset
  with the same trade markers, synced to the equity pane via `PaneStack` +
  `useSyncKey`. **This is the first thing to cut under time pressure** — equity +
  markers + dock already satisfy QA17's "performance as a first-class citizen with
  on-chain markers." No candle data flows through the marketplace seam today, so
  this is gated on the same eval/chart endpoints and is explicitly optional.

#### C. Provenance / Mantlescan (fixes QA16)
- Single URL-building path through `<TxChip>`. **Fix `TxChip.tsx`** so
  `explorerTxUrl` uses the canonical explorer from `chain.ts`
  (`https://explorer.sepolia.mantle.xyz`, NOT `sepolia.mantlescan.xyz`) for
  mantle-sepolia/testnet networks. Route every explorer link (detail accordion,
  receipt) through `<TxChip>` so there is one builder.
- Keep the existing `VerifiedEvalsSection` (eval attestations) and `ReceiptsDrawer`
  (on-chain receipts accordion).

#### D. NFT image parity (fixes QA11)
- Browse entry uses `row.genArtSeed`; detail uses `detail.genArtSeed`. Both read the
  same field. For the Api client, ensure `toRow`/`toDetail` both set `genArtSeed`
  from the same source — and add the fallback `genArtSeed: l.gen_art_seed ||
  String(l.listing_id)` so an empty seed never renders blank art (graft from all
  three proposals). Remove `ApiMarketplaceData.getListing`'s fixture fallback for
  numeric IDs (currently the `catch` at line 230-233 returns fixture detail for an
  unknown numeric id, which can serve a wrong seed) — for numeric IDs that 404,
  surface the designed not-found state (§3.6) instead of silently serving fixture
  data. Add a dev-only assertion `row.genArtSeed === detail.genArtSeed` for any
  listing appearing in both collections.

---

### 3.3 FREE-VS-PAID RUN FLOWS (fixes QA12)

| Path | Trigger | Call | Destination | Header copy |
|---|---|---|---|---|
| **Acquire (paid)** | `Acquire` on the detail page | `purchaseIntent(id)` | `/marketplace/receipts/:tx` | "Acquired № 0043" |
| **Run free (open)** | `Run free` on the detail page | `cloneIntent(id)` (NOT `purchaseIntent`) | `/marketplace/receipts/:tx` (clone receipt) | "Activated № 0043 — added to your strategies" |

- The `ReceiptRoute` success header **branches on whether a price was paid**:
  `license.pricePaidUsdc > 0` → "Acquired {name}"; else → "Activated {name}".
  A free run NEVER shows "You bought" or a "LICENSE {tokenId} · {price} USDC paid"
  card claiming a purchase.
- The detail page's free CTA calls `cloneMutation` (already wired to `cloneIntent`
  in `LineageRoute`) — rename the button from "Clone to edit" intent where
  appropriate so the open-tier primary action reads "Run free".

---

### 3.4 SELL / MINT FUNNEL — `/marketplace/sell` (`SellRoute.tsx`)

- **Rename the heading (fixes QA2):** `SellRoute.tsx:85` `<h1>Share your strategy</h1>`
  → **`<h1>List your strategy</h1>`** (the sub-copy "List a strategy from your XVN to
  the marketplace" already aligns). Add a Fraunces page eyebrow `SUBMIT A WORK TO THE
  CATALOGUE`. The word "Share" never labels the mint flow.
- The single discoverable entry point to mint is the browse hero's
  `List your strategy →` button → `/marketplace/sell`. The nav/breadcrumb label
  becomes "List strategy".
- Keep the existing 3-step funnel (`Step1PickStrategy` / `Step2Configure` /
  `Step3Preview`). Step 2's live `ListingPreviewCard` should render as a
  catalogue-style entry preview (same plate-frame + caption layout as Browse) so the
  seller sees exactly the entry they are minting. (Light restyle; do not rewrite the
  step logic.)

#### ReceiptRoute layout (fixes QA13 + the 3-col right-box violation)
- **Refactor `ReceiptRoute.tsx:196-197` from `320px 1fr 380px` to a 2-column
  `320px 1fr`** (License plate left + Install steps right, **install primary**). No
  third right column (deletes the 380px ShareComposer column that abutted the chat
  rail).
- **Share is a collapsed inline accordion below install (fixes QA13).** Default
  state = a single ~56px strip: `[ Share this acquisition ]  [ Copy link ]`.
  Expanding ("Customize post") reveals the OG preview + caption + post targets
  inline. Collapsed by default; install is what the user sees first.
  - In `ShareComposer.tsx`: gate the OG preview + caption + variants behind the
    expand toggle. Remove the Discord link (no real web intent). The chain
    notification hint becomes a single-line chip.
- **License "paid" row (fixes QA15):** change `ReceiptRoute.tsx:52-55` from
  `${pricePaidUsdc} USDC (5% fee · ${feeUsdc} USDC)` to `${pricePaidUsdc} USDC` only.
  The fee is already implied by the `{netToCreatorUsdc} USDC → creator` line in the
  success header; if explicit, add a separate muted `Platform fee` row — never
  parenthesized into the paid amount.
- **Mantlescan (fixes QA16):** delete the hand-built href at
  `ReceiptRoute.tsx:181-191` and replace it with
  `<TxChip hash={receipt.txHash} network={network} label="View on explorer" />`.

---

### 3.5 DATA-LAYER HONESTY PASS (fixes QA1 and the data half of 7/8/11)

All in `src/features/marketplace/data/`:

- **`makeWallListings()` removed from the production pool.** In
  `fixtures/listings.ts`, gate the 200 wall rows behind `import.meta.env.DEV` (or a
  named flag) so `ALL_LISTINGS` in a production build is just `NAMED_LISTINGS`.
- **`getStats()` honesty.** `FixtureMarketplaceData.getStats()` may keep fixture
  numbers (it is the demo client and is marked `DEMO CATALOGUE`). The hero only
  displays `totalStrategies` + computed creators + (optional) paid-to-creators;
  `paidThisWeekUsd`/`agentPurchases`/`mintedLast24h` are not displayed by the new
  hero regardless. `ApiMarketplaceData.getStats` keeps `totalStrategies` real and
  the unbacked fields absent from display.
- **`getSlices()` live counts** in `ApiMarketplaceData` + `SubgraphMarketplaceData`
  (recipe in §3.1D).
- **`getViewer()` real wallet check** in `ApiMarketplaceData`: call
  `currentAddress()` from `lib/chain.ts`; return `{isConnected:false,
  createdListingIds:[], ownedListingIds:[]}` when null instead of delegating to the
  fixture `@ed` viewer. (Subgraph client may keep delegating — wallet signer is
  deferred — but it must not assert `@ed` is connected.) This stops the "Mine"
  segment and ownership CTAs from showing fixture state.
- **`subscribePurchases()`** in `ApiMarketplaceData`/`SubgraphMarketplaceData`:
  return a no-op cleanup instead of delegating to the fixture 5-second fake feed.
  No fake purchase events in production.
- **`name` on `ListingRow`** populated in the api `toRow` (`l.name`) and subgraph
  `mapListingRow` (`meta?.name`).
- **`genArtSeed` fallback** `l.gen_art_seed || String(l.listing_id)` (§3.2D).
- **`dataSource` exposure** for the `DEMO CATALOGUE` marker — see §6.

> **Assets (QA7) and return/equity (QA8) real data** depend on the manifest
> resolver (IPFS/Pinata, deploy-gated) and the `listing_id → eval_run_id` backend
> link, which are OUT OF SCOPE for this frontend overhaul. This spec deliberately
> routes both to designed honest empty states (absent assets segment; "pending
> first live cycle" caption). The UI ships truthful on day one and gets richer as
> those seams land — no fake data in the interim.

### 3.6 ERROR / NOT-FOUND STATES (designed, truthful)
- **Detail not found** (e.g. a numeric id that 404s, or a removed fixture `wall-0`):
  replace `LineageRoute.tsx:453-457`'s bare `"Strategy not found."` with a designed
  catalogue-miss state: an empty `№ ——` plate frame + Fraunces caption "This entry
  is not in the catalogue." + a `← Back to the catalogue` link.
- **Receipt not found:** keep the existing inline error but restyle to the catalogue
  voice.

---

## 4. QA TRACEABILITY (all 18 items)

| # | QA item | Resolution | Work item |
|---|---|---|---|
| 1 | Placeholder data throughout | Remove `makeWallListings` from prod pool; honest hero ledger (no `paidThisWeek`/`agentPurchases`/`minted24h`); `DEMO CATALOGUE` marker on fixture client; real `getViewer`; no-op `subscribePurchases`; computed slice counts; `1,247` literal → `totalCount` prop | W2-DATA + W2-BROWSE |
| 2 | "Share your strategy" doesn't link to Mint | `SellRoute` h1 → "List your strategy"; browse hero `List your strategy →` is the single mint entry → `/marketplace/sell` | W2-SELL + W2-BROWSE |
| 3 | Remove Share button | Delete plain Share button (`HeaderStrip`) and disabled Share button (`LineageRoute:650-657`) | W2-BROWSE + W2-DETAIL |
| 4 | Filter dropdown won't dismiss | Delete absolute `FilterDrawer` `<aside>`; render filters as inline accordion (in-flow) + Escape + Done button | W2-BROWSE |
| 5 | "Open"/Sort button label nonsense | Sort button → `SignalSelectMenu` (click-outside+Esc); hide return/sharpe sort options on real client; free CTA labeled "Run free" | W2-BROWSE |
| 6 | Leaderboards collapse/move | Delete 232px `LeaderboardRail`; slice nav → inline chip strip, gated to render only when a slice has real `count>0` | W2-BROWSE |
| 7 | Assets column blank | Remove assets-as-column; render `AssetPill`s inline on the provenance caption only when present; absent = omitted (caption comfortable with absence) | W2-BROWSE |
| 8 | Fake 30D sparkline | Delete seeded `Sparkline` usage; real `MiniSparkline` when equity present, else "PERFORMANCE RECORD · pending first live cycle" caption | W2-BROWSE |
| 9 | Strategy field truncated / column UX | Add `name` to `ListingRow`; Fraunces title (2-line wrap + `title` attr); editorial entry replaces cramped grid | W2-DATA + W2-BROWSE |
| 10 | Can't inspect without buying | Whole entry is a `<Link>` to detail; on detail, clicking the plate inline-expands the on-chain inspector accordion (`?inspect=art`) | W2-BROWSE + W2-DETAIL |
| 11 | Browse vs detail art mismatch | Single `genArtSeed` field on both surfaces; api mapper fallback `l.gen_art_seed \|\| String(l.listing_id)`; remove fixture fallback for numeric IDs; dev assertion | W2-DATA |
| 12 | "Run Free" claims you bought it | List CTAs route to detail (no tx from list); detail free CTA calls `cloneIntent`; receipt header "Activated"/"Acquired" branches on price paid | W2-BROWSE + W2-DETAIL + W2-RECEIPT |
| 13 | Share panel too big in inspector | `ReceiptRoute` → 2-col (license + install); `ShareComposer` collapsed to ~56px inline strip, expand-on-demand; Discord removed | W2-RECEIPT |
| 14 | Install/CTA column squeezed | Remove 232px rail + 8-col grid → full-width editorial entries; acquisition zone auto-width; `TESTNET` badge once in hero | W2-BROWSE |
| 15 | Fee on "Paid" | Price shows `49 USDC` only; fee on a separate muted line / `creator receives …`; receipt `paid` row de-feed | W2-DETAIL + W2-RECEIPT |
| 16 | Broken Mantlescan link | Fix `TxChip` → `explorer.sepolia.mantle.xyz`; route all explorer links through one `TxChip`; delete hand-built href | W1-FOUNDATION (TxChip) + W2-RECEIPT + W2-DETAIL |
| 17 | Inspector perf charts w/ on-chain markers | Full-width 360px `ChartFrame` + `HeroGradientEquity` (real equity, no SVG) + `xvnTradeMarkers` plugin from `onChain.trades` + `MarkerDock`; designed empty state | W1-FOUNDATION (plugin) + W2-DETAIL |
| 18 | Cryptic CHAIN OPS strip | Delete from browse entirely (`LeaderboardRail:62-71`) | W2-BROWSE |

---

## 5. SIGNATURE MOMENTS

1. **The developing plate.** On load, every gen-art plate runs `xvn-plate-develop`
   — it resolves from a blurred, desaturated ghost into crisp pixel-art over 200ms,
   like a photographic plate developing under the lights. Staggered across entries
   (`index*45ms`), the catalogue *materializes* on first paint. Pure CSS.
2. **The wax seal.** Re-skin `VerifiedBadge` as a small antique-gilt wax-seal glyph
   (filled `--gilt` circle with an embossed check, `bg-gilt-bg ring-1 ring-gilt/40`).
   On hover: a faint rotate + a `title` caption "Attested on-chain." Enormous trust
   work, unmistakably catalogue. CSS-only restyle.
3. **Title underline wipe.** On entry hover, the Fraunces title gets a `--gilt`
   underline that wipes in L→R (`bg-[linear-gradient(...)] bg-no-repeat
   bg-[length:0%_1px] bg-bottom hover:bg-[length:100%_1px]
   transition-[background-size]`), and the plate frame's inner gilt ring brightens.
   Reads like running a finger across a catalogue line.
4. **Marker reveal on the performance chart.** On-chain buy/sell triangles draw onto
   the equity curve (gold ▲ / red ▼) at exact timestamps, with the `MarkerDock`
   below letting you click through every actuation. This is the "proof of edge"
   moment — provenance you can SEE on the curve, which no screenshot can fake.
5. **Scarcity becomes a vitrine.** With 3–9 strategies, the catalogue presents its
   entries with generous spacing and adapted hero copy ("A small, curated
   collection"). Judges never see an empty table — they see a watch case. (Graft
   from Terminal Luxe's vitrine idea; realized as the catalogue's default rather than
   a separate card-grid mode.)

---

## 6. DATA / WIRING NOTES

- **`dataSource` exposure for the `DEMO CATALOGUE` marker.** The cleanest path: add
  an optional `readonly dataSource?: "fixture" | "api" | "subgraph"` getter or field
  to each `MarketplaceData` implementation (FixtureMarketplaceData → `"fixture"`,
  Api → `"api"`, Subgraph → `"subgraph"`), surfaced through the provider so
  `HeaderStrip` can branch. This is a small additive interface change (optional
  field, no breaking change to existing callers). Owned by W1-FOUNDATION.
- **Chart primitives work in the marketplace tree** — `useChart2Theme` depends only
  on the global `useTheme`, no special chart provider is required. Import primitives
  from `@/components/chart/v2/primitives` directly.
- **`HeroGradientEquity`** takes `time: number[]` + `values: number[]`.
  **`UplotEquityPane`** takes `points: EquityPoint[]` (`{time, value}`).
  **`MiniSparkline`** takes `time: number[]` + `values: number[]` + `color`.
  **`MarkerDock`** takes `markers: V2Marker[]` + `activeId?` + `onSelect?`.
  **`KlineCandlePane`** takes `candles: CandleColumns` + `markers?: V2Marker[]`.
- **`xvnTradeMarkers` plugin** lives in
  `@/components/chart/v2/adapters/uplot-plugins.ts`, returns
  `{ hooks: { draw } }`, modeled on `xvnLastDot`. Export it from the same module.

---

## 7. EXISTING TESTS THAT MUST BE UPDATED (co-located with their work item)

These existing tests assert behavior this overhaul changes; each must be updated in
the SAME work item that changes its component:

- `routes/browse/browse.test.tsx` — asserts CHAIN OPS callout, `Sparkline` render,
  `1,247`, `LeaderboardRail` slices, "Sort by". → **W2-BROWSE**
- `routes/BrowseRoute.test.tsx` — asserts "Sort by", `slice-sol-7d`, two
  complementary `<aside>` roles (FilterDrawer + LeaderboardRail). → **W2-BROWSE**
- `routes/BrowseRoute.buy.test.tsx` — asserts list-row buy → receipt flow (now
  removed; rows route to detail). → **W2-BROWSE**
- `components/FilterDrawer.test.tsx` — the absolute `<aside>` is deleted. → **W2-BROWSE**
- `components/Sparkline.test.tsx` — `Sparkline` usage removed from rows (component
  may remain but is no longer used on browse). → **W2-BROWSE** (leave the component;
  remove/adjust the browse-usage assertions only)
- `routes/EquityPanel.test.tsx` — `EquityPanel` rewritten to the full-width
  ChartFrame performance section. → **W2-DETAIL**
- `routes/LineageRoute.test.tsx` / `LineageRoute.buy.test.tsx` — hero layout, Share
  button removal, single-column below-fold. → **W2-DETAIL**
- `routes/ReceiptRoute.test.tsx` / `ReceiptRoute.real.test.tsx` — 2-col layout, fee
  row, Mantlescan via TxChip, share collapse, header copy branch. → **W2-RECEIPT**
- `routes/ShareComposer.test.tsx` — collapse behavior, Discord removed. → **W2-RECEIPT**
- `routes/SellRoute.test.tsx` — heading "List your strategy". → **W2-SELL**
- `data/ApiMarketplaceData.test.ts` / `SubgraphMarketplaceData.test.ts` /
  `subgraph/map.test.ts` / `FixtureMarketplaceData.test.ts` — `name` field,
  `genArtSeed` fallback, real `getViewer`, no-op `subscribePurchases`, live
  `getSlices`, `dataSource`. → **W2-DATA**
- `data/fixtures/fixtures.test.ts` — wall listings gated out of prod pool. → **W2-DATA**
- `components/TxChip` (no existing dedicated test of the URL; add coverage). →
  **W1-FOUNDATION**

---

## 8. FEASIBILITY & CUT ORDER

**Reused (near-zero new code):** `GenArtPlaceholder` (the plate), `ChartFrame`,
`HeroGradientEquity` / `UplotEquityPane`, `MarkerDock`, `MiniSparkline`, `KpiCard`,
`EmptyState`, `GrainOverlay`, `TradeHistoryTable`, `ReceiptsDrawer` (accordion
pattern), `SignalSelectMenu`, `TxChip`, the token system + motion infra (extended by
~5 CSS vars + 2 keyframes), the `@fontsource` pattern (one Fraunces install).

**New:** the editorial entry-row component (layout + Tailwind, ~120 lines); the
`xvnTradeMarkers` uPlot draw-hook (~40 lines, modeled on `xvnLastDot`); the inline
filter accordion refactor; several mechanical text/data-layer fixes.

**Riskiest piece:** the on-chain buy/sell markers (QA17 headline). The draw-hook
math is fiddly AND there is no real trade/equity data for on-chain listings today
(`onChain.trades` and `equityCurve.points` are empty until the `listing_id →
eval_run_id` backend link exists). **Mitigation:** ship the chart + marker plugin
fully working against fixture/eval data, and design the honest empty state as the
default real-data experience. The empty state is not a fallback — it is the dignified
default the catalogue concept is built to carry.

**Cut order under time pressure (in order):**
1. The `KlineCandlePane` price-candle layer in the inspector (keep equity + markers
   + dock — QA17 still satisfied).
2. The marker draw-on choreography / `xvn-rule-draw` section animation (static is
   fine).
3. The `Index` (dense table) view toggle (Catalogue view is the thesis).
4. The `xvn-plate-develop` stagger could degrade to a simple opacity fade if
   canvas-blur perf is a concern on low-end devices.

**Never cut (the spec's spine):** the plate-number/editorial-entry identity, the
honest stat ledger + empty states, the no-popup inline filter dismiss, the
`cloneIntent` free-run fix, the single-column detail layout with `ChartFrame` +
`HeroGradientEquity` + markers as the first-class performance citizen, the Fraunces
display voice, and the data-layer honesty pass.

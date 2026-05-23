# Chart rework — KlineCharts + uPlot (Tracks A + B)

Date: 2026-05-21 · **Amended 2026-05-23** to fold in the design package
under `docs/design/trading-charts/XVN.zip` (Claude design handoff,
2026-05-22). Amendment adds **Track B — Charts dashboard section**
(four new product-level chart canvases plus a left-nav entry); the
original library migration is preserved as **Track A** below.

Status:
- Track A (eval surface library swap): **M0 landed**. M1–M4 sequenced
  behind staff cookie.
- Track B (Charts dashboards from design handoff): **not started**.
  B0 foundation queued behind A-M1.

> **Predecessors:** [TradingView Lightweight Eval Surface](./2026-05-14-tradingview-lightweight-eval-surface-design.md) | [TradingView Charts](./2026-05-11-tradingview-charts-design.md).
> **Inputs (2026-05-23 amendment):** `docs/design/trading-charts/XVN.zip`
> → `design_handoff_charts/README.md` (handoff doc), `source/charts/*.jsx`
> (five chart-canvas references), `source/charts/chart-helpers.js`
> (uPlot plugins + KlineCharts theme), `source/charts/chart-theme.css`
> (design tokens), `source/charts/chart-data.js` (synthetic data + named
> strategy rotation).
>
> This spec **does not delete** v1 (lightweight-charts) — it parks the
> v2 implementation next to v1 and ramps Track A in over four
> milestones. v1 is deleted only at A-M4. Track B is greenfield; it
> targets fresh routes and does not touch existing eval surfaces.

## 1. Purpose

Two tracks are now in flight under one chart-runtime substrate.

### Track A — Eval surfaces leave `lightweight-charts`

The eval/scenario/strategy/live surfaces have outgrown a single-library
approach:

- Candle drawing, candle-anchored overlays (SMA/EMA/Bollinger/Donchian),
  and timestamped markers (buy/sell/veto/hold) belong to a **price-pane
  specialist** — KlineCharts ships that out of the box (candle types,
  overlay engine, indicator overlay system, marker overlay, hover
  crosshair).
- Equity / drawdown / oscillators (RSI, MACD, ATR), compare overlays, and
  small synced panes are **time-series line charts**. uPlot is purpose-built
  for that at hundred-thousand-point scale, with first-class cursor sync
  via `uPlot.sync()`.

The v1 charts (`frontend/web/src/components/chart/*Chart.tsx`) cram all of
that into `lightweight-charts`, which is good at candles but middling at
oscillator panes, and hostile to compare overlays and large equity
series. We rebuild on **two libraries doing what each is best at**.

### Track B — Charts dashboard section (from design handoff)

The 2026-05-22 design handoff (`docs/design/trading-charts/XVN.zip`)
introduces five chart-canvas concepts that are **new product surfaces**,
not re-skins of the eval surfaces. They share the Track A primitive
library (KlineCharts for candles, uPlot for line/area) but compose into
larger dashboard frames sitting at their own routes under a new top-level
`Charts` left-nav entry. Per user direction 2026-05-23:

| # | Name | Frame | Status in this spec |
|---|---|---|---|
| 01 | Dark Minimal Strategy Dashboard | Full chrome | Track B (B1) |
| 02 | Comparison AB Scalable | Full chrome, N strategies | Track B (B2) |
| 03 | AI Annotation Chart | Chart-only, embeddable | Track B (B3) |
| 04 | Liquidation Heatmap | Chart-only, embeddable | **Deferred** — followup, not in this spec wave |
| 05 | Gradient Warm Dashboard | Full chrome, hero variant | Track B (B4) |

## 2. Locked decisions

1. **Library split.**
   - KlineCharts owns: candles + candle-anchored overlays (SMA/EMA,
     Bollinger, Donchian) + candle-anchored markers (trades, vetoes,
     holds) + price-pane crosshair.
   - uPlot owns: equity, drawdown, histograms (volume), oscillator panes
     (RSI, MACD, ATR), compare overlays, line panes, wizard preview.
2. **Surface map (inherited).** The six Track-A chart surfaces stay 1:1
   with v1: Run, Compare, Scenario, Strategy, Live, Wizard preview.
   Their payloads are migrated to the columnar v2 format below.
3. **Primitives ↔ surfaces split.** A surface composes primitives; a
   primitive renders one pane (candles, equity, drawdown, oscillator,
   line, compare overlay) plus shared chrome (frame, layer panel, marker
   dock, legend, connection status, cache badge, empty state, data
   table). Surfaces never `createChart()` directly — they assemble
   primitives. Track B's dashboard frames also compose these primitives;
   they do not introduce a parallel runtime.
4. **Columnar payload.** v2 payloads ship as parallel `Float64Array`-ish
   columns (`time[], open[], high[], low[], close[], volume[]`) plus a
   typed indicator map. Adapters translate columnar → KLineData[] and
   columnar → uPlot AlignedData. The HTTP endpoint moves to
   `/api/v2/charts/...` so v1 stays usable during the ramp. Track B
   reuses the same payload shapes plus three new ones (multi-strategy
   equity bundle, annotation set, monthly-return matrix) defined in §6.
5. **`/chart-lab` is staff-only.** Mounted at `/chart-lab` and gated by
   the same staff predicate the eval-review surface uses. v1 routes are
   untouched until A-M4.
6. **(2026-05-23) New left-nav entry: `Charts`.** Track B mounts under
   `/charts` as a primary sidebar item. Subnav: `Overview` (default,
   = Chart 01 dashboard) · `Compare` (= Chart 02) · `Annotated`
   (= Chart 03) · `Hero` (= Chart 05). Chart 04 (Liquidation Heatmap)
   is **not** added in this wave; it is parked as F-CHART-LIQHEAT in
   `FOLLOWUPS.md` and will slot in as a future `/charts/heatmap`.
7. **(2026-05-23) Track B uses the design tokens as a superset of the
   existing `Chart2ThemeDefinition`.** The handoff's `chart-theme.css`
   tokens are mirrored into the dark theme verbatim (already-matching
   names get aliased; new categories — warm palette, heat ramp, named
   strategy rotation, typography stack — are added under their own
   groups). Light/black themes get derived equivalents so Track B
   surfaces don't crash in those themes; visual parity in non-dark
   themes is **best-effort, not pixel-faithful** for the gradient/hero
   variants (washes look different on light bg by design).
8. **(2026-05-23) The design's custom `<XvnCandleChart>` is reference
   only.** It exists in the handoff because KlineCharts didn't render
   in the prototype sandbox. Production composes `KlineCandlePane`
   (Track A primitive). Annotation positioning ports the handoff's
   `convertToPixel` + `onVisibleRangeChange` pattern into a new adapter
   (`adapters/kline-anchor.ts`).

## 3. Milestones

Two tracks. Track A keeps its original M0–M4 numbering (M-codes); Track
B introduces B0–B4 plus a B5 review checkpoint. The two tracks share
primitives and tokens but target disjoint routes and ship behind the
shared `xvn.chartv2=1` cookie. Track B must not block on A-M4 (v1
delete) — only on A-M1 (which moves `/api/v2/charts/*` onto the staff
cookie and proves the columnar endpoints).

| Track | Milestones | Status |
|---|---|---|
| **A — Eval surface library swap** | M0 → M1 → M2 → M3 → M4 | M0 landed |
| **B — Charts dashboard section** | B0 → B1 → B2 → B3 → B4 → B5 (review) → B-rollout | not started |

Followup (not in this wave): **F-CHART-LIQHEAT** — Chart 04 Liquidation
Heatmap. See §10.

---

### Track A — Eval surface library swap

#### A-M0 — Foundation (this PR)

- Branch: `chart-rework-klinecharts-uplot`.
- Adds `frontend/web/src/components/chart/v2/` with 17 primitives, 6
  surface compositions, 7 adapters, 5 hooks.
- Adds `Chart2ThemeDefinition` to all three themes (light, folio-dark,
  black) covering surface / candle / overlay / marker / position / pane /
  compare-palette / motion / density tokens — ~80 tokens per theme.
- Adds `scripts/gen-chart-v2-fixtures.ts` plus 5 generated fixtures
  (run, compare, scenario, strategy, live, wizard) under
  `frontend/web/src/components/chart/v2/__fixtures__/`.
- Adds `/chart-lab` route (staff-only) with four tabs: Overview ·
  Primitives · Surfaces · Tokens. Every primitive renders standalone
  against fixture data; every surface composition renders full-bleed
  at `/chart-lab/surfaces/{run|compare|scenario|strategy|live|wizard}`.
- **No production route changes.** `RunChart`, `CompareChart`,
  `ScenarioChart`, `StrategyChart`, `LiveChart`, `WizardPreviewChart`
  keep rendering v1 in their existing routes.
- Verification:
  - `npm run typecheck` clean.
  - Production `vite build` clean.
  - All existing v1 chart tests still pass.

#### A-M1 — Eval surfaces (Run + Compare)

- Cut `RunChartV2` and `CompareChartV2` over to production routes
  behind a staff cookie (`xvn.chartv2=1`). Default = v1.
- Add `/api/v2/charts/run/:id` and `/api/v2/charts/compare/:cmp` returning
  columnar payloads; v1 endpoints stay live.
- Snapshot tests on adapter outputs (columnar ↔ KLineData, columnar ↔
  uPlot) for regressions.
- **Track B unblocks here** — Track B uses the same columnar payloads
  + adapters, so B0 can start as soon as A-M1 lands.

#### A-M2 — Scenario + Strategy

- Cut `ScenarioChartV2` and `StrategyChartV2` over behind the same
  cookie. Move scenario detail's chart-rail into v2.
- Strategy chart switches its compare overlay to `UplotCompareOverlayPane`.

#### A-M3 — Live + Wizard preview

- `LiveChartV2` wires up the streaming hook (`useChart2Streaming`) and
  ConnectionStatus / CacheStatusBadge primitives.
- `WizardPreviewChartV2` replaces the inline preview SVG with a real
  candle pane.
- Cookie default flips to v2 once Live has been on staff cookie for a
  week with no regressions.

#### A-M4 — v1 deletion

- Delete `frontend/web/src/components/chart/*Chart.tsx` (v1).
- Remove `lightweight-charts` dependency.
- Delete v1 fixtures + v1 chart tests.
- Surfaces directly export `*ChartV2` under the v1 names.

---

### Track B — Charts dashboard section (2026-05-23 amendment)

Track B adds the `Charts` left-nav section described in the design
handoff. Each milestone targets one chart canvas and one route under
`/charts`. All four ship behind the same staff cookie used by Track A
(`xvn.chartv2=1`) until the section is feature-complete, then the
sidebar entry is shown for all users.

#### B0 — Section foundation + token superset

- Add `Charts` entry to `frontend/web/src/components/shell/Sidebar.tsx`
  `PRIMARY` array (icon: `chart-pie` or new `chart-grid`; route: `/charts`).
  Place **after `Scenarios`, before `Eval`** per §11.1 resolution.
- Add `/charts` route in `frontend/web/src/routes.tsx` with a nested
  layout (subnav) and a default index redirect to `/charts/overview`.
- Create `frontend/web/src/routes/charts/` directory with:
  - `ChartsLayout.tsx` — subnav row + `<Outlet/>`.
  - `ChartsOverview.tsx` — placeholder (B1 fills this in).
  - `ChartsCompare.tsx`, `ChartsAnnotated.tsx`, `ChartsHero.tsx` —
    placeholder shells.
- Extend `Chart2ThemeDefinition` per §4.5-amended:
  - Add warm palette group (`gold`, `amber`, `bronze`, `ember`,
    `copper`, `plum`, `teal`, `info`, `warn`, `danger`).
  - Add named strategy palette (8 entries — Fibonacci/EMA/Breakout/
    Momentum/Reversion/Vol/Liquidation/Benchmark, dashed flag on
    benchmark).
  - Add typography stack tokens (`fontSerif`, `fontSans`, `fontMono`)
    with the design's Cormorant Garamond / Inter / JetBrains Mono
    families. Wire Google Fonts in `index.html` or `<head>` injector
    (already loaded for v1 — verify and dedupe).
  - Add `.caps` utility class to global styles (Inter, 10–10.5px,
    `letter-spacing: 0.10em`, uppercase, `color: var(--text-3)`).
- Mirror the handoff `chart-theme.css` token names onto the dark theme
  verbatim. Light + folio-dark + black themes get derived equivalents.
- Add design assets under `docs/design/trading-charts/handoff/`
  (already present via XVN.zip) and reference them from Storybook /
  `/chart-lab/dashboards` (new sub-tab).
- Commit deterministic fixtures matching the new payload shapes (see
  §6) into `frontend/web/src/components/chart/v2/__fixtures__/`:
  - `multi-strategy-equity.json` — 5+ strategies × 240 points
  - `annotations.json` — 5 annotations matching the handoff sample
  - `monthly-returns.json` — 5 strategies × 17 months
- Verification:
  - `npm run typecheck` clean.
  - `npm run test -- charts/` covering token extensions + sidebar entry
    presence behind cookie.
  - `npm run build` clean.

#### B1 — Chart 01 Dark Minimal Strategy Dashboard → `/charts/overview`

- New surface `DarkMinimalDashboard` under
  `frontend/web/src/components/chart/v2/surfaces/`.
- New primitives:
  - `MultiStrategyEquityPane` — uPlot wrapper specialised for N
    overlaid equity series with `xvnLastDot`-style halo plugin
    (port from `chart-helpers.js`) on the lead series. Per-series
    stroke width: lead = 1.7px, others = 1.15px, benchmark dashed.
    Cursor sync key: `'hero'`.
  - `DrawdownCard` — composes `UplotDrawdownPane` + a 4-cell
    footer (Max DD / Avg DD / Duration / Recovery) using `.caps`
    eyebrows.
  - `MonthlyReturnsHeatmap` — pure DOM/SVG grid, `N strategies ×
    M months`. Cell color from `var(--gold)`/`var(--danger)` with
    α = `clamp(0.10, |value/0.15|, 0.65)`. Legend bar bottom.
    Tooltip on cell hover shows month + value (JetBrains Mono).
  - `KpiCard` — 5-up row primitive (Total Return / Sharpe / Max DD /
    Win Rate / Profit Factor). Value: Cormorant 30–32px; label:
    `.caps`; foot: JetBrains Mono 11px in `--text-3`.
  - `Topbar` — page eyebrow (`.caps`) + headline (Cormorant) +
    tagline (Cormorant italic). Right cluster: env pill (paper ·
    localhost), timeframe toggle row (`1D 1W 1M 3M YTD 1Y ALL`),
    `⤓ Export` ghost button.
- Surface composition: `ChartFrame` · `Topbar` · `KpiCard×5` ·
  `MultiStrategyEquityPane` · `DrawdownCard` ·
  `MonthlyReturnsHeatmap`.
- Data: read multi-strategy equity bundle (§6.1) from `/api/v2/charts/
  dashboards/overview` (new endpoint, M-side stub returning fixture
  initially; real builder pairs strategy IDs to their latest backtest
  run equity series).
- Verification:
  - Render against fixture; visual diff vs the design handoff PNG.
  - Cursor sync works between equity pane and drawdown card.
  - Heatmap cell α scales correctly across the ±15% range.

#### B2 — Chart 02 Comparison AB Scalable → `/charts/compare`

- New surface `ComparisonABDashboard`.
- New primitives:
  - `StrategyRosterPills` — horizontal pill rail of available
    strategies. On-state: `${color}14` bg + `${color}55` border.
    Off-state: muted. `×` removes a strategy; click toggles
    inclusion. Min 2 selected (× disabled at 2). Stable order =
    strategy color rotation order.
  - `StrategyCardGrid` — CSS-grid wrapper with column count
    derived from N selected: `n≤2→2`, `n≤4→4`, `n≤6→3`, `n≥7→4`.
    Reflows via `grid-template-columns` change.
  - `StrategyCard` — head (color dot + Cormorant name + `.caps`
    kind line + `LEAD` badge if first + `×` if removable) →
    `MiniSparkline` (uPlot, area-filled in strategy color, no
    axes, no cursor) → 2×2 metrics (Return / Sharpe / MaxDD /
    Win) → indicator chip strip (`RSI 56`, `MACD ↑`, `EMA`,
    `Fib · 0.618`).
  - `MiniSparkline` — micro `UplotLinePane` variant with no axes,
    no legend, no cursor, deterministic area-fill plugin
    (`xvnAreaFill` port). Toggle between price line and drawdown
    line via prop.
  - `LeadCardChrome` — gradient backdrop
    (`linear-gradient(180deg, rgba(212,165,71,0.04), var(--surface-card) 38%)`)
    + gold-tinted border + 1px gold gradient line across the top.
    Applied to whichever card is `selectedIds[0]`.
- Surface composition: `ChartFrame` · `Topbar` (headline =
  "`N` strategies, one frame") · `MultiStrategyEquityPane` (hero
  overlay; legend chips include return as JetBrains Mono micro-stat)
  · `StrategyRosterPills` · `StrategyCardGrid` of `StrategyCard`s.
- State: `selectedIds: string[]` controlled by the route (URL
  param: `?ids=fib,ema,brk,...`). Default = first 6 from the named
  strategy rotation.
- Verification:
  - Add/remove a strategy → grid reflows column count without remount
    flicker.
  - Grid widths sum within ±2px at all breakpoints.
  - LeadCardChrome only ever wraps `selectedIds[0]`.

#### B3 — Chart 03 AI Annotation Chart → `/charts/annotated`

- New surface `AIAnnotationDashboard` (chart-only frame; the chrome
  here is lighter — header + chart + insight log).
- New primitives:
  - `AnnotationOverlay` — absolutely-positioned layer above
    `KlineCandlePane`. SVG `<svg pointer-events:none>` connectors
    (dashed `3,3`) from candle anchor (`r=6` ring 55% opacity +
    `r=2.4` solid dot) to nearest callout corner. Two rows of
    callout cards: top row at `y=24`, bottom row at `y=H-180`.
    Callouts spread evenly across `(width - 12 - 80)` to avoid
    horizontal overlap; connector lines still point to real candle
    x via the anchor bridge.
  - `Callout` — ~210px wide card. Border
    `rgba(212,165,71,0.32)` (gold) or
    `rgba(200,68,58,0.32)` (red, when `danger: true`). Inner:
    `.callout-head` eyebrow (`type · conf 74%`), Cormorant title
    14px, body 11.5px in `--text-2`, foot row showing
    `idx · 22` + action verb in accent color.
  - `InsightLog` — collapsible right rail (280px ↔ 36px).
    List of cards: title row (Cormorant 14px / Mono timestamp),
    body (11.5px), footer with category pill + confidence. Left
    edge has 2px accent bar (gold / red). Collapse button `›`
    shrinks the panel to a 36px rail showing vertical
    "Insight Log · N events" text + 6 colored dots.
  - `AiEnginePill` — animated gold-dot pill with the
    `aiPulse` keyframe (1.8s ease-out:
    `scale(1, opacity:0.7) → scale(3.4, opacity:0)`).
- New adapter: `adapters/kline-anchor.ts` — wraps a KlineCharts
  instance to expose `xForIndex(idx)`, `yForPrice(p)`, and
  `subscribeLayout(cb)` that fires on `onVisibleRangeChange` +
  ResizeObserver. Annotation overlay subscribes and re-anchors;
  matches the handoff's `recompute` pattern.
- Surface composition: `ChartFrame` · header (xvn lozenge + symbol
  pill + price + 24h change + `AiEnginePill` + filter toggle row) ·
  `<grid 1fr 280px>` of `(KlineCandlePane + AnnotationOverlay)`
  and `InsightLog` · status footer (JetBrains Mono 10.5px).
- Data: annotation payload (§6.2). `idx` references the bar's index
  in the candle column array; backend producer must keep these
  aligned. Per §11.2 resolution, B3 supports **both** sources via a
  `?source=` query param:
  - `?source=run&run_id=...` → `/api/v2/charts/annotated/:run_id`
    (reads stored annotations from the run that emitted them).
  - `?source=live&symbol=...` → `/api/v2/charts/annotated/live/:symbol`
    (on-demand annotation generation; the producer itself is **out
    of scope** per §9 — when no producer is wired the response is
    empty and the surface renders an `EmptyState` reading
    "annotation producer not configured").
  - Default: `?source=run` when neither is set.
- Verification:
  - Pan / zoom the candle chart → callouts track candles, connectors
    stay anchored to the right candle index, clip cleanly at chart
    edges.
  - Insight log collapse animates 200ms ease without layout shift in
    the candle pane.
  - Filter toggle (`Patterns | Risk | Flow | All`) hides callouts +
    log entries by `type`.

#### B4 — Chart 05 Gradient Warm Dashboard → `/charts/hero`

- New surface `GradientHeroDashboard`. Same `1440×900` shell as B1
  but in "hero" garb. Per §11.3 resolution, B4 mounts **only at
  `/charts/hero`**; `/` is unchanged. After B4 lands and all four
  canvases are running in production, a separate **B5 review
  milestone** decides whether `/` should redirect / alias to the
  hero variant (or surface a minimal/hero toggle). No code
  commitment to that decision in this wave.
- New primitives:
  - `AuraBackground` — three absolutely-positioned radial-blur
    washes behind the content layer. Defaults match the handoff:
    - 520×520 gold/ember wash, top-left of main area.
    - 680×680 ember/plum wash, bottom-right (-260, -100).
    - 380×380 amber wash, top-right.
    `opacity: 0.30–0.55`, `filter: blur(20px)`.
    Light theme: muted desaturated counterparts (Open Question §11.3).
  - `GrainOverlay` —
    `repeating-linear-gradient(0deg, rgba(241,236,221,0.012) 0 1px, transparent 1px 3px)`
    at `opacity: 0.5`, full-bleed, behind content.
  - `GlassCard` — `.glass` utility component:
    `linear-gradient(180deg, rgba(34,30,20,0.62), rgba(20,18,14,0.78))`,
    `border: 1px solid rgba(241,236,221,0.07)`,
    `backdrop-filter: blur(8px)`, subtle inset highlight
    `0 1px 0 rgba(255,255,255,0.02)`.
  - `HeroGradientEquity` — variant of `UplotEquityPane` with a
    multi-stop warm gradient area-fill plugin (5 stops, matching
    handoff §05) + horizontal sheen plugin (top 40%) +
    `xvnLastDot` halo on lead series. Implemented as a uPlot
    `hooks.draw` plugin in `adapters/uplot-plugins.ts`.
  - `PerformanceRadar` — pure SVG, 260×220, 6 axes (Return /
    Sharpe / Stability / Win Rate / Consistency / Drawdown). Top
    3 strategies as overlaid polygons (`fillOpacity 0.10`,
    stroke 1.4px in strategy color, vertex dots `r=2.2`). 4
    ring polygons at 25/50/75/100%. Axis labels at 1.18× radius,
    tracked uppercase.
  - `MarketContextCard` — 2×2 BTC stats grid (Price / Funding /
    Open Interest / Liq 24h) + regime chip row
    (`BULL · 62%`, `SIDEWAYS · 22%`, `BEAR · 9%`,
    `HIGH VOL · 7%`).
  - `GradientHeadline` — Cormorant + JetBrains Mono headline with
    a linear-gradient text fill on the bracketed phrase
    (`90deg, #E5B86A → #D4A547 → #C16A3A`).
- Surface composition: `AuraBackground` + `GrainOverlay` (chrome) ·
  `Topbar` (`GradientHeadline`) · `KpiCard×5` (Total Return card
  gets the radial gold corner glow) · `HeroGradientEquity` ·
  `PerformanceRadar` + `DrawdownCard` (drawdown variant of B1's
  card; lead series area-filled in gold-tinted red) ·
  `MarketContextCard`.
- Verification:
  - Aura washes do not increase paint count past one extra
    composite layer (use `will-change: transform` only if Lighthouse
    flags repaint cost).
  - HeroGradientEquity area-fill renders identically across themes
    in dark mode (one fixture, three browsers).
  - PerformanceRadar polygon points round-trip via known fixture
    values (snapshot test on computed SVG `d` strings).

#### B5 — Hero-default decision (review, no code commitment)

Per §11.3 resolution, B5 is **not an implementation milestone** —
it's a review checkpoint that fires after B4 lands. With all four
canvases visible in production behind the staff cookie, the team
picks one of:

- (a) leave `/` as today's Dashboard, hero stays at `/charts/hero`,
- (b) redirect `/` to `/charts/hero`,
- (c) introduce a `?variant=minimal|hero` param on `/` that picks
  between `DarkMinimalDashboard` and `GradientHeroDashboard`.

Outcome of B5 either closes this thread (decision: stay at (a)) or
spawns a tiny follow-up plan to implement (b)/(c).

#### B-rollout — flip the cookie

After B0–B4 land (B5 review may run in parallel) and an internal
review pass on each canvas:

- Default the sidebar `Charts` entry on for all users.
- Per §11.4 resolution, the single `xvn.chartv2=1` cookie is dropped
  for the Charts section in this step (Track A keeps its own
  decision about when to drop). No per-canvas flag cleanup needed.

## 4. Foundation code (A-M0)

### 4.1 Primitives (17) — `frontend/web/src/components/chart/v2/`

| # | Primitive | Library | Purpose |
|---|-----------|---------|---------|
| 1 | `KlineCandlePane` | KlineCharts | Candle pane + candle-anchored indicator overlays (SMA/EMA/Boll/Donchian) + candle-anchored markers |
| 2 | `UplotEquityPane` | uPlot | Equity curve, baseline @ starting equity, gain/loss fills |
| 3 | `UplotDrawdownPane` | uPlot | Drawdown area pane (negative-only, red) |
| 4 | `UplotHistogramPane` | uPlot | Volume histograms, MACD histograms |
| 5 | `UplotOscillatorPane` | uPlot | RSI / MACD lines / ATR — single primitive parametrised by series spec + guide lines |
| 6 | `UplotLinePane` | uPlot | Generic line pane (one or many series, no candle backdrop) |
| 7 | `UplotCompareOverlayPane` | uPlot | Multiple normalized series on one axis for compare runs |
| 8 | `ChartFrame` | — | Title row + range selector + layers button + data-table toggle |
| 9 | `LayerPanel` | — | Layer toggles (candles / overlays / markers / panes / volume) |
| 10 | `MarkerDock` | — | Right-rail dock listing recent markers (replaces v1 `MarkerSidePanel`) |
| 11 | `Legend` | — | Per-pane legend chip row |
| 12 | `ConnectionStatus` | — | Live streaming status pill (connected / reconnecting / offline) |
| 13 | `CacheStatusBadge` | — | "served from cache" / "fresh" badge |
| 14 | `EmptyState` | — | "no bars yet" placeholder |
| 15 | `DataTable` | — | Tabular fallback under the chart |
| 16 | `PaneStack` | — | Vertical stack of panes with shared time axis + sync handle |
| 17 | `SyncCursor` | — | Crosshair-sync coordinator (uses `uPlot.sync()` for the uplot side; thin wrapper around KlineCharts' subscribe API for the candle pane) |

### 4.2 Surfaces (6)

| Surface | Composes |
|---|---|
| `RunChartV2` | ChartFrame · KlineCandlePane · UplotOscillatorPane · UplotEquityPane · UplotDrawdownPane · UplotHistogramPane · LayerPanel · MarkerDock · Legend · DataTable |
| `CompareChartV2` | ChartFrame · UplotCompareOverlayPane · UplotDrawdownPane · LayerPanel · Legend · DataTable |
| `ScenarioChartV2` | ChartFrame · KlineCandlePane · UplotEquityPane · UplotHistogramPane · LayerPanel · MarkerDock · Legend |
| `StrategyChartV2` | ChartFrame · KlineCandlePane · UplotCompareOverlayPane · UplotDrawdownPane · LayerPanel · Legend |
| `LiveChartV2` | ChartFrame · KlineCandlePane · UplotEquityPane · MarkerDock · ConnectionStatus · CacheStatusBadge · EmptyState |
| `WizardPreviewChartV2` | ChartFrame · KlineCandlePane · UplotEquityPane · Legend |

### 4.3 Adapters (7) — `frontend/web/src/components/chart/v2/adapters/`

1. `columnar-to-klinedata.ts` — columnar OHLCV → `KLineData[]` for KlineCharts.
2. `columnar-to-uplot.ts` — columnar series map → `uPlot.AlignedData`.
3. `markers.ts` — v1/v2 marker payloads → KlineCharts overlay markers + MarkerDock entries.
4. `theme-to-klinecharts.ts` — `Chart2ThemeDefinition` → KlineCharts styles object.
5. `theme-to-uplot.ts` — `Chart2ThemeDefinition` → uPlot options (axis, grid, series stroke).
6. `sync-bridge.ts` — coordinator that joins a KlineCharts crosshair to a `uPlot.sync()` key.
7. `streaming.ts` — stub that buffers WS bar appends and flushes them to `KlineCandlePane`. Real wire-up in M3.

### 4.4 Hooks (5) — `frontend/web/src/components/chart/v2/hooks/`

1. `useChart2Theme` — returns `Chart2ThemeDefinition` for the resolved theme.
2. `useChart2Layers` — like v1's `useChartLayers` but typed against v2 layer keys.
3. `useChart2Sync` — produces a stable sync key for `PaneStack` children.
4. `useChart2Fixture` — loads one of the five JSON fixtures (lab + tests).
5. `useChart2Streaming` — streaming stub (returns frozen state in M0; wired in M3).

### 4.5 Theme tokens

`Chart2ThemeDefinition` adds ~80 tokens per theme grouped as:

```
surface { bg, panelBg, gridStrong, gridSoft, axisText, axisTick, crosshair }
candle  { up, down, wickUp, wickDown, borderUp, borderDown }
overlay { sma20..sma200, ema20..ema200, bollUpper, bollMiddle, bollLower,
          donchianUpper, donchianLower }
marker  { buy, sell, veto, hold, halo, textOnAccent }
position { longBand, shortBand, longLine, shortLine }
panes   { equity, equityFillTop, equityFillBottom, drawdown,
          drawdownFillTop, drawdownFillBottom, volumeUp, volumeDown,
          rsi, rsiGuide, macdLine, macdSignal, macdHist, atr }
compare { palette0..palette7 }   // 8-color overlay palette
motion  { hoverMs, animMs }
density { axisFont, axisGap, paneGap }
```

Reused colours alias the existing `chart.series.*` so themes stay
visually consistent across v1 and v2.

### 4.6 Mock fixtures

`scripts/gen-chart-v2-fixtures.ts` is a deterministic generator (seeded
PRNG). It writes five JSON files into
`frontend/web/src/components/chart/v2/__fixtures__/`:

- `run.json` — 240 hourly bars + every indicator series + markers + equity + drawdown.
- `compare.json` — 4 arms × 240 normalized equity points, with drawdown per arm.
- `scenario.json` — 96 bars + position bands + sparse markers.
- `strategy.json` — 480 bars + compare overlay (live vs paper).
- `live.json` — 60 bars with a "live tail" cursor (`live_index`).
- `wizard.json` — 30 bars + 30 equity points.

Each file is committed; the script is rerunnable and idempotent
(`npm run gen:chart-v2-fixtures`).

### 4.7 `/chart-lab` route

| Tab | Content |
|---|---|
| Overview | Library split rationale, surface map, lib version pins, links to other chart specs. |
| Primitives | Each primitive rendered standalone in a card, against the relevant fixture. |
| Surfaces | Links to `/chart-lab/surfaces/{run\|compare\|scenario\|strategy\|live\|wizard}`; each renders the surface composition full-bleed against its fixture. |
| Tokens | A palette wall — every `Chart2ThemeDefinition` token across the three themes, side-by-side. |

The route is gated by the same staff predicate as eval-review.

## 4-amended. Track B additions (planned, 2026-05-23)

The additions below extend the Track A foundation. They do **not**
replace any existing primitive; they share `Chart2ThemeDefinition`,
`ChartFrame`, `KlineCandlePane`, the columnar payload, and the
adapters.

### 4A.1 New primitives (10) — `frontend/web/src/components/chart/v2/primitives/`

| # | Primitive | Substrate | Used by | Notes |
|---|-----------|-----------|---------|-------|
| 18 | `MultiStrategyEquityPane` | uPlot | B1, B2, B4 | N overlaid equity series, lead halo via `xvnLastDot` port |
| 19 | `DrawdownCard` | composes `UplotDrawdownPane` | B1, B4 | 4-cell footer (Max DD / Avg DD / Duration / Recovery) |
| 20 | `MonthlyReturnsHeatmap` | SVG/DOM grid | B1 | α-scaled cells, ±15% legend bar |
| 21 | `KpiCard` | DOM | B1, B4 | 5-up row, Cormorant value + `.caps` label |
| 22 | `Topbar` | DOM | B1, B2, B4 | Eyebrow + headline + tagline + right action cluster |
| 23 | `StrategyRosterPills` | DOM | B2 | Min-2 selection, on-state tinted by strategy color |
| 24 | `StrategyCardGrid` + `StrategyCard` + `MiniSparkline` | DOM + uPlot | B2 | Auto-wrap card grid, mini sparkline area-fill |
| 25 | `AnnotationOverlay` + `Callout` | SVG + DOM | B3 | Anchored to candle index via `kline-anchor` adapter |
| 26 | `InsightLog` | DOM | B3 | Collapsible right rail, accent-bar entries |
| 27 | `AiEnginePill` | DOM | B3 | Pulsing gold dot, `aiPulse` keyframe |
| 28 | `AuraBackground` + `GrainOverlay` + `GlassCard` | CSS | B4 | Hero chrome trio |
| 29 | `HeroGradientEquity` | uPlot | B4 | Multi-stop gradient + sheen plugin variant of equity pane |
| 30 | `PerformanceRadar` | SVG | B4 | 6-axis radar, top-3 polygons |
| 31 | `MarketContextCard` | DOM | B4 | BTC stats grid + regime chip row |
| 32 | `GradientHeadline` | DOM | B4 | Text-fill linear gradient on bracketed phrase |

(Numbers continue past 17 to keep the foundation table stable.)

### 4A.2 New surfaces (4) — `frontend/web/src/components/chart/v2/surfaces/`

| Surface | Route | Composes |
|---|---|---|
| `DarkMinimalDashboard` | `/charts/overview` | ChartFrame · Topbar · KpiCard×5 · MultiStrategyEquityPane · DrawdownCard · MonthlyReturnsHeatmap |
| `ComparisonABDashboard` | `/charts/compare` | ChartFrame · Topbar · MultiStrategyEquityPane · StrategyRosterPills · StrategyCardGrid (of StrategyCard) |
| `AIAnnotationDashboard` | `/charts/annotated` | ChartFrame · header (AiEnginePill + filters) · KlineCandlePane + AnnotationOverlay · InsightLog |
| `GradientHeroDashboard` | `/charts/hero` | AuraBackground · GrainOverlay · Topbar (GradientHeadline) · KpiCard×5 · HeroGradientEquity · PerformanceRadar · DrawdownCard · MarketContextCard |

### 4A.3 New adapters (2) — `frontend/web/src/components/chart/v2/adapters/`

1. `kline-anchor.ts` — wraps a KlineCharts instance. Exports
   `xForIndex(idx) → px`, `yForPrice(p) → px`, and
   `subscribeLayout(cb)` that fans out `onVisibleRangeChange` +
   ResizeObserver into a single layout-changed callback. Drives
   `AnnotationOverlay` re-anchoring (B3).
2. `uplot-plugins.ts` — TypeScript port of the handoff's
   `chart-helpers.js` plugins: `xvnLastDot`, `xvnAreaFill`,
   `xvnRegimeBands`, `xvnGradientFill` (5-stop variant for
   `HeroGradientEquity`), `xvnSheen` (top-band highlight overlay).
   All take `(seriesIdx, options)` and return a uPlot plugin.

### 4A.4 New hook (1)

- `useChart2Roster` — owns `selectedIds: string[]` state for
  Comparison surfaces; reads/writes the URL `?ids=...` param; exposes
  `add`, `remove`, `toggle`, `setLead`. Min-2 invariant enforced here.

### 4A.5 Theme tokens (Track B additions)

`Chart2ThemeDefinition` gains the following groups. Token names
mirror the handoff's `chart-theme.css` so the design is portable
back-and-forth.

```
warm {
  gold        // #D4A547  primary accent / lead strategy / EMA line
  amber       // #E5B86A  gradient top stop, 2nd-tier accent
  bronze      // #A87A3C  quiet warm
  ember       // #C16A3A  gradient mid-stop
  copper      // #8C4A2E  deep warm
  plum        // #8E6789  cool counterpoint #1
  teal        // #5E8A8C  cool counterpoint #2
  info        // #6F8FB8
  warn        // #DB9230
  danger      // #C8443A  (also `candle.down`)
}

strategyRotation [               // 8 entries, ordered, for multi-strategy charts
  { id, name, short, color, kind, dashed? }
  ...
]

heatRamp {                        // Bookmap-style, RED — not gold
  scorching   // #FF6B5C @ α 0.48   (>0.75)
  hot         // #E04A3A @ α 0.42   (0.55–0.75)
  warm        // #A93428 @ α 0.36   (0.35–0.55)
  cool        // #6A2A22 @ α 0.30   (0.15–0.35)
  cold        // #3A1E1A @ α 0.22   (<0.15)
}
                                  // Used only by the followup Chart 04;
                                  // tokens land in B0 so they're stable when
                                  // the followup picks up.

typography {
  fontSerif   // "Cormorant Garamond", serif  — headlines, KPI values, callout titles
  fontSans    // "Inter", sans-serif           — body, nav, buttons
  fontMono    // "JetBrains Mono", monospace   — all numerics, axis labels, timestamps
  // type scale: handoff §Typography sizes
}

radius {
  card        // 6px
  sm          // 4px
}
```

Sidebar uses `surface-sidebar` (from existing tokens) plus the new
`warm.gold` for the active-row 2px left edge accent + tint.

### 4A.6 New fixtures (3) — `frontend/web/src/components/chart/v2/__fixtures__/`

- `multi-strategy-equity.json` — 5 strategies × 240 daily points,
  one drawdown per strategy, with lead marked. Generator seed = 1.
- `annotations.json` — the five annotations from the handoff
  (`bull-flag`, `volume-divergence`, `liquidation-wall (danger)`,
  `rsi-reset`, `break-of-structure`), each with `idx`, `side`,
  `type`, `title`, `body`, `conf`, `action`, optional `danger`.
- `monthly-returns.json` — 5 strategies × 17 months, deterministic
  via the handoff's seeded PRNG (mulberry32 seed 99).

`scripts/gen-chart-v2-fixtures.ts` extended with these three
generators (idempotent re-run).

### 4A.7 `/chart-lab/dashboards` (new tab)

Adds a fifth tab to `/chart-lab`:

| Tab | Content |
|---|---|
| Dashboards | Links to `/chart-lab/dashboards/{overview\|compare\|annotated\|hero}` each rendering the surface composition full-bleed against its fixture; serves as a visual regression target. |

## 5. Out of scope for A-M0

- Server-side `/api/v2/charts/*` endpoints — schemas are committed but
  the route handlers land in M1.
- Real live-streaming hookup — `useChart2Streaming` returns frozen
  fixture state in M0; the WS client wire-up is M3.
- v1 deletion — explicitly deferred to M4 so we can ramp on staff
  cookie before flipping defaults.

## 6. Payload schemas (Track B additions)

The Track B endpoints sit alongside the Track A `/api/v2/charts/*`
namespace. All payloads are columnar where applicable, JSON-encoded,
with timestamps in **seconds since epoch** for uPlot panes and **ms
since epoch** for KlineCharts panes (existing convention).

### 6.1 Multi-strategy equity bundle

`GET /api/v2/charts/dashboards/overview`

```ts
type MultiStrategyEquityBundle = {
  kind: "multi_strategy_equity";
  generatedAt: number;            // unix seconds
  granularity: string;            // "1d" | "1h" | …
  time: number[];                 // shared timeline, unix seconds
  strategies: Array<{
    id: string;                   // matches strategyRotation.id when known
    name: string;
    short: string;
    color: string;                // hex; if absent, surface picks from rotation
    kind: string;                 // "Trend" | "Momentum" | "Reversion" | "Vol" | "Bench"
    dashed?: boolean;             // true for benchmark
    equity: number[];             // % return, baselined at 0, same length as time[]
    drawdown: number[];           // ≤ 0, % from peak
    monthly: Array<{ year: number; month: number; value: number }>;
    metrics: {
      return: number; sharpe: number; mdd: number; win: number; pf: number;
    };
  }>;
  lead?: string;                  // optional override; defaults to strategies[0].id
};
```

### 6.2 Annotation payload

`GET /api/v2/charts/annotated/:run_id` *(or `/live/:symbol`, §11.2)*

```ts
type AnnotatedChartPayload = {
  kind: "annotated";
  source: "run" | "live";                  // provenance (§11.2)
  run_id?: string;                         // present when source = "run"
  symbol?: string;                         // present when source = "live"
  asset: string;
  granularity: string;
  candles: CandleColumns;                  // reuses Track A type
  ema?: LineSeries;                        // optional EMA(21) overlay
  annotations: Annotation[];               // may be [] if producer not wired
};

type Annotation = {
  idx: number;                             // candle index this is anchored to
  side: "top" | "bottom";                  // which row of callouts
  type: "PATTERN" | "FLOW" | "RISK" | "REVERSION" | "STRUCTURE";
  title: string;
  body: string;                            // 12–25 words
  conf: number;                            // 0..1
  action: "WATCH" | "LONG" | "SHORT" | "CAUTION";
  danger?: boolean;                        // tints callout red
  ts?: number;                             // generated-at unix seconds; for insight log timestamp
};
```

### 6.3 Liquidation-level payload *(reserved for the followup F-CHART-LIQHEAT)*

```ts
type LiquidationLevel = {
  price: number;
  heat: number;                            // 0..1
  notional: number;                        // millions USD
  side: "long" | "short";
};

type LiquidationHeatmapPayload = {
  kind: "liquidation_heatmap";
  asset: string;
  granularity: string;
  candles: CandleColumns;
  ema?: LineSeries;
  levels: LiquidationLevel[];              // top-N
  cascade: {
    longExposure: number;
    shortExposure: number;
    nearestWall: number;
    cascadeRisk: number;
  };
};
```

Defined now so the heat-ramp tokens land in B0 with a typed home.
Endpoint not implemented in this wave.

## 7. Information architecture — left nav + routes

**Sidebar (`frontend/web/src/components/shell/Sidebar.tsx`)** —
final order per §11.1 resolution:

```
Dashboard       /
Strategies      /strategies
Agents          /agents
Scenarios       /scenarios
Charts          /charts              ← new in B0, after Scenarios, before Eval
  Overview      /charts/overview     (B1)
  Compare       /charts/compare      (B2)
  Annotated     /charts/annotated    (B3)
  Hero          /charts/hero         (B4)
  Heatmap       /charts/heatmap      (followup F-CHART-LIQHEAT; hidden)
Eval            /eval-runs
Docs            /docs
Settings        /settings
```

Charts has its own sub-navigation, rendered as a top-of-page tab row
under `ChartsLayout`. It does not collapse the sidebar (the no-popups
rule continues to apply — the heatmap detail pane sits in the page
chrome, not a modal).

## 8. Verification

Track A verification is unchanged (see A-M0–A-M4 above).

Track B per-surface verification is listed inside each B-milestone.
Section-wide gates for B0–B4:

- `npm run typecheck` clean.
- `npm run build` clean.
- New routes appear in `frontend/web/src/routes.tsx` and resolve under
  staff cookie; sidebar hides Charts entry when cookie absent until
  `B-rollout`.
- `/chart-lab/dashboards` renders each new surface full-bleed against
  its fixture without console errors.
- Visual diff vs the three handoff PNGs
  (`docs/design/trading-charts/xvn theme charts*.png` and the
  ChatGPT renders) on the dark theme. Light/black themes pass a
  "no crashes, no z-index bugs" bar only.
- No new `lightweight-charts` imports anywhere under
  `frontend/web/src/components/chart/v2/` or `routes/charts/`.

## 9. Out of scope (for this spec)

- Mobile variants of the Track B dashboards. The handoff calls these
  out as a separate workstream; the existing mobile shell continues
  to apply only to Track A surfaces until that workstream lands.
- Real-time WebSocket wiring of `MultiStrategyEquityPane` for live
  performance updates. B1 reads a fresh-at-load bundle.
- Tweaks / per-chart runtime knob system. If a Track B surface needs
  a knob, fold it into the existing xvn tweaks pattern (handoff
  §Interactions note).
- AI-annotation **producer**. B3 consumes annotations; the producer
  (LLM call, schedule, persistence) is a separate spec.

## 10. Followup — F-CHART-LIQHEAT (Chart 04 Liquidation Heatmap)

Parked here so the implementation is ready when the followup picks up.
Per user direction 2026-05-23, this is **not** part of the current
wave.

**Trigger condition for un-parking:** liquidation-level data source
exists (broker feed or third-party feed) and a stakeholder confirms
the heatmap is the next chart to ship.

**Adds:**

- Surface: `LiquidationHeatmapDashboard` at `/charts/heatmap` (hidden
  from sidebar until trigger).
- Primitive: `LiquidationHeatbands` — absolutely-positioned overlay
  above `KlineCandlePane` (`z-index: 2`, `pointer-events: none`).
  Each band's height = `6px + heat * 18px`. Background gradient per
  handoff §04. Color from `heatRamp` (already landed in B0 tokens).
- Primitive: `LiquidationTopLevels` — right-rail table with heat bar
  + USD notional + side coloring.
- Primitive: `CascadeAnalytics` — 2×2 metric grid + heat-scale legend
  bar.
- Adapter: `liquidation-anchor.ts` — reuses `kline-anchor` to convert
  price → y-pixel; subscribes to layout changes to re-position bands.
- Endpoint: `/api/v2/charts/heatmap/:symbol` returning
  `LiquidationHeatmapPayload` (§6.3).

No frontend code lands for this followup in the current wave — only
the token group and the payload type, both already specified above.

## 11. Open-question resolutions (locked 2026-05-23)

The questions originally listed here have all been answered. Locked
choices are folded back into the relevant §2 / §3 / §6 / §7 sections
above. Captured here so the audit trail stays in one place.

1. **Sidebar placement → After `Scenarios`, before `Eval`.** Charts is
   treated as another "view results" surface alongside Eval. §7
   diagram updated accordingly.

2. **B3 data source → Both, with `?source=` switch.** Single B3 route
   serves both run-stored annotations and live on-demand annotations.
   Default `source=run` (B3 in-scope), `source=live` opt-in (requires
   the live annotation producer that is itself **explicitly out of
   scope** in §9 — B3 ships the live route with a "producer not
   wired" placeholder when the producer is absent). The
   `AnnotatedChartPayload` (§6.2) gains a `source: "run" | "live"`
   provenance field.

3. **B4 hero relationship to `/` → deferred.** Per user direction, we
   want to see all four canvases running in production before
   deciding whether `GradientHeroDashboard` should replace `/`. For
   this wave: B4 mounts only at `/charts/hero`; `/` is unchanged. The
   replace-or-not decision becomes a **B5 review milestone** after
   B4 lands — no code commitment now.

4. **Track B rollout granularity → single cookie.** All four canvases
   gated by `xvn.chartv2=1` (same cookie as Track A). Ship canvases
   one at a time; section becomes visible to all users at
   `B-rollout`. Per-canvas cookies considered and rejected to avoid
   flag sprawl.

5. **Strategy ID mapping → add `color` field to `Strategy` schema
   now.** No fallback ambiguity; dashboards always have a color to
   render. Schema addition belongs in B0 (see B0 task list below) —
   migration must go through the migration registry per
   `team/MANIFEST.md` and the `cycle-migration` skill conventions.
   Open sub-questions deferred to the migration PR: storage location
   (engine vs core), default-color backfill strategy (rotation index
   vs NULL → fall back to rotation at render time), pin-a-color UI
   surface (likely Strategy detail page).

6. **Demo home → `Dashboards` tab on `/chart-lab`.** B0 adds the
   tab. Storybook not introduced for this wave.

### B0 scope additions from the locked decisions

The B0 task list grows by:

- **Migration:** add `color` column (string, nullable) to whatever
  table owns Strategy metadata today (verify against `cycles` /
  strategy bundle store — see `crates/xvision-engine/migrations/`
  history). Allocate the next migration number through
  `team/MANIFEST.md`'s registry **before** writing the SQL file.
  Ship `_down.sql` per the project rule. Add a serialization /
  ts-rs export so the frontend sees `color?: string` on the
  Strategy type. **No backfill UI in B0** — picking a color is a
  followup (sketched in §11.5).
- **API:** the `/api/v2/charts/dashboards/overview` payload (§6.1)
  reads `color` per strategy when present; falls back to the
  `strategyRotation` palette by stable index when absent. The
  fallback rule must be documented inline in the payload builder
  so the frontend never re-implements it.
- **Frontend `?source=` handling for `/charts/annotated`:** parse
  the query param in `AIAnnotationDashboard`; default to `run`;
  when `source=live` is set, fetch from the live endpoint and
  render a "producer not configured" `EmptyState` if the response
  is empty / 404.

## 12. Sources

- KlineCharts on npm — https://www.npmjs.com/package/klinecharts
- KLineChart styles guide — https://klinecharts.com/en-US/guide/styles.html
- uPlot sync-cursor demo — https://leeoniya.github.io/uPlot/demos/sync-cursor.html
- uPlot docs README — https://github.com/leeoniya/uPlot#readme
- Design handoff (2026-05-22): `docs/design/trading-charts/XVN.zip`
  → `design_handoff_charts/README.md`,
  `design_handoff_charts/source/charts/{chart-theme.css,chart-helpers.js,chart-data.js}`,
  and the five `chart-*.jsx` references.
- Handoff renders: `docs/design/trading-charts/xvn theme charts.png`,
  `…charts 2.png`, and the three `ChatGPT Image …` PNGs.

# Chart rework — KlineCharts + uPlot (M0–M4)

Date: 2026-05-21
Status: M0 (foundation) — landed. M1–M4 sequenced behind feature flag.

> **Predecessors:** [TradingView Lightweight Eval Surface](./2026-05-14-tradingview-lightweight-eval-surface-design.md) | [TradingView Charts](./2026-05-11-tradingview-charts-design.md). This spec **does not delete** v1 (lightweight-charts) — it parks the v2 implementation next to v1 and ramps in over four milestones. v1 is deleted only at M4.

## 1. Purpose

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

## 2. Locked decisions

1. **Library split.**
   - KlineCharts owns: candles + candle-anchored overlays (SMA/EMA,
     Bollinger, Donchian) + candle-anchored markers (trades, vetoes,
     holds) + price-pane crosshair.
   - uPlot owns: equity, drawdown, histograms (volume), oscillator panes
     (RSI, MACD, ATR), compare overlays, line panes, wizard preview.
2. **Surface map (inherited).** The six chart surfaces stay 1:1 with v1:
   Run, Compare, Scenario, Strategy, Live, Wizard preview. Their
   payloads are migrated to the columnar v2 format below.
3. **Primitives ↔ surfaces split.** A surface composes primitives; a
   primitive renders one pane (candles, equity, drawdown, oscillator,
   line, compare overlay) plus shared chrome (frame, layer panel, marker
   dock, legend, connection status, cache badge, empty state, data
   table). Surfaces never `createChart()` directly — they assemble
   primitives.
4. **Columnar payload.** v2 payloads ship as parallel `Float64Array`-ish
   columns (`time[], open[], high[], low[], close[], volume[]`) plus a
   typed indicator map. Adapters translate columnar → KLineData[] and
   columnar → uPlot AlignedData. The HTTP endpoint moves to
   `/api/v2/charts/...` so v1 stays usable during the ramp.
5. **`/chart-lab` is staff-only.** Mounted at `/chart-lab` and gated by
   the same staff predicate the eval-review surface uses. v1 routes are
   untouched until M4.

## 3. Milestones

### M0 — Foundation (this PR)

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

### M1 — Eval surfaces (Run + Compare)

- Cut `RunChartV2` and `CompareChartV2` over to production routes
  behind a staff cookie (`xvn.chartv2=1`). Default = v1.
- Add `/api/v2/charts/run/:id` and `/api/v2/charts/compare/:cmp` returning
  columnar payloads; v1 endpoints stay live.
- Snapshot tests on adapter outputs (columnar ↔ KLineData, columnar ↔
  uPlot) for regressions.

### M2 — Scenario + Strategy

- Cut `ScenarioChartV2` and `StrategyChartV2` over behind the same
  cookie. Move scenario detail's chart-rail into v2.
- Strategy chart switches its compare overlay to `UplotCompareOverlayPane`.

### M3 — Live + Wizard preview

- `LiveChartV2` wires up the streaming hook (`useChart2Streaming`) and
  ConnectionStatus / CacheStatusBadge primitives.
- `WizardPreviewChartV2` replaces the inline preview SVG with a real
  candle pane.
- Cookie default flips to v2 once Live has been on staff cookie for a
  week with no regressions.

### M4 — v1 deletion

- Delete `frontend/web/src/components/chart/*Chart.tsx` (v1).
- Remove `lightweight-charts` dependency.
- Delete v1 fixtures + v1 chart tests.
- Surfaces directly export `*ChartV2` under the v1 names.

## 4. Foundation code (M0)

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

## 5. Out of scope for M0

- Server-side `/api/v2/charts/*` endpoints — schemas are committed but
  the route handlers land in M1.
- Real live-streaming hookup — `useChart2Streaming` returns frozen
  fixture state in M0; the WS client wire-up is M3.
- v1 deletion — explicitly deferred to M4 so we can ramp on staff
  cookie before flipping defaults.

## 6. Sources

- KlineCharts on npm — https://www.npmjs.com/package/klinecharts
- KLineChart styles guide — https://klinecharts.com/en-US/guide/styles.html
- uPlot sync-cursor demo — https://leeoniya.github.io/uPlot/demos/sync-cursor.html
- uPlot docs README — https://github.com/leeoniya/uPlot#readme

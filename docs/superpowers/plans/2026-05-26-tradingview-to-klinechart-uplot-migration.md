# Retire TradingView (lightweight-charts) → KlineChart + uPlot v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut the five remaining v1 lightweight-charts call sites (`eval-runs`, `eval-runs-detail`, `scenarios-detail`, `scenarios-new`, `live`) over to the v2 KlineChart+uPlot surfaces — closing the v2 renderer parity gaps first — then delete the v1 chart stack and the `lightweight-charts` dependency.

**Architecture:** Frontend-only. Each route keeps its existing row-based API call and pipes the payload through a `*PayloadToV2` adapter into a v2 surface (the pattern already shipping in `home.tsx`/`authoring.tsx`). Before any production route switches, the v2 `KlineCandlePane` must actually render candle-pane overlays, markers, and position bands (today they are converted then discarded as `void`), and the `ChartFrame` range controls must drive the visible window (today inert). The live route reuses the proven `useRunStream` SSE hook to feed a real (de-stubbed) `LiveChartV2`.

**Tech Stack:** React + TypeScript + Vite, `klinecharts@10.0.0-beta2`, `uplot@^1.6.32`, TanStack Query, Vitest + jsdom. No Rust/cargo. Spec: `docs/superpowers/specs/2026-05-26-tradingview-to-klinechart-uplot-migration-design.md`.

---

## Reference facts (verified, do not re-discover)

**Verify/test/build (run from `frontend/web/`):**
- Typecheck: `npm run typecheck` (`tsc -b`)
- Test: `npm test -- <path>` (`vitest run`), single file e.g. `npm test -- src/components/chart/v2/adapters/scenario-chart-payload.test.ts`
- Build: `npm run build` (`tsc -b && vite build`)
- There is **no `lint` script** — do not run lint.

**v2 columnar types** (`src/components/chart/v2/types.ts`):
```ts
export type CandleColumns = { time: number[]; open: number[]; high: number[]; low: number[]; close: number[]; volume: number[]; };
export type LineSeries = { time: number[]; value: number[]; };
export type IndicatorMap = { sma20?: LineSeries; sma30?: LineSeries; sma50?: LineSeries; sma60?: LineSeries; sma90?: LineSeries; sma200?: LineSeries; ema20?: LineSeries; ema30?: LineSeries; ema50?: LineSeries; ema60?: LineSeries; ema90?: LineSeries; ema200?: LineSeries; bollUpper?: LineSeries; bollMiddle?: LineSeries; bollLower?: LineSeries; donchianUpper?: LineSeries; donchianLower?: LineSeries; rsi?: LineSeries; macdLine?: LineSeries; macdSignal?: LineSeries; macdHist?: LineSeries; atr?: LineSeries; };
export type V2Marker = { kind: "buy" | "sell" | "veto" | "hold"; time: number; price?: number; text?: string; decision_index?: number; };
export type PositionSpan = { side: "long" | "short"; start: number; end: number; };
export type EquityPoint = { time: number; value: number };
export type DrawdownPoint = { time: number; value: number };
export type ScenarioChartV2Payload = { kind: "scenario"; asset: string; granularity: string; candles: CandleColumns; markers: V2Marker[]; positions: PositionSpan[]; equity: EquityPoint[]; }; // EXTENDED in Task 6 with `indicators: IndicatorMap`
export type WizardPreviewV2Payload = { kind: "wizard"; asset: string; granularity: string; candles: CandleColumns; equity: EquityPoint[]; };
export type LiveChartV2Payload = { kind: "live"; asset: string; granularity: string; candles: CandleColumns; equity: EquityPoint[]; markers: V2Marker[]; live_index: number; connection: "connected" | "reconnecting" | "offline"; cache: "fresh" | "cached" | "stale"; };
```

**v2 layers** (`src/components/chart/v2/hooks/useChart2Layers.ts`): `useChart2Layers(surface)` → `{ layers, toggle, set, reset }`, `layers: Record<ChartV2LayerKey, boolean>`. Current keys: `candles, sma20, sma50, sma200, ema20, ema50, bollinger, donchian, markerBuy, markerSell, markerVeto, markerHold, positionBand, volume, rsi, macd, atr, equity, drawdown, compareOverlay`. Surfaces keyed: `run`/`scenario`/`compare`/`strategy`. `DEFAULT_V2_LAYERS` defaults true: `candles, sma20, sma50, sma200, markerBuy, markerSell, markerVeto, positionBand, rsi, equity, drawdown`.

**Existing v2 primitive props** (all re-exported from `src/components/chart/v2/primitives/index.ts`):
- `MarkerDock`: `{ markers: V2Marker[]; activeId?: string; onSelect?: (id: string) => void }`
- `Legend`: `{ items: { label: string; color: string; dashed?: boolean; title?: string }[] }`
- `LayerPanel`: `{ groups: { title: string; items: { key: string; label: string; on: boolean }[] }[]; onToggle: (key: string) => void }`
- `EmptyState`: `{ title: string; message: string }`
- `ConnectionStatus`: `{ state: "connected"|"reconnecting"|"offline"; lastTickMs?: number | null }`
- `DataTable`: `{ columns: { key: string; header: string; align?: "left"|"right" }[]; rows: Record<string, string|number>[] }`
- `ChartFrame`: `{ title?; range: RangePreset; onRange: (r) => void; layersPanel?; dataTable?; children }`, exports `RangePreset = "1d"|"1w"|"1m"|"3m"|"All"` and `CHART_V2_ZOOM_EVENT = "xvn:chart-v2:zoom"`.
- `KlineCandlePane`: `{ candles: CandleColumns; overlays?: {...}; markers?: V2Marker[]; positions?: PositionSpan[]; height?: number; onReady?: (chart: Chart|null) => void }`

**klinecharts@10 Chart instance API** (from `node_modules/klinecharts/dist/index.d.ts`): `createIndicator(value, options?)`, `registerIndicator(template)`, `createOverlay(value)`, `registerOverlay(template)`, `convertToPixel(points, filter?)`, `subscribeAction("onVisibleRangeChange", cb)`/`unsubscribeAction`, `scrollToTimestamp(ts, ms?)`, `scrollToRealTime(ms?)`, `setBarSpace(n)`, `getBarSpace()`, `getVisibleRange()`, `getDom(paneId?)`, `setStyles`, `setDataLoader`. Overlay template: `createPointFigures(params) => OverlayFigure | OverlayFigure[]`, where `OverlayFigure = { key?; type: "rect"|"text"|"line"|"polygon"|...; attrs; styles?; ignoreEvent? }` and `params = { chart, overlay, coordinates: {x,y}[], bounding, xAxis, yAxis }`; overlay `points: { dataIndex?; timestamp?; value? }[]`.

**KlineCandlePane current stub** (`src/components/chart/v2/primitives/KlineCandlePane.tsx`): inits chart via `init(el)`, calls `onReadyRef.current?.(chart)`, `setSymbol`/`setPeriod`, `setDataLoader({ getBars: ({callback}) => callback(columnarToKLineData(candles), false) })`, applies theme via `setStyles(themeToKlinechartsStyles(theme))`, listens to `CHART_V2_ZOOM_EVENT`. Overlays/markers/positions are converted then `void`-suppressed (the gap).

**Adapters** (`src/components/chart/v2/adapters/`): `columnarToKLineData(candles): KLineData[]` (seconds→ms), `runChartPayloadToV2(RunChartPayload): RunChartV2Payload` (exists), `v2MarkersToKlineOverlay(markers, theme): unknown[]`, `createKlineAnchor(chart): { xForIndex, yForPrice, subscribeLayout }`. Barrel: `adapters/index.ts`.

**Vitest klinecharts mock pattern** (from `KlineCandlePane.test.tsx`):
```ts
const klineMocks = vi.hoisted(() => {
  const calls = { indicators: [] as unknown[], overlays: [] as unknown[] };
  const chart = {
    resize: vi.fn(), setStyles: vi.fn(), setSymbol: vi.fn(), setPeriod: vi.fn(),
    setDataLoader: vi.fn((l: { getBars: (p: { callback: (b: unknown[], m: boolean) => void }) => void }) =>
      l.getBars({ callback: () => {} })),
    createIndicator: vi.fn((v: unknown) => { calls.indicators.push(v); return "ind_1"; }),
    createOverlay: vi.fn((v: unknown) => { calls.overlays.push(v); return "ovl_1"; }),
    subscribeAction: vi.fn(), unsubscribeAction: vi.fn(),
    convertToPixel: vi.fn(() => ({ x: 0, y: 0 })), getDom: vi.fn(() => document.createElement("div")),
  };
  return { chart, calls, init: vi.fn(() => chart), dispose: vi.fn(), registerIndicator: vi.fn(), registerOverlay: vi.fn() };
});
vi.mock("klinecharts", () => ({
  init: klineMocks.init, dispose: klineMocks.dispose,
  registerIndicator: klineMocks.registerIndicator, registerOverlay: klineMocks.registerOverlay,
}));
```

**v1 components to delete (Phase 5):** `RunChart.tsx`, `ScenarioChart.tsx`, `StrategyChart.tsx`, `CompareChart.tsx`, `LiveChart.tsx`, `WizardPreviewChart.tsx`, `chart-fit.ts`, `chart-theme.ts`, `ChartContainer.tsx`, `ChartLayersPanel.tsx`, `MarkerSidePanel.tsx`, `chart-layers.ts`, `use-chart-layers.ts` + their `*.test.tsx`. **Keep** `use-run-stream.ts`. Also delete v2 stub `hooks/useChart2Streaming.ts` + `adapters/streaming.ts` and their barrel exports. `components/scenario/CacheStatusBadge.tsx`, `useBarsFetchJob.ts`, `BarsFetchJobStatus` are **scenario-domain (not chart-v1) — keep**.

---

## Phase 0 — Setup

### Task 0: Worktree + baseline

**Files:** none (environment).

- [ ] **Step 1: Create an isolated worktree** (per superpowers:using-git-worktrees) off the feature branch, OR continue on `feat/charts-v2-tradingview-removal` if working solo. Set a per-tree cargo target only if cargo runs (it won't here).

- [ ] **Step 2: Establish a green baseline**

Run (from `frontend/web/`):
```bash
npm run typecheck && npm test -- src/components/chart/v2
```
Expected: PASS (existing v2 suites green). If red on an unrelated suite, note it — do not fix unrelated failures.

- [ ] **Step 3: Confirm the five v1 call sites still exist**

Run (from repo root):
```bash
rg -n "from \"@/components/chart/(RunChart|ScenarioChart|LiveChart|WizardPreviewChart)\"" frontend/web/src/routes
```
Expected: matches in `eval-runs.tsx`, `eval-runs-detail.tsx`, `scenarios-detail.tsx`, `scenarios-new.tsx`, `live.tsx`.

---

## Phase 1 — KlineCandlePane parity (overlays, markers, position bands) + range controls

These are the spec's hard parity gates. RunChartV2 already ships in `home.tsx`, so all changes here must be additive and not regress that surface.

### Task 1: Render candle-pane line overlays (SMA/EMA/Bollinger/Donchian)

Render precomputed `LineSeries` overlays as polylines on the candle pane using a single registered custom overlay per active line, drawn from `convertToPixel` coordinates. (Indicators recompute from candles; our values are precomputed, so an overlay that draws the supplied points is the correct fit.)

**Files:**
- Create: `src/components/chart/v2/adapters/overlay-lines.ts`
- Create: `src/components/chart/v2/adapters/overlay-lines.test.ts`
- Modify: `src/components/chart/v2/primitives/KlineCandlePane.tsx`
- Modify: `src/components/chart/v2/primitives/KlineCandlePane.test.tsx`

- [ ] **Step 1: Write the failing adapter test**

`overlay-lines.ts` builds, from an `IndicatorMap` + theme, an array of overlay-create descriptors (one per present, active line key) whose `points` carry `{ timestamp, value }` and whose `extendData` carries the stroke color + dash flag.

```ts
// overlay-lines.test.ts
import { describe, expect, it } from "vitest";
import { overlayLineDescriptors, OVERLAY_LINE_KEYS } from "./overlay-lines";
import type { IndicatorMap } from "../types";

const theme = { overlay: { line: "#abc" } } as never; // theme lookups tolerate partial in test

describe("overlayLineDescriptors", () => {
  it("emits one descriptor per present line key with ms timestamps", () => {
    const indicators: IndicatorMap = {
      sma20: { time: [1, 2], value: [10, 11] },
      ema20: { time: [1, 2], value: [9, 9.5] },
    };
    const out = overlayLineDescriptors(indicators, theme, { sma20: true, ema20: true });
    expect(out).toHaveLength(2);
    const sma = out.find((d) => d.extendData.key === "sma20")!;
    expect(sma.name).toBe("xvnLine");
    expect(sma.points).toEqual([
      { timestamp: 1000, value: 10 },
      { timestamp: 2000, value: 11 },
    ]);
  });

  it("skips absent or toggled-off keys", () => {
    const out = overlayLineDescriptors({ sma20: { time: [1], value: [1] } }, theme, { sma20: false });
    expect(out).toHaveLength(0);
  });

  it("OVERLAY_LINE_KEYS covers the full v1 overlay set", () => {
    expect(OVERLAY_LINE_KEYS).toEqual(
      expect.arrayContaining(["sma20","sma30","sma50","sma60","sma90","sma200","ema20","ema30","ema50","ema60","ema90","ema200","bollUpper","bollMiddle","bollLower","donchianUpper","donchianLower"]),
    );
  });
});
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `npm test -- src/components/chart/v2/adapters/overlay-lines.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `overlay-lines.ts`**

```ts
import type { Chart2ThemeDefinition } from "@/theme/themes";
import type { IndicatorMap, LineSeries } from "../types";

export type OverlayLineKey = keyof IndicatorMap;

// Candle-pane overlay lines only (oscillators rsi/macd/atr live in uPlot subpanes).
export const OVERLAY_LINE_KEYS: OverlayLineKey[] = [
  "sma20","sma30","sma50","sma60","sma90","sma200",
  "ema20","ema30","ema50","ema60","ema90","ema200",
  "bollUpper","bollMiddle","bollLower",
  "donchianUpper","donchianLower",
];

const DASHED = new Set<OverlayLineKey>(["ema20","ema30","ema50","ema60","ema90","ema200"]);

export type OverlayLineDescriptor = {
  name: "xvnLine";
  points: { timestamp: number; value: number }[];
  extendData: { key: OverlayLineKey; color: string; dashed: boolean };
};

function colorFor(theme: Chart2ThemeDefinition, key: OverlayLineKey): string {
  const t = theme as unknown as { overlay?: Record<string, string> };
  return t.overlay?.[key] ?? t.overlay?.line ?? "#888888";
}

function toPoints(series: LineSeries): { timestamp: number; value: number }[] {
  const out: { timestamp: number; value: number }[] = [];
  for (let i = 0; i < series.time.length; i++) {
    out.push({ timestamp: series.time[i] * 1000, value: series.value[i] });
  }
  return out;
}

export function overlayLineDescriptors(
  indicators: IndicatorMap,
  theme: Chart2ThemeDefinition,
  active: Partial<Record<string, boolean>>,
): OverlayLineDescriptor[] {
  const out: OverlayLineDescriptor[] = [];
  for (const key of OVERLAY_LINE_KEYS) {
    const series = indicators[key];
    if (!series || series.time.length === 0) continue;
    if (active[key] === false) continue;
    out.push({
      name: "xvnLine",
      points: toPoints(series),
      extendData: { key, color: colorFor(theme, key), dashed: DASHED.has(key) },
    });
  }
  return out;
}
```

Map UI layer keys to overlay keys for the `active` arg in callers: `bollinger` toggles `bollUpper/Middle/Lower`; `donchian` toggles `donchianUpper/Lower`; `sma20/sma50/sma200` and `ema20/ema50` map 1:1. Keys without a layer toggle (sma30/60/90, ema30/60/90/200) are treated as always-on when present (callers pass `active` without those keys, so `active[key] === false` is false).

- [ ] **Step 4: Run the test, verify it passes**

Run: `npm test -- src/components/chart/v2/adapters/overlay-lines.test.ts`
Expected: PASS.

- [ ] **Step 5: Register the `xvnLine` overlay and create overlays in KlineCandlePane**

In `KlineCandlePane.tsx`, replace the `void _overlayExtData` stub. Register once at module scope, and after the chart is ready + data loaded, create one overlay per descriptor.

```ts
// top of KlineCandlePane.tsx, after imports
import { registerOverlay } from "klinecharts";
import { overlayLineDescriptors } from "../adapters/overlay-lines";

let xvnLineRegistered = false;
function ensureXvnLineOverlay() {
  if (xvnLineRegistered) return;
  xvnLineRegistered = true;
  registerOverlay({
    name: "xvnLine",
    totalStep: 1,
    needDefaultPointFigure: false,
    needDefaultXAxisFigure: false,
    needDefaultYAxisFigure: false,
    createPointFigures: ({ coordinates, overlay }) => {
      const ext = overlay.extendData as { color: string; dashed: boolean };
      if (!coordinates || coordinates.length < 2) return [];
      return [{
        type: "line",
        attrs: { coordinates },
        styles: { color: ext.color, size: 1, style: ext.dashed ? "dashed" : "solid" },
        ignoreEvent: true,
      }];
    },
  });
}
```

In the data-load effect (where `_overlayExtData` was), after `setDataLoader`, create overlays. Because `overlays` is the v2 surface's `overlays?: {...}` prop of `LineSeries`, accept it directly (it is already an `IndicatorMap`-compatible subset). Pass an `active` map from the surface via a new optional prop `overlayActive?: Partial<Record<string, boolean>>` (default `{}`):

```ts
ensureXvnLineOverlay();
const descriptors = overlayLineDescriptors(
  (overlays ?? {}) as IndicatorMap,
  theme,
  overlayActive ?? {},
);
for (const d of descriptors) {
  chart.createOverlay({ name: d.name, points: d.points, extendData: d.extendData });
}
```

Add `overlayActive?: Partial<Record<string, boolean>>;` to `KlineCandlePaneProps`. Remove the `void _overlayExtData;` line.

- [ ] **Step 6: Add the KlineCandlePane render test**

Append to `KlineCandlePane.test.tsx` (mock already provides `createOverlay`):
```ts
it("creates an xvnLine overlay per active candle-pane line", async () => {
  render(<KlineCandlePane candles={fixtureCandles()} overlays={{ sma20: { time: [1,2], value: [1,2] } }} overlayActive={{ sma20: true }} />);
  await waitFor(() => expect(klineMocks.chart.createOverlay).toHaveBeenCalled());
  const names = klineMocks.calls.overlays.map((o: { name: string }) => o.name);
  expect(names).toContain("xvnLine");
});
```
(Reuse the file's existing `fixtureCandles()`/render helper; if absent, build a minimal `CandleColumns` with 2 bars.)

- [ ] **Step 7: Run KlineCandlePane tests + typecheck**

Run: `npm test -- src/components/chart/v2/primitives/KlineCandlePane.test.tsx && npm run typecheck`
Expected: PASS. **Manual visual check** deferred to Phase 6.

- [ ] **Step 8: Commit**

```bash
git add src/components/chart/v2/adapters/overlay-lines.ts src/components/chart/v2/adapters/overlay-lines.test.ts src/components/chart/v2/primitives/KlineCandlePane.tsx src/components/chart/v2/primitives/KlineCandlePane.test.tsx
git commit -m "feat(chart-v2): render candle-pane line overlays via xvnLine overlay"
```

### Task 2: Render buy/sell/veto/hold markers on the candle pane

**Files:**
- Modify: `src/components/chart/v2/adapters/markers.ts` (already produces descriptors; ensure shape matches createOverlay)
- Modify: `src/components/chart/v2/primitives/KlineCandlePane.tsx`
- Modify: `src/components/chart/v2/primitives/KlineCandlePane.test.tsx`

- [ ] **Step 1: Write the failing test**
```ts
it("creates a marker overlay for each marker", async () => {
  render(<KlineCandlePane candles={fixtureCandles()} markers={[
    { kind: "buy", time: 1, price: 10, text: "Buy 1" },
    { kind: "veto", time: 2, price: 11, text: "Veto: risk" },
  ]} />);
  await waitFor(() => expect(klineMocks.chart.createOverlay).toHaveBeenCalled());
  const names = klineMocks.calls.overlays.map((o: { name: string }) => o.name);
  expect(names.filter((n) => n === "xvnMarker")).toHaveLength(2);
});
```

- [ ] **Step 2: Run, verify fail** — `npm test -- src/components/chart/v2/primitives/KlineCandlePane.test.tsx` → FAIL (markers discarded).

- [ ] **Step 3: Register `xvnMarker` overlay + create markers**

In `KlineCandlePane.tsx`, register a marker overlay drawing an arrow (buy/sell) or circle (veto/hold) with a text label, anchored at the marker's `{ timestamp, value }`:
```ts
let xvnMarkerRegistered = false;
function ensureXvnMarkerOverlay() {
  if (xvnMarkerRegistered) return;
  xvnMarkerRegistered = true;
  registerOverlay({
    name: "xvnMarker",
    totalStep: 1,
    needDefaultPointFigure: false, needDefaultXAxisFigure: false, needDefaultYAxisFigure: false,
    createPointFigures: ({ coordinates, overlay }) => {
      const ext = overlay.extendData as { kind: string; text: string; color: string };
      const c = coordinates?.[0];
      if (!c) return [];
      const up = ext.kind === "buy";
      const isArrow = ext.kind === "buy" || ext.kind === "sell";
      const yOff = up ? 14 : -14;
      const figs: unknown[] = [];
      if (isArrow) {
        figs.push({ type: "polygon", attrs: { coordinates: [
          { x: c.x, y: c.y + (up ? 8 : -8) },
          { x: c.x - 5, y: c.y + yOff },
          { x: c.x + 5, y: c.y + yOff },
        ] }, styles: { style: "fill", color: ext.color }, ignoreEvent: true });
      } else {
        figs.push({ type: "circle", attrs: { x: c.x, y: c.y, r: 4 }, styles: { style: "fill", color: ext.color }, ignoreEvent: true });
      }
      if (ext.text) {
        figs.push({ type: "text", attrs: { x: c.x + 6, y: c.y + yOff, text: ext.text }, styles: { color: ext.color, size: 10 }, ignoreEvent: true });
      }
      return figs as never;
    },
  });
}
```
Replace the discarded `_markerExtData` block: build descriptors from the `markers` prop directly (don't rely on the existing `v2MarkersToKlineOverlay` `unknown[]` shape if it diverges — inline the mapping):
```ts
ensureXvnMarkerOverlay();
for (const m of markers ?? []) {
  if (m.price == null) continue;
  chart.createOverlay({
    name: "xvnMarker",
    points: [{ timestamp: m.time * 1000, value: m.price }],
    extendData: { kind: m.kind, text: m.text ?? "", color: theme.marker[m.kind] },
  });
}
```
Remove `void _markerExtData;`. Keep `MarkerDock` in surfaces (it complements, not replaces, on-chart markers).

- [ ] **Step 4: Run, verify pass** — `npm test -- src/components/chart/v2/primitives/KlineCandlePane.test.tsx` → PASS.

- [ ] **Step 5: Commit**
```bash
git add src/components/chart/v2/primitives/KlineCandlePane.tsx src/components/chart/v2/primitives/KlineCandlePane.test.tsx
git commit -m "feat(chart-v2): render trade/veto/hold markers on candle pane"
```

### Task 3: Render long/short position bands

**Files:**
- Modify: `src/components/chart/v2/primitives/KlineCandlePane.tsx`
- Modify: `src/components/chart/v2/primitives/KlineCandlePane.test.tsx`

- [ ] **Step 1: Write the failing test**
```ts
it("creates a position-band overlay per span", async () => {
  render(<KlineCandlePane candles={fixtureCandles()} positions={[{ side: "long", start: 1, end: 2 }]} />);
  await waitFor(() => expect(klineMocks.chart.createOverlay).toHaveBeenCalled());
  expect(klineMocks.calls.overlays.some((o: { name: string }) => o.name === "xvnPositionBand")).toBe(true);
});
```

- [ ] **Step 2: Run, verify fail** → FAIL.

- [ ] **Step 3: Register `xvnPositionBand` overlay + create bands**

A band is a full-height shaded rectangle between `start` and `end` timestamps. Use the pane bounding for height (figures get `bounding`):
```ts
let xvnBandRegistered = false;
function ensureXvnBandOverlay() {
  if (xvnBandRegistered) return;
  xvnBandRegistered = true;
  registerOverlay({
    name: "xvnPositionBand",
    totalStep: 1,
    needDefaultPointFigure: false, needDefaultXAxisFigure: false, needDefaultYAxisFigure: false,
    createPointFigures: ({ coordinates, bounding, overlay }) => {
      const ext = overlay.extendData as { color: string };
      if (!coordinates || coordinates.length < 2) return [];
      const x0 = coordinates[0].x; const x1 = coordinates[1].x;
      return [{
        type: "rect",
        attrs: { x: Math.min(x0, x1), y: 0, width: Math.abs(x1 - x0), height: bounding.height },
        styles: { style: "fill", color: ext.color },
        ignoreEvent: true,
      }] as never;
    },
  });
}
```
Create one per span (anchor both points at the same arbitrary value 0; only `.x` is used):
```ts
ensureXvnBandOverlay();
for (const p of positions ?? []) {
  chart.createOverlay({
    name: "xvnPositionBand",
    points: [
      { timestamp: p.start * 1000, value: 0 },
      { timestamp: p.end * 1000, value: 0 },
    ],
    extendData: { color: p.side === "long" ? theme.position.longBand : theme.position.shortBand },
  });
}
```
Remove `void _positionExtData;`. If `theme.position.longBand/shortBand` keys don't exist, use `theme.position.longLine`/`shortLine` with low-opacity suffix (verify the theme shape via `useChart2Theme` during implementation; `Legend` in ScenarioChartV2 already references `theme.position.longLine`).

- [ ] **Step 4: Run, verify pass** → PASS.

- [ ] **Step 5: Commit**
```bash
git add src/components/chart/v2/primitives/KlineCandlePane.tsx src/components/chart/v2/primitives/KlineCandlePane.test.tsx
git commit -m "feat(chart-v2): render long/short position bands on candle pane"
```

### Task 4: Range presets drive the visible window

Implement a shared range event mirroring `CHART_V2_ZOOM_EVENT`. `ChartFrame` dispatches it on `onRange`; `KlineCandlePane` and `usePlot` apply it. Per spec, if a surface cannot reliably support range for KlineCharts, hide the control for that surface — but implement the uPlot path fully and the Kline path best-effort.

**Files:**
- Create: `src/components/chart/v2/primitives/range-window.ts` (+ `.test.ts`)
- Modify: `src/components/chart/v2/primitives/ChartFrame.tsx`
- Modify: `src/components/chart/v2/primitives/usePlot.ts`
- Modify: `src/components/chart/v2/primitives/KlineCandlePane.tsx`

- [ ] **Step 1: Write the failing helper test**
```ts
// range-window.test.ts
import { describe, expect, it } from "vitest";
import { rangeWindowSeconds, granularitySeconds } from "./range-window";

describe("rangeWindowSeconds", () => {
  it("maps presets to seconds, null for All", () => {
    expect(rangeWindowSeconds("1d")).toBe(86_400);
    expect(rangeWindowSeconds("1w")).toBe(7 * 86_400);
    expect(rangeWindowSeconds("1m")).toBe(30 * 86_400);
    expect(rangeWindowSeconds("3m")).toBe(90 * 86_400);
    expect(rangeWindowSeconds("All")).toBeNull();
  });
  it("parses granularity to seconds", () => {
    expect(granularitySeconds("1h")).toBe(3600);
    expect(granularitySeconds("15m")).toBe(900);
    expect(granularitySeconds("1d")).toBe(86_400);
    expect(granularitySeconds("bogus")).toBeNull();
  });
});
```

- [ ] **Step 2: Run, verify fail** → FAIL.

- [ ] **Step 3: Implement `range-window.ts`**
```ts
import type { RangePreset } from "./ChartFrame";

export function rangeWindowSeconds(preset: RangePreset): number | null {
  switch (preset) {
    case "1d": return 86_400;
    case "1w": return 7 * 86_400;
    case "1m": return 30 * 86_400;
    case "3m": return 90 * 86_400;
    case "All": return null;
  }
}

export function granularitySeconds(g: string): number | null {
  const m = /^(\d+)\s*([mhdwM])$/.exec(g.trim());
  if (!m) return null;
  const n = Number(m[1]);
  switch (m[2]) {
    case "m": return n * 60;
    case "h": return n * 3600;
    case "d": return n * 86_400;
    case "w": return n * 7 * 86_400;
    case "M": return n * 30 * 86_400;
    default: return null;
  }
}
```

- [ ] **Step 4: Run, verify pass** → PASS.

- [ ] **Step 5: Add the range event + dispatch in ChartFrame**

In `ChartFrame.tsx` add `export const CHART_V2_RANGE_EVENT = "xvn:chart-v2:range";` and in the range button `onClick`, after `onRange(r)`, dispatch:
```ts
window.dispatchEvent(new CustomEvent(CHART_V2_RANGE_EVENT, { detail: r }));
```

- [ ] **Step 6: uPlot consumes the range event**

In `usePlot.ts`, add a listener alongside the existing zoom listener. On a preset with a finite window, set x-scale to `[maxX - windowSec, maxX]` (uPlot x is in seconds, matching data[0]); on `"All"`, reset to full extent:
```ts
import { CHART_V2_RANGE_EVENT } from "./ChartFrame";
import { rangeWindowSeconds } from "./range-window";
// inside the effect that adds onZoom:
const onRange = (event: Event) => {
  const preset = (event as CustomEvent<string>).detail;
  const plot = plotRef.current;
  if (!plot) return;
  const xs = plot.data[0];
  if (!xs || xs.length === 0) return;
  const maxX = xs[xs.length - 1] as number;
  const win = rangeWindowSeconds(preset as never);
  if (win == null) { plot.setScale("x", { min: xs[0] as number, max: maxX }); return; }
  plot.setScale("x", { min: maxX - win, max: maxX });
};
window.addEventListener(CHART_V2_RANGE_EVENT, onRange);
// in cleanup: window.removeEventListener(CHART_V2_RANGE_EVENT, onRange);
```

- [ ] **Step 7: KlineCandlePane consumes the range event (best-effort)**

In `KlineCandlePane.tsx`, after the chart is ready, subscribe to `CHART_V2_RANGE_EVENT`. Compute target bar count from the preset window ÷ candle interval (derive interval from the first two candle timestamps), then `setBarSpace` to fit that count into the pane width and `scrollToRealTime()`:
```ts
import { CHART_V2_RANGE_EVENT } from "./ChartFrame";
import { rangeWindowSeconds } from "./range-window";
// in the init effect, after onReady:
const onRange = (event: Event) => {
  const preset = (event as CustomEvent<string>).detail;
  const ch = chartRef.current;
  if (!ch) return;
  const t = candles.time;
  if (t.length < 2) return;
  const win = rangeWindowSeconds(preset as never);
  const dom = ch.getDom();
  const width = dom?.clientWidth ?? 600;
  if (win == null) { ch.setBarSpace(Math.max(1, width / t.length)); ch.scrollToRealTime(); return; }
  const intervalSec = Math.max(1, t[t.length - 1] - t[t.length - 2]);
  const count = Math.max(1, Math.ceil(win / intervalSec));
  ch.setBarSpace(Math.max(1, width / count));
  ch.scrollToRealTime();
};
window.addEventListener(CHART_V2_RANGE_EVENT, onRange);
// cleanup: window.removeEventListener(CHART_V2_RANGE_EVENT, onRange);
```

- [ ] **Step 8: Run typecheck + existing chart suites**

Run: `npm run typecheck && npm test -- src/components/chart/v2`
Expected: PASS. Range behavior is verified visually in Phase 6. **If, at Phase 6 visual check, the KlineCharts range does not reliably reflect presets, hide the range buttons on Kline-containing surfaces** by passing a `showRange={false}`-style guard (add a `rangeEnabled?: boolean` prop to `ChartFrame`, default true, and set false where unsupported) — do not ship inert controls.

- [ ] **Step 9: Commit**
```bash
git add src/components/chart/v2/primitives/range-window.ts src/components/chart/v2/primitives/range-window.test.ts src/components/chart/v2/primitives/ChartFrame.tsx src/components/chart/v2/primitives/usePlot.ts src/components/chart/v2/primitives/KlineCandlePane.tsx
git commit -m "feat(chart-v2): range presets drive visible window (uPlot + kline)"
```

---

## Phase 2 — Adapters

### Task 5: Extend `ScenarioChartV2Payload` with indicators + `scenarioChartPayloadToV2`

**Files:**
- Modify: `src/components/chart/v2/types.ts`
- Create: `src/components/chart/v2/adapters/scenario-chart-payload.ts` (+ `.test.ts`)

- [ ] **Step 1: Extend the type** — in `types.ts`, add `indicators: IndicatorMap;` to `ScenarioChartV2Payload` (after `candles`).

- [ ] **Step 2: Write the failing adapter test**
```ts
// scenario-chart-payload.test.ts
import { describe, expect, it } from "vitest";
import { scenarioChartPayloadToV2 } from "./scenario-chart-payload";
import type { ScenarioChartPayload } from "@/api/types.gen";

function payload(): ScenarioChartPayload {
  return {
    scenario: { id: "s1", granularity: "1h" } as never,
    bars: [
      { time: 1, open: 1, high: 2, low: 0.5, close: 1.5, volume: 100 },
      { time: 2, open: 1.5, high: 2.5, low: 1, close: 2, volume: 120 },
    ],
    indicators: { sma_20: [{ time: 1, value: 1.2 }], bollinger: { upper: [], middle: [], lower: [] }, donchian: { upper: [], lower: [] }, macd: { line: [], signal: [], histogram: [] } } as never,
    cache_status: { type: "FullyCached", bar_count: 2 } as never,
  } as ScenarioChartPayload;
}

describe("scenarioChartPayloadToV2", () => {
  it("maps bars to columnar candles", () => {
    const out = scenarioChartPayloadToV2(payload(), "BTC/USD", "1h");
    expect(out.kind).toBe("scenario");
    expect(out.asset).toBe("BTC/USD");
    expect(out.candles.time).toEqual([1, 2]);
    expect(out.candles.close).toEqual([1.5, 2]);
  });
  it("maps indicators into IndicatorMap and defaults empty arrays", () => {
    const out = scenarioChartPayloadToV2(payload(), "BTC/USD", "1h");
    expect(out.indicators.sma20).toEqual({ time: [1], value: [1.2] });
    expect(out.equity).toEqual([]);
    expect(out.markers).toEqual([]);
    expect(out.positions).toEqual([]);
  });
});
```

- [ ] **Step 3: Run, verify fail** → FAIL (module not found).

- [ ] **Step 4: Implement the adapter**

Reuse the `line()` and `indicatorMap()` shape from `run-chart-payload.ts` (the v1 `Indicators` shape is shared). The adapter takes the v1 payload plus the route-chosen `asset` and `granularity` (since `ScenarioChartPayload` carries asset only inside `scenario`):
```ts
import type { ScenarioChartPayload } from "@/api/types.gen";
import type { IndicatorMap, ScenarioChartV2Payload } from "../types";

export function scenarioChartPayloadToV2(
  payload: ScenarioChartPayload,
  asset: string,
  granularity: string,
): ScenarioChartV2Payload {
  return {
    kind: "scenario",
    asset,
    granularity,
    candles: {
      time: payload.bars.map((b) => b.time),
      open: payload.bars.map((b) => b.open),
      high: payload.bars.map((b) => b.high),
      low: payload.bars.map((b) => b.low),
      close: payload.bars.map((b) => b.close),
      volume: payload.bars.map((b) => b.volume),
    },
    indicators: indicatorMap(payload.indicators),
    equity: [],
    markers: [],
    positions: [],
  };
}

function line(points: Array<{ time: number; value: number }> | undefined) {
  const rows = points ?? [];
  return { time: rows.map((p) => p.time), value: rows.map((p) => p.value) };
}

function indicatorMap(i: ScenarioChartPayload["indicators"]): IndicatorMap {
  return {
    sma20: line(i.sma_20), sma30: line(i.sma_30), sma50: line(i.sma_50),
    sma60: line(i.sma_60), sma90: line(i.sma_90), sma200: line(i.sma_200),
    ema20: line(i.ema_20), ema30: line(i.ema_30), ema50: line(i.ema_50),
    ema60: line(i.ema_60), ema90: line(i.ema_90), ema200: line(i.ema_200),
    bollUpper: line(i.bollinger.upper), bollMiddle: line(i.bollinger.middle), bollLower: line(i.bollinger.lower),
    donchianUpper: line(i.donchian.upper), donchianLower: line(i.donchian.lower),
    rsi: line(i.rsi_14), macdLine: line(i.macd.line), macdSignal: line(i.macd.signal),
    macdHist: line(i.macd.histogram), atr: line(i.atr_14),
  };
}
```
(If `line()`/`indicatorMap()` should be shared with `run-chart-payload.ts`, that file's `indicatorMap` takes the whole payload; keep a local copy here — DRY across two small adapters is not worth a shared-module coupling. Verify exact `Indicators` field names against `run-chart-payload.ts:36-62` which already uses them.)

- [ ] **Step 5: Run, verify pass** → `npm test -- src/components/chart/v2/adapters/scenario-chart-payload.test.ts` → PASS.

- [ ] **Step 6: Export from barrel** — add to `adapters/index.ts`:
```ts
export { scenarioChartPayloadToV2 } from "./scenario-chart-payload";
```

- [ ] **Step 7: Commit**
```bash
git add src/components/chart/v2/types.ts src/components/chart/v2/adapters/scenario-chart-payload.ts src/components/chart/v2/adapters/scenario-chart-payload.test.ts src/components/chart/v2/adapters/index.ts
git commit -m "feat(chart-v2): scenarioChartPayloadToV2 adapter + indicators on payload"
```

### Task 6: `scenarioPreviewToWizardV2` adapter

**Files:**
- Create: `src/components/chart/v2/adapters/scenario-preview-payload.ts` (+ `.test.ts`)
- Modify: `src/components/chart/v2/adapters/index.ts`

- [ ] **Step 1: Write the failing test**
```ts
// scenario-preview-payload.test.ts
import { describe, expect, it } from "vitest";
import { scenarioPreviewToWizardV2 } from "./scenario-preview-payload";
import type { ScenarioPreviewPayload } from "@/api/types.gen";

function preview(): ScenarioPreviewPayload {
  return {
    cache_key: "k", asset: "ETH", granularity: "1h",
    bars: [{ time: 1, open: 1, high: 2, low: 0.5, close: 1.5, volume: 10 }],
    cache_status: { type: "FullyCached", bar_count: 1 } as never,
    baseline_equity: [{ time: 1, equity_usd: 1000 }],
  } as ScenarioPreviewPayload;
}

describe("scenarioPreviewToWizardV2", () => {
  it("maps bars to candles and baseline_equity to equity", () => {
    const out = scenarioPreviewToWizardV2(preview());
    expect(out.kind).toBe("wizard");
    expect(out.asset).toBe("ETH");
    expect(out.candles.time).toEqual([1]);
    expect(out.equity).toEqual([{ time: 1, value: 1000 }]);
  });
  it("defaults equity to [] when baseline_equity is null", () => {
    const p = preview(); p.baseline_equity = null;
    expect(scenarioPreviewToWizardV2(p).equity).toEqual([]);
  });
});
```

- [ ] **Step 2: Run, verify fail** → FAIL.

- [ ] **Step 3: Implement**
```ts
import type { ScenarioPreviewPayload } from "@/api/types.gen";
import type { WizardPreviewV2Payload } from "../types";

export function scenarioPreviewToWizardV2(p: ScenarioPreviewPayload): WizardPreviewV2Payload {
  return {
    kind: "wizard",
    asset: p.asset,
    granularity: p.granularity,
    candles: {
      time: p.bars.map((b) => b.time),
      open: p.bars.map((b) => b.open),
      high: p.bars.map((b) => b.high),
      low: p.bars.map((b) => b.low),
      close: p.bars.map((b) => b.close),
      volume: p.bars.map((b) => b.volume),
    },
    equity: (p.baseline_equity ?? []).map((e) => ({ time: e.time, value: e.equity_usd })),
  };
}
```

- [ ] **Step 4: Run, verify pass** → PASS.

- [ ] **Step 5: Export + commit**
```bash
# add `export { scenarioPreviewToWizardV2 } from "./scenario-preview-payload";` to adapters/index.ts
git add src/components/chart/v2/adapters/scenario-preview-payload.ts src/components/chart/v2/adapters/scenario-preview-payload.test.ts src/components/chart/v2/adapters/index.ts
git commit -m "feat(chart-v2): scenarioPreviewToWizardV2 adapter"
```

---

## Phase 3 — Surface enhancements

### Task 7: ScenarioChartV2 parity (indicators overlays, data table, no empty equity)

**Files:**
- Modify: `src/components/chart/v2/surfaces/ScenarioChartV2.tsx`
- Create: `src/components/chart/v2/surfaces/ScenarioChartV2.test.tsx`

- [ ] **Step 1: Write the failing test** (mock klinecharts + uPlot panes to null as in `b1-primitives.test.tsx`)
```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ScenarioChartV2 } from "./ScenarioChartV2";
import type { ScenarioChartV2Payload } from "../types";

vi.mock("../primitives/KlineCandlePane", () => ({ KlineCandlePane: () => <div data-testid="kline" /> }));
vi.mock("../primitives/UplotHistogramPane", () => ({ UplotHistogramPane: () => null }));

function payload(): ScenarioChartV2Payload {
  return { kind: "scenario", asset: "BTC/USD", granularity: "1h",
    candles: { time: [1,2], open: [1,1], high: [2,2], low: [0,0], close: [1.5,2], volume: [10,12] },
    indicators: { sma20: { time: [1,2], value: [1,1.5] } }, equity: [], markers: [], positions: [] };
}

describe("ScenarioChartV2", () => {
  it("renders candle pane and a bars data table", () => {
    render(<ScenarioChartV2 payload={payload()} />);
    expect(screen.getByTestId("kline")).toBeInTheDocument();
    expect(screen.getByText("Time")).toBeInTheDocument(); // DataTable header
  });
});
```

- [ ] **Step 2: Run, verify fail** → FAIL (no data table; surface needs update).

- [ ] **Step 3: Update ScenarioChartV2**

Wire `indicators` into the candle pane overlays + `overlayActive` from `useChart2Layers("scenario")`, add a `DataTable` of the first 200 bars via `ChartFrame`'s `dataTable` slot, add overlay layer toggles to the existing `LayerPanel`, and default the equity layer off (scenarios have no equity). Concretely:
```tsx
import { ChartFrame, KlineCandlePane, LayerPanel, Legend, MarkerDock, PaneStack, UplotHistogramPane, DataTable, type RangePreset } from "../primitives";
// ...
const overlays = {
  sma20: payload.indicators.sma20, sma50: payload.indicators.sma50, sma200: payload.indicators.sma200,
  ema20: payload.indicators.ema20, ema50: payload.indicators.ema50,
  bollUpper: payload.indicators.bollUpper, bollMiddle: payload.indicators.bollMiddle, bollLower: payload.indicators.bollLower,
  donchianUpper: payload.indicators.donchianUpper, donchianLower: payload.indicators.donchianLower,
};
const overlayActive = {
  sma20: layers.sma20, sma50: layers.sma50, sma200: layers.sma200,
  ema20: layers.ema20, ema50: layers.ema50,
  bollUpper: layers.bollinger, bollMiddle: layers.bollinger, bollLower: layers.bollinger,
  donchianUpper: layers.donchian, donchianLower: layers.donchian,
};
const tableRows = payload.candles.time.slice(0, 200).map((t, i) => ({
  time: t, open: payload.candles.open[i], high: payload.candles.high[i],
  low: payload.candles.low[i], close: payload.candles.close[i], volume: payload.candles.volume[i],
}));
```
Pass `dataTable={<DataTable columns={[{key:"time",header:"Time"},{key:"open",header:"Open",align:"right"},{key:"high",header:"High",align:"right"},{key:"low",header:"Low",align:"right"},{key:"close",header:"Close",align:"right"},{key:"volume",header:"Volume",align:"right"}]} rows={tableRows} />}` into `ChartFrame`. Add to the `KlineCandlePane`: `overlays={overlays} overlayActive={overlayActive}`. Add overlay + Bollinger/Donchian items to the `LayerPanel` groups (mirroring the existing Markers/Panes groups). Remove the always-on equity pane; only render `UplotEquityPane` when `layers.equity && payload.equity.length > 0`. Set `useChart2Layers("scenario")` default: since the hook reads `DEFAULT_V2_LAYERS` (equity:true), guard the render with `payload.equity.length > 0` so empty scenarios never show an empty equity pane.

- [ ] **Step 4: Run, verify pass** → `npm test -- src/components/chart/v2/surfaces/ScenarioChartV2.test.tsx` → PASS.

- [ ] **Step 5: Commit**
```bash
git add src/components/chart/v2/surfaces/ScenarioChartV2.tsx src/components/chart/v2/surfaces/ScenarioChartV2.test.tsx
git commit -m "feat(chart-v2): ScenarioChartV2 indicators overlays + bars data table"
```

### Task 8: WizardPreviewChartV2 fetching container (gate, debounce, baseline, fetch-job)

**Files:**
- Create: `src/components/chart/v2/surfaces/WizardPreviewChartV2Container.tsx`
- Create: `src/components/chart/v2/surfaces/WizardPreviewChartV2Container.test.tsx`

This container reproduces v1 `WizardPreviewChart` behavior but renders `WizardPreviewChartV2`. Props match v1 exactly: `{ asset; from; to; granularity; includeBaseline? }`.

- [ ] **Step 1: Write the failing test** (mock the surface + query)
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { WizardPreviewChartV2Container } from "./WizardPreviewChartV2Container";

vi.mock("./WizardPreviewChartV2", () => ({ WizardPreviewChartV2: () => <div data-testid="wizard-v2" /> }));
vi.mock("@/components/scenario/useBarsFetchJob", () => ({
  useBarsFetchJob: () => ({ start: vi.fn(), canStart: true, statusText: null, outputText: null, errorText: null }),
}));

function wrap(ui: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

describe("WizardPreviewChartV2Container", () => {
  it("gates behind a Show preview chart button", () => {
    wrap(<WizardPreviewChartV2Container asset="ETH" from="2024-01-01" to="2024-02-01" granularity="1h" includeBaseline />);
    expect(screen.getByTestId("wizard-preview-show")).toHaveTextContent("Show preview chart");
    expect(screen.queryByTestId("wizard-v2")).not.toBeInTheDocument();
  });
  it("prompts to fill inputs when not ready", () => {
    wrap(<WizardPreviewChartV2Container asset="" from="" to="" granularity="1h" />);
    expect(screen.getByText("Fill asset + date range to see preview…")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run, verify fail** → FAIL.

- [ ] **Step 3: Implement the container** — port v1 `WizardPreviewChart.tsx` logic verbatim (DEBOUNCE_MS=350; `shown` gate reset on any prop change; `ready = !!asset && !!from && !!to`; query enabled `ready && shown` via `getScenarioPreview`; `useBarsFetchJob` spec when `ready && shown`), but replace the rendered `<ScenarioChart .../>` with the adapted v2 payload + surface, keeping the exact label strings and testids:
  - "Fill asset + date range to see preview…" when `!ready`
  - Button `data-testid="wizard-preview-show"` label "Show preview chart" when `!shown && ready`
  - "Loading preview…" while `query.isLoading`
  - "Preview failed: {message}" on error
  - Header: `Preview — {asset} · {from} → {to} · {granularity}` + `· Buy & Hold baseline ({n} pts)` when `includeBaseline && baseline_equity`
  - Button `data-testid="wizard-preview-hide"` label "Hide" when shown
  - Render `<WizardPreviewChartV2 payload={scenarioPreviewToWizardV2(query.data)} />`
  - Below it, the bars-fetch status block: render `barsFetch.statusText` / `errorText` (danger) / `outputText` (pre) exactly as v1 lines 208-225.

(Quote v1 `WizardPreviewChart.tsx` while implementing; it is the source of truth for the exact JSX/labels. Do not invent new copy.)

- [ ] **Step 4: Run, verify pass** → PASS.

- [ ] **Step 5: Export + commit**
```bash
# add to surfaces/index.ts: export * from "./WizardPreviewChartV2Container";
git add src/components/chart/v2/surfaces/WizardPreviewChartV2Container.tsx src/components/chart/v2/surfaces/WizardPreviewChartV2Container.test.tsx src/components/chart/v2/surfaces/index.ts
git commit -m "feat(chart-v2): WizardPreviewChartV2Container with preview gate + fetch job"
```

### Task 9: LiveChartV2 streaming + LiveChartV2Container (follow/freeze/resume); kill the stub

**Files:**
- Create: `src/components/chart/v2/surfaces/LiveChartV2Container.tsx`
- Create: `src/components/chart/v2/surfaces/LiveChartV2Container.test.tsx`
- Modify: `src/components/chart/v2/surfaces/LiveChartV2.tsx`

- [ ] **Step 1: Write the failing container test** (mock `useRunStream` + the v2 surface)
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LiveChartV2Container } from "./LiveChartV2Container";

const streamMock = vi.fn();
vi.mock("@/components/chart/use-run-stream", () => ({ useRunStream: (id: string) => streamMock(id) }));
vi.mock("./LiveChartV2", () => ({
  LiveChartV2: (p: { payload: { connection: string }; follow: boolean }) =>
    <div data-testid="live-v2" data-conn={p.payload.connection} data-follow={String(p.follow)} />,
}));

function runPayload() {
  return { run_id: "r1", asset: "BTC", granularity: "1h",
    bars: [{ time: 1, open: 1, high: 2, low: 0, close: 1.5, volume: 9 }],
    indicators: { bollinger: {upper:[],middle:[],lower:[]}, donchian:{upper:[],lower:[]}, macd:{line:[],signal:[],histogram:[]} },
    equity: [], drawdown: [], position: [], markers: { trades: [], vetoes: [], holds: [] } };
}

describe("LiveChartV2Container", () => {
  it("maps streaming status to connected", () => {
    streamMock.mockReturnValue({ data: runPayload(), status: "streaming" });
    render(<LiveChartV2Container runId="r1" />);
    expect(screen.getByTestId("live-v2")).toHaveAttribute("data-conn", "connected");
  });
  it("freezes when follow toggled off and resumes", () => {
    streamMock.mockReturnValue({ data: runPayload(), status: "streaming" });
    render(<LiveChartV2Container runId="r1" />);
    expect(screen.getByText("Following live")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByText("Frozen")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Resume live"));
    expect(screen.getByText("Following live")).toBeInTheDocument();
  });
  it("shows waiting state with no data", () => {
    streamMock.mockReturnValue({ data: undefined, status: "snapshot" });
    render(<LiveChartV2Container runId="r1" />);
    expect(screen.getByText("Waiting for first event…")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run, verify fail** → FAIL.

- [ ] **Step 3: Implement `LiveChartV2Container.tsx`**

Owns `useRunStream`, the follow/freeze/resume state (ported from v1 `LiveChart.tsx` lines 14-48), maps status→connection, adapts the run payload to a `LiveChartV2Payload`, and renders the follow chrome + `LiveChartV2`:
```tsx
import { useEffect, useRef, useState } from "react";
import { useRunStream } from "@/components/chart/use-run-stream";
import { runChartPayloadToV2 } from "../adapters/run-chart-payload";
import { LiveChartV2 } from "./LiveChartV2";
import type { LiveChartV2Payload } from "../types";

function connectionFor(status: string): LiveChartV2Payload["connection"] {
  if (status === "streaming") return "connected";
  if (status === "closed") return "offline";
  return "reconnecting"; // snapshot | reconnecting
}

export function LiveChartV2Container({ runId }: { runId: string }) {
  const { data, status } = useRunStream(runId);
  const [follow, setFollow] = useState(true);
  const prevRunId = useRef(runId);
  const effectiveFollow = prevRunId.current === runId ? follow : true;
  useEffect(() => {
    if (prevRunId.current === runId) return;
    prevRunId.current = runId;
    setFollow(true);
  }, [runId]);

  return (
    <div>
      <label className="flex items-center gap-2 mb-2 text-[12px] text-text-2">
        <input type="checkbox" checked={effectiveFollow} onChange={(e) => setFollow(e.target.checked)} />
        {effectiveFollow ? "Following live" : "Frozen"}
        {!effectiveFollow && (
          <button type="button" onClick={() => setFollow(true)} className="ml-2 underline">Resume live</button>
        )}
      </label>
      {data ? (
        <LiveChartV2 payload={toLivePayload(data, status)} follow={effectiveFollow} />
      ) : (
        <div className="text-text-3 py-12 text-center">Waiting for first event…</div>
      )}
    </div>
  );
}

function toLivePayload(data: Parameters<typeof runChartPayloadToV2>[0], status: string): LiveChartV2Payload {
  const v2 = runChartPayloadToV2(data);
  return {
    kind: "live", asset: v2.asset, granularity: v2.granularity,
    candles: v2.candles, equity: v2.equity, markers: v2.markers,
    live_index: Math.max(0, v2.candles.time.length - 1),
    connection: connectionFor(status), cache: "fresh",
  };
}
```

- [ ] **Step 4: Update `LiveChartV2.tsx` to remove the stub + accept follow + derive lastTick**

Remove the `useChart2Streaming` import/call. Add `follow?: boolean` to props. Derive `lastTickMs` from the newest candle timestamp. The payload's `candles` already update each render as the container re-adapts streamed data:
```tsx
type Props = { payload: LiveChartV2Payload; follow?: boolean };
export function LiveChartV2({ payload }: Props) {
  // remove: const { connection, lastTickMs } = useChart2Streaming({...});
  const t = payload.candles.time;
  const lastTickMs = t.length ? t[t.length - 1] * 1000 : null;
  // ...empty-state guard unchanged...
  // ConnectionStatus state={payload.connection} lastTickMs={lastTickMs}
}
```
(The `follow` prop is currently advisory for KlineCandlePane auto-scroll; if KlineCandlePane gains a `follow`/`scrollToRealTime` hook, thread it through — otherwise the connection chrome + live-updating payload satisfy parity. Document if follow is visual-only this wave.)

- [ ] **Step 5: Run, verify pass** → `npm test -- src/components/chart/v2/surfaces/LiveChartV2Container.test.tsx` → PASS. Confirm no remaining import of `useChart2Streaming` in `LiveChartV2.tsx`.

- [ ] **Step 6: Export + commit**
```bash
# add to surfaces/index.ts: export * from "./LiveChartV2Container";
git add src/components/chart/v2/surfaces/LiveChartV2Container.tsx src/components/chart/v2/surfaces/LiveChartV2Container.test.tsx src/components/chart/v2/surfaces/LiveChartV2.tsx src/components/chart/v2/surfaces/index.ts
git commit -m "feat(chart-v2): real LiveChartV2 streaming via useRunStream + follow/freeze/resume"
```

---

## Phase 4 — Route cutover

### Task 10: eval-runs.tsx → RunChartV2

**Files:** Modify `src/routes/eval-runs.tsx`.

- [ ] **Step 1: Swap imports** — replace
```ts
import { RunChart } from "@/components/chart/RunChart";
```
with
```ts
import { RunChartV2 } from "@/components/chart/v2/surfaces/RunChartV2";
import { runChartPayloadToV2 } from "@/components/chart/v2/adapters/run-chart-payload";
```
(`chartKeys, getRunChart` import stays.)

- [ ] **Step 2: Swap the JSX** (line ~469): `<RunChart payload={latestChart.data} />` → `<RunChartV2 payload={runChartPayloadToV2(latestChart.data)} />`.

- [ ] **Step 3: Typecheck + commit**
```bash
npm run typecheck
git add src/routes/eval-runs.tsx
git commit -m "feat(charts): eval-runs latest chart uses RunChartV2"
```

### Task 11: eval-runs-detail.tsx → RunChartV2

**Files:** Modify `src/routes/eval-runs-detail.tsx`.

- [ ] **Step 1: Swap imports** — replace `import { RunChart } from "@/components/chart/RunChart";` with the `RunChartV2` + `runChartPayloadToV2` imports (keep `chartKeys, getRunChart, openRunStream`).

- [ ] **Step 2: Swap the `chartNode` prop** (line ~283):
```tsx
chartNode={chart.data ? <RunChartV2 payload={runChartPayloadToV2(chart.data)} /> : null}
```

- [ ] **Step 3: Typecheck + commit**
```bash
npm run typecheck
git add src/routes/eval-runs-detail.tsx
git commit -m "feat(charts): eval-runs-detail uses RunChartV2"
```

### Task 12: scenarios-detail.tsx → ScenarioChartV2 + route fetch chrome

The fetch-bars affordance and cache status stay at the route (they already are — `useBarsFetchJob` + `BarsFetchJobStatus` are scenario-domain and unchanged). Only the chart component swaps; the v2 surface receives empty candles when bars are uncached and shows its own empty state, but to match v1's "No bars cached yet…" the route should keep showing the fetch chrome above the chart.

**Files:** Modify `src/routes/scenarios-detail.tsx`.

- [ ] **Step 1: Swap imports** — replace `import { ScenarioChart } from "@/components/chart/ScenarioChart";` with:
```ts
import { ScenarioChartV2 } from "@/components/chart/v2/surfaces/ScenarioChartV2";
import { scenarioChartPayloadToV2 } from "@/components/chart/v2/adapters/scenario-chart-payload";
import { CacheStatusBadge } from "@/components/scenario/CacheStatusBadge";
```
(Keep `getScenarioChart, scenarioChartKeys, useBarsFetchJob`.)

- [ ] **Step 2: Replace the `<ScenarioChart .../>` block** (lines ~496-503). v1 passed `onFetch/fetchStatus/fetchDisabled` into the chart; v2 doesn't take them, so render the fetch chrome at the route and the empty/loaded chart conditionally:
```tsx
{chart.data && (
  <div className="mb-5">
    <div className="flex items-center justify-between mb-2">
      <span className="text-text-3 text-[12px]">
        {chartAsset} · {chartGranularity}
      </span>
      <CacheStatusBadge
        status={chart.data.cache_status}
        onFetch={barsFetch.start}
        fetchStatus={barsFetch.statusText}
        disabled={!barsFetch.canStart}
      />
    </div>
    {chart.data.bars.length === 0 ? (
      <div className="flex items-center justify-center h-[360px] text-text-3 text-[13px] border border-border rounded">
        No bars cached yet. Use Fetch bars to populate this chart.
      </div>
    ) : (
      <ScenarioChartV2 payload={scenarioChartPayloadToV2(chart.data, chartAsset, chartGranularity)} />
    )}
    <BarsFetchJobStatus fetch={barsFetch} />
  </div>
)}
```
(Preserve the existing `Preview asset` + `Indicator timeframe` selectors and `BarsFetchJobStatus` exactly. Confirm the v1 `components/scenario/CacheStatusBadge` is the one with the fetch button — it is.)

- [ ] **Step 3: Update `scenarios-detail.test.tsx`** — it imports `ScenarioChartPayload` fixtures; ensure the test still constructs a `chart.data` with `bars` + `cache_status` and asserts the chart region renders. Retarget any assertion that referenced v1 `ScenarioChart` internals to the new route markup ("No bars cached yet…" empty state, or presence of the chart container).

- [ ] **Step 4: Typecheck + test + commit**
```bash
npm run typecheck && npm test -- src/routes/scenarios-detail.test.tsx
git add src/routes/scenarios-detail.tsx src/routes/scenarios-detail.test.tsx
git commit -m "feat(charts): scenarios-detail uses ScenarioChartV2 + route fetch chrome"
```

### Task 13: scenarios-new.tsx → WizardPreviewChartV2Container

**Files:** Modify `src/routes/scenarios-new.tsx`.

- [ ] **Step 1: Swap import + usage** — replace
```ts
import { WizardPreviewChart } from "@/components/chart/WizardPreviewChart";
```
with
```ts
import { WizardPreviewChartV2Container } from "@/components/chart/v2/surfaces/WizardPreviewChartV2Container";
```
and the JSX (line ~48) `<WizardPreviewChart ... />` → `<WizardPreviewChartV2Container asset="ETH" from={draft.from} to={draft.to} granularity={draft.granularity} includeBaseline />` (identical props).

- [ ] **Step 2: Typecheck + commit**
```bash
npm run typecheck
git add src/routes/scenarios-new.tsx
git commit -m "feat(charts): scenarios-new uses WizardPreviewChartV2Container"
```

### Task 14: live.tsx → LiveChartV2Container

**Files:** Modify `src/routes/live.tsx`.

- [ ] **Step 1: Swap import + usage** — replace `import { LiveChart } from "@/components/chart/LiveChart";` with `import { LiveChartV2Container } from "@/components/chart/v2/surfaces/LiveChartV2Container";` and `<LiveChart runId={id} />` → `<LiveChartV2Container runId={id} />`.

- [ ] **Step 2: Typecheck + commit**
```bash
npm run typecheck
git add src/routes/live.tsx
git commit -m "feat(charts): live route uses LiveChartV2Container"
```

### Task 15: Confirm all production v1 chart imports are gone

- [ ] **Step 1: Grep**

Run (repo root):
```bash
rg -n "components/chart/(RunChart|ScenarioChart|LiveChart|WizardPreviewChart|StrategyChart|CompareChart)\b" frontend/web/src --glob '!**/*.test.*' --glob '!frontend/web/src/components/chart/**'
```
Expected: **no matches** (only test files and the v1 components themselves may still reference each other). If any route/page still imports a v1 chart, fix it before Phase 5.

- [ ] **Step 2: Full typecheck + build smoke**
```bash
npm run typecheck && npm run build
```
Expected: PASS.

---

## Phase 5 — Full removal

### Task 16: Delete v1 renderers (grep-gated)

**Files:** Delete `src/components/chart/{RunChart,ScenarioChart,StrategyChart,CompareChart,LiveChart,WizardPreviewChart}.tsx` + their `.test.tsx`.

- [ ] **Step 1: Confirm zero non-test importers per component**
```bash
for c in RunChart ScenarioChart StrategyChart CompareChart LiveChart WizardPreviewChart; do
  echo "== $c =="; rg -n "chart/$c\"" frontend/web/src --glob '!**/$c.tsx' --glob '!**/$c.test.tsx';
done
```
Expected: empty (after Phase 4). `StrategyChart`/`CompareChart` were already test-only.

- [ ] **Step 2: Delete the files**
```bash
cd frontend/web
git rm src/components/chart/RunChart.tsx src/components/chart/RunChart.test.tsx \
  src/components/chart/ScenarioChart.tsx src/components/chart/ScenarioChart.test.tsx \
  src/components/chart/StrategyChart.tsx src/components/chart/StrategyChart.test.tsx \
  src/components/chart/CompareChart.tsx src/components/chart/CompareChart.test.tsx \
  src/components/chart/LiveChart.tsx src/components/chart/LiveChart.test.tsx \
  src/components/chart/WizardPreviewChart.tsx
```
(Add `WizardPreviewChart.test.tsx` to the `git rm` list if it exists.)

- [ ] **Step 3: Typecheck** — `npm run typecheck`. Fix any now-broken imports (should be none after Task 15). Expected: PASS.

- [ ] **Step 4: Commit**
```bash
git commit -m "chore(charts): delete v1 lightweight-charts renderer components"
```

### Task 17: Delete orphaned v1 support files

**Files:** Delete `src/components/chart/{chart-fit.ts,chart-theme.ts,ChartContainer.tsx,ChartLayersPanel.tsx,MarkerSidePanel.tsx,chart-layers.ts,use-chart-layers.ts}` + any `.test.*`.

- [ ] **Step 1: Confirm orphaned**
```bash
for f in chart-fit chart-theme ChartContainer ChartLayersPanel MarkerSidePanel chart-layers use-chart-layers; do
  echo "== $f =="; rg -n "chart/$f\"" frontend/web/src --glob '!frontend/web/src/components/chart/**';
done
```
Expected: empty (only intra-v1 references, now deleted). **Verify `use-run-stream.ts` is NOT in this list — it is kept.**

- [ ] **Step 2: Delete + typecheck + commit**
```bash
cd frontend/web
git rm src/components/chart/chart-fit.ts src/components/chart/chart-theme.ts \
  src/components/chart/ChartContainer.tsx src/components/chart/ChartLayersPanel.tsx \
  src/components/chart/MarkerSidePanel.tsx src/components/chart/chart-layers.ts \
  src/components/chart/use-chart-layers.ts
# include any matching .test files reported by step 1
npm run typecheck
git commit -m "chore(charts): delete orphaned v1 chart support files"
```

### Task 18: Delete the v2 streaming stub + barrel exports

**Files:** Delete `src/components/chart/v2/hooks/useChart2Streaming.ts` + `src/components/chart/v2/adapters/streaming.ts` (+ any `.test.*`). Modify `hooks/index.ts` and `adapters/index.ts`.

- [ ] **Step 1: Confirm no importers** (after Task 9 removed the LiveChartV2 call)
```bash
rg -n "useChart2Streaming|createStreamingBuffer|StreamingBuffer|adapters/streaming|hooks/useChart2Streaming" frontend/web/src
```
Expected: only the stub files + their barrel re-exports. If a fixture/test still imports them, update it first.

- [ ] **Step 2: Remove barrel exports** — in `hooks/index.ts` delete the `export { useChart2Streaming, type KLineDataLike, type Chart2StreamingResult, type UseChart2StreamingOpts } from "./useChart2Streaming";` block; in `adapters/index.ts` delete `export { createStreamingBuffer } from "./streaming";` and `export type { StreamingBuffer } from "./streaming";`.

- [ ] **Step 3: Delete + typecheck + commit**
```bash
cd frontend/web
git rm src/components/chart/v2/hooks/useChart2Streaming.ts src/components/chart/v2/adapters/streaming.ts
# include streaming.test.ts / useChart2Streaming.test.ts if present
npm run typecheck
git add src/components/chart/v2/hooks/index.ts src/components/chart/v2/adapters/index.ts
git commit -m "chore(chart-v2): remove M0 streaming stub + barrel exports"
```

### Task 19: Drop the `lightweight-charts` dependency

**Files:** Modify `frontend/web/package.json` (+ `package-lock.json` and/or `pnpm-lock.yaml` — whichever exist).

- [ ] **Step 1: Detect the package manager**
```bash
cd frontend/web && ls package-lock.json pnpm-lock.yaml yarn.lock 2>/dev/null
```

- [ ] **Step 2: Remove the dependency**
- If `package-lock.json`: `npm uninstall lightweight-charts`
- If `pnpm-lock.yaml`: `pnpm remove lightweight-charts`
- This updates `package.json` and the lockfile together.

- [ ] **Step 3: Confirm gone**
```bash
rg -n "lightweight-charts" frontend/web/src frontend/web/package.json frontend/web/package-lock.json frontend/web/pnpm-lock.yaml 2>/dev/null
```
Expected: **no output**.

- [ ] **Step 4: Commit**
```bash
git add frontend/web/package.json frontend/web/package-lock.json frontend/web/pnpm-lock.yaml 2>/dev/null
git commit -m "chore(charts): drop lightweight-charts dependency"
```

---

## Phase 6 — Final verification

### Task 20: Full verification + visual parity check

- [ ] **Step 1: Typecheck, test, build**
```bash
cd frontend/web
npm run typecheck && npm test && npm run build
```
Expected: all PASS.

- [ ] **Step 2: Confirm TradingView is fully gone**
```bash
rg -n "lightweight-charts|tradingview|createChart|addCandlestickSeries|IChartApi" frontend/web/src frontend/web/package.json
```
Expected: **no output**.

- [ ] **Step 3: Manual visual parity check** (run the app per the project `run`/`xvision-cli` skill, or `npm run dev`). On each migrated route confirm parity with the pre-migration behavior:
  - `/eval-runs` (latest run chart) and `/eval-runs/:id`: candles + SMA/EMA/Bollinger/Donchian overlays visible, buy/sell/veto/hold markers on the candle pane, position bands shaded, equity/drawdown/volume/oscillator panes, range buttons change the visible window (or are hidden if Kline range proved unreliable — Task 4 Step 8).
  - `/scenarios/:id`: indicator overlays render, bars data table toggles, "No bars cached yet…" shows when uncached, Fetch bars chrome + status works, no empty equity pane.
  - `/scenarios/new`: "Show preview chart" gate, debounce, "Hide", preview header with baseline point count, fetch-bars status/output/error.
  - `/live/:id`: live candle updates as the stream merges bars, "Following live" / "Frozen" / "Resume live", connection dot maps streaming→connected / reconnecting / closed→offline, follow resets on run-id change.

- [ ] **Step 4: Finish the branch** — use superpowers:finishing-a-development-branch to open the PR. PR body should list the parity gates closed (overlays, markers, position bands, range, scenario indicators+table, wizard gate+fetch, live streaming+follow) and the deletions (v1 stack + `lightweight-charts`).

---

## Self-review notes (author)

- **Spec coverage:** Parity gates 1-3 (overlays/markers/positions) → Tasks 1-3. Gate 4 (range) → Task 4. Gate 5 (scenario parity) → Tasks 5,7,12. Gate 6 (wizard) → Tasks 6,8,13. Gate 7 (live) → Task 9,14. Cleanup (lockfiles, barrels) → Tasks 18-19. All covered.
- **Type consistency:** `overlayLineDescriptors(indicators, theme, active)`, `scenarioChartPayloadToV2(payload, asset, granularity)`, `scenarioPreviewToWizardV2(payload)`, `LiveChartV2Container({runId})`, `LiveChartV2({payload, follow})`, `KlineCandlePane` new `overlayActive?` prop — names used consistently across tasks.
- **Known uncertainty (flagged in-task, resolved via TDD + visual check):** exact klinecharts v10 `OverlayFigure` `attrs`/`styles` shapes (Tasks 1-3) and KlineCharts range fidelity (Task 4) — both have explicit verify-or-fallback steps. The theme key names for overlay/position colors (`theme.overlay.*`, `theme.position.*`) must be confirmed against `useChart2Theme` during Task 1/3 implementation.

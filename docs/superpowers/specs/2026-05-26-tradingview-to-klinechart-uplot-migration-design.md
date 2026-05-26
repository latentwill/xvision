# Retire TradingView (lightweight-charts) → KlineChart + uPlot v2 surfaces

**Date:** 2026-05-26
**Status:** Design approved, spec review incorporated → implementation plan
**Scope:** `frontend/web/` only. No backend / Rust / API changes.

## Problem

The frontend still ships **two** chart stacks:

- **v1 (TradingView Lightweight Charts, `lightweight-charts ^4.1`)** — the
  original `RunChart` / `ScenarioChart` / `StrategyChart` / `CompareChart` /
  `LiveChart` / `WizardPreviewChart` components under
  `frontend/web/src/components/chart/`.
- **v2 (KlineChart + uPlot)** — the columnar-payload surfaces under
  `frontend/web/src/components/chart/v2/` delivered by the 2026-05-21 chart
  rework (`RunChartV2`, `ScenarioChartV2`, `StrategyChartV2`, `CompareChartV2`,
  `LiveChartV2`, `WizardPreviewChartV2`, plus the `/charts/*` dashboards).

The v2 rework completed Track B (the new `/charts/*` dashboards) and migrated
`home.tsx` and `authoring.tsx`, but the **eval/scenario/live production routes
were never cut over**. Five call sites still render v1 lightweight-charts:

| File | Line | v1 component |
|---|---|---|
| `routes/eval-runs.tsx` | 469 | `RunChart` |
| `routes/eval-runs-detail.tsx` | 283 | `RunChart` |
| `routes/scenarios-detail.tsx` | 496 | `ScenarioChart` |
| `routes/scenarios-new.tsx` | 48 | `WizardPreviewChart` |
| `routes/live.tsx` | 20 | `LiveChart` |

Goal: cut all five over to the v2 surfaces, then delete the v1 stack entirely
(components, support files, tests, and the `lightweight-charts` dependency).

### Spec-review amendments (2026-05-26)

The route cutover is not allowed to reduce chart behavior. The current v2 stack
is close enough for dashboard/lab use, but several Track-A parity pieces are
still incomplete and must be treated as first-class migration tasks:

- `KlineCandlePane` still has TODO-only wiring for candle overlays, trade/hold/
  veto markers, and position bands. Those must render in KlineCharts before v1
  deletion.
- Scenario charts must keep indicator overlays and the bars data table; the
  adapter cannot drop `payload.indicators`.
- Wizard preview must keep the explicit "show preview" gate, debounce/reset
  behavior, hide button, baseline label, fetch-bars job controls, and fetch job
  status/output.
- Live charts must keep follow/freeze/resume behavior, not just connection
  state.
- v2 range buttons must either actually change the visible data window on every
  migrated surface or be removed/disabled until they do.
- Cleanup must include both frontend lockfiles and v2 barrel exports for deleted
  streaming stubs.

## Approach

**Frontend adapter cutover. No backend changes.** Each route keeps its existing
row-based API call and pipes the payload through a `*PayloadToV2` adapter into
the matching v2 surface.

This is an **already-proven, in-repo pattern**, not a new idea:

```tsx
// routes/home.tsx (shipping today)
import { runChartPayloadToV2 } from "@/components/chart/v2/adapters/run-chart-payload";
<RunChartV2 payload={runChartPayloadToV2(chart)} showMarkerDock={false} />
```

`authoring.tsx` does the same with `StrategyHistoryChartV2` consuming the v1
`getStrategyChart` payload. We apply this recipe to the five remaining sites.

### Rejected alternative

New `/api/v2/charts/run/:id` columnar endpoints (the original 2026-05-21 spec's
A-M1 plan). Rejected: unnecessary backend churn and deploy risk when the
frontend adapters already exist and the underlying data is identical — only the
shape (row arrays → columnar parallel arrays) differs.

### Rollout & cleanup decisions (operator-confirmed 2026-05-26)

- **Hard cutover.** Direct swap, no `xvn.chartv2` cookie / feature flag. The
  `/charts/*` dashboards already dropped their cookie gate (PR #564), so this is
  consistent.
- **Full removal, gated on parity.** Delete v1 components + tests and drop
  `lightweight-charts` only after the parity checklist below is complete.
- **Live:** reuse the existing `useRunStream` SSE hook to feed v2, and
  implement the `LiveChartV2` streaming stub for real.

## Parity gates before deleting v1

These are implementation blockers, not nice-to-haves:

1. **Kline candle overlays.** `KlineCandlePane` must render SMA/EMA/Bollinger/
   Donchian overlays passed through `overlays`. Today those values are only held
   as extData placeholders.
2. **Kline markers.** `KlineCandlePane` must render buy/sell/veto/hold markers
   on the candle pane. `MarkerDock` is not sufficient by itself because v1
   rendered time/price-anchored markers on the chart.
3. **Position bands.** `KlineCandlePane` must render long/short position spans
   when `positions` is provided.
4. **Range controls.** `ChartFrame`'s `1d/1w/1m/3m/All` controls must drive the
   visible range for KlineCharts and uPlot panes, including multi-pane surfaces.
   If a surface cannot support range selection in this wave, hide the range
   control for that surface rather than shipping inert controls.
5. **Scenario parity.** Scenario v2 must render scenario indicators, cache/fetch
   status, bars-empty state, and a bars data table.
6. **Wizard parity.** Wizard v2 must keep the existing preview gate and
   fetch-bars affordance.
7. **Live parity.** Live v2 must keep follow/freeze/resume and reset follow when
   the run id changes.

## Per-site migration

### 1–2. RunChart → RunChartV2 (`eval-runs.tsx`, `eval-runs-detail.tsx`)

Adapter `runChartPayloadToV2` already exists and is used by `home.tsx`.

```tsx
// eval-runs-detail.tsx:283
chartNode={chart.data ? <RunChartV2 payload={runChartPayloadToV2(chart.data)} /> : null}
// eval-runs.tsx:469
<RunChartV2 payload={runChartPayloadToV2(latestChart.data)} />
```

The v1 `follow` prop (auto-scroll to latest) is non-streaming on these routes
(static run snapshots), so no behavior is lost.

Implementation note: this cutover depends on the Kline overlay/marker/position
parity gates above. `runChartPayloadToV2` already carries those arrays; the
renderer must actually paint them before these production routes switch.

### 3. ScenarioChart → ScenarioChartV2 (`scenarios-detail.tsx`)

New adapter `scenarioChartPayloadToV2(ScenarioChartPayload): ScenarioChartV2Payload`.
The v1 `ScenarioChartPayload` is `{ scenario, bars, indicators, cache_status }`.
Scenarios are a data source, not a backtest, so there is **no equity, markers,
or positions**. The adapter maps:

- `scenario.asset[0]?.symbol` / `preview_asset` / selected route asset → `asset`
- `scenario.granularity` → normalized `granularity`
- `bars → candles`
- `indicators → indicators` using the same `IndicatorMap` shape as
  `RunChartV2Payload`
- `equity/markers/positions = []`
- `cache_status` remains available to the route-level fetch/status chrome

`ScenarioChartV2Payload` must be extended with `indicators: IndicatorMap`.
`ScenarioChartV2` must expose the same overlay toggles as v1 where data exists
(`sma20/30/50/60/90/200`, `ema20/30/50/60/90/200`, Bollinger, Donchian). If
KlineCharts only supports the smaller current v2 overlay set at first, the
implementation must either add the missing keys or explicitly document the
temporary gap before v1 deletion. Do not ship a silent indicator drop.

The scenario v2 surface should not render an empty equity pane by default. Since
scenario payloads have no equity curve, hide/disable the equity layer for the
scenario surface or default it off for `useChart2Layers("scenario")`.

**Fetch-bars affordance:** v1 `ScenarioChart` accepted
`onFetch` / `fetchStatus` / `fetchDisabled` and rendered an inline "fetch bars"
control driven by `cache_status` (scenario bars may not be cached yet).
`ScenarioChartV2` is a pure renderer with no such prop. The control + status
**lift to the route** (`scenarios-detail.tsx`), shown when bars are
uncached/empty; ScenarioChartV2 renders once bars exist. Route-inline, no popup
(per the no-popup UI rule). The exact current UX (button label, status text,
disabled conditions) will be read from the current `scenarios-detail.tsx`
+ `ScenarioChart.tsx` and replicated faithfully during implementation.

**Data table:** v1 `ScenarioChart` exposes `ScenarioBarsTable` through
`ChartContainer`'s data-table toggle. `ScenarioChartV2` must add an equivalent
`DataTable` for at least the first/latest 200 bars before v1 removal.

**Header/context:** preserve the route-level preview asset and indicator
timeframe selectors already in `scenarios-detail.tsx`. Preserve the market /
quote label and cache-status context somewhere visible near the v2 chart; it may
be route chrome rather than inside `ScenarioChartV2`.

### 4. WizardPreviewChart → WizardPreviewChartV2 (`scenarios-new.tsx`)

v1 `WizardPreviewChart` is a *fetching wrapper*: it takes
`{ asset, from, to, granularity, includeBaseline }`, calls `getScenarioPreview`,
and renders v1 `ScenarioChart`. `WizardPreviewChartV2` is a pure renderer taking
a `WizardPreviewV2Payload`.

Replace the wrapper with a thin v2 fetching container (same prop surface so
`scenarios-new.tsx` changes only its import) that calls `getScenarioPreview`,
adapts via a new `scenarioPreviewToWizardV2(ScenarioPreviewPayload)`
(`bars → candles`, `baseline_equity ?? [] → equity`), and renders
`WizardPreviewChartV2`.

The new container must preserve the current wrapper behavior:

- debounce input changes before querying
- reset `shown=false` whenever `asset/from/to/granularity/includeBaseline`
  changes
- require the explicit "Show preview chart" button before fetching
- keep the "Hide" button
- keep the preview header with asset, date range, granularity, and buy-and-hold
  baseline point count when available
- keep `useBarsFetchJob` for uncached preview bars, including status text,
  output text, error text, and disabled/start conditions

Do not move fetch-bars into a modal or popup. Keep it inline with the preview
chart, mirroring the current UX.

### 5. LiveChart → LiveChartV2 (`live.tsx`) — and kill the stub

v1 `LiveChart` does `useRunStream(runId)` → `<RunChart payload follow />` + a
status badge. `useRunStream` (in `components/chart/use-run-stream.ts`) is solid:
SSE snapshot load, `bar`/`equity`/`marker`/`indicator_tail` merge, reconnect
with backoff, terminal-status close, structured tracing. **Keep it unchanged.**

`LiveChartV2` currently calls the no-op `useChart2Streaming` stub
(`hooks/useChart2Streaming.ts` + `adapters/streaming.ts`) and renders a static
payload. Replace this:

- New `LiveChartV2Container` (route-level) owns `useRunStream(runId)`, adapts
  each updated `RunChartPayload` via `runChartPayloadToV2`, builds the
  live-updating `LiveChartV2Payload` (or feeds `RunChartV2` directly — decided in
  plan; `LiveChartV2` preferred to keep the live chrome), and passes a
  `connection` derived from stream `status`:
  `streaming → connected`, `snapshot/reconnecting → reconnecting`,
  `closed → offline`.
- `LiveChartV2` renders straight from the live-updating payload + the passed-in
  connection state. The `useChart2Streaming` call is removed.

`live.tsx:20` becomes `<LiveChartV2Container runId={id} />`.

The container/surface must preserve the v1 live controls:

- `Following live` checkbox
- `Frozen` state when follow is off
- `Resume live` button
- automatic reset to follow=true when `runId` changes

Implementation detail: `LiveChartV2` currently expects `lastTickMs` from
`useChart2Streaming`. After deleting that stub, derive last tick from the latest
stream data update or from the newest candle/equity timestamp and pass it
explicitly into `ConnectionStatus`.

The orphaned stub files (`hooks/useChart2Streaming.ts`, `adapters/streaming.ts`,
`createStreamingBuffer`) are deleted once no importer remains (verify any v2
fixture/test references first).

## Already migrated (untouched)

- `home.tsx` — `RunChartV2` via `runChartPayloadToV2`.
- `authoring.tsx` — `StrategyHistoryChartV2` via `getStrategyChart`.

## New adapters (mirroring `run-chart-payload.ts` style)

- `components/chart/v2/adapters/scenario-chart-payload.ts` —
  `scenarioChartPayloadToV2`.
- `components/chart/v2/adapters/scenario-preview-payload.ts` —
  `scenarioPreviewToWizardV2`.

Each gets a unit test alongside, following `run-chart-payload`'s existing test.

## Full removal (final phase, after all sites green)

Delete only after a grep confirms zero remaining importers per file:

**v1 renderers** (`components/chart/`):
`RunChart.tsx`, `ScenarioChart.tsx`, `StrategyChart.tsx`, `CompareChart.tsx`,
`LiveChart.tsx`, `WizardPreviewChart.tsx`.

**v1 support** (orphaned once renderers are gone — confirmed imported only by
the v1 renderers, not by `v2/`):
`chart-fit.ts`, `chart-theme.ts`, `ChartContainer.tsx`, `ChartLayersPanel.tsx`,
`MarkerSidePanel.tsx`, `chart-layers.ts`, `use-chart-layers.ts`.

**Tests:** the matching `*.test.tsx` for all of the above, plus
`routes/scenarios-detail.test.tsx`'s v1-payload fixtures (retarget to v2).

**v2 stub:** `hooks/useChart2Streaming.ts`, `adapters/streaming.ts`.
Also remove their barrel exports from `components/chart/v2/hooks/index.ts` and
`components/chart/v2/adapters/index.ts`.

**Kept:** `components/chart/use-run-stream.ts` (now consumed by the v2 live
container).

**Dependency:** remove `"lightweight-charts": "^4.1"` from
`frontend/web/package.json`, `frontend/web/package-lock.json`, and
`frontend/web/pnpm-lock.yaml`.

`StrategyChart` and `CompareChart` have **no production callers** (test files
only) — safe deletes, but grep-confirm before removing.

## Testing & verification

- New adapter unit tests: `scenario-chart-payload`, `scenario-preview-payload`.
- Kline parity tests for overlays, markers, and position bands. Unit tests
  should assert the KlineCharts APIs are called with the converted structures;
  at least one browser/manual visual check should confirm they are visible in
  the rendered pane.
- Range-control tests for the migrated surfaces, or explicit assertions that
  range controls are hidden where unsupported.
- `LiveChartV2Container` test mocking `useRunStream` (asserts adapted payload +
  connection mapping; covers reconnect/closed states, last tick, follow/freeze,
  resume, and run-id reset).
- Scenario route/surface tests covering fetch-bars chrome, empty bars state,
  indicator adapter output, visible data table, and no empty equity pane.
- Wizard preview tests covering the show/hide gate, debounce/reset, baseline
  label, fetch-bars job start/disabled/status/output/error, and adapted v2
  payload.
- Update/retarget route + surface tests that referenced v1 components.
- Verify: `npm run typecheck`, `npm test -- <targeted chart/route suites>`, and
  `npm run build` in `frontend/web`. There is currently no `lint` script in
  `frontend/web/package.json`; do not list lint unless a script is added.
- Confirm `rg "lightweight-charts" frontend/web/src frontend/web/package.json
  frontend/web/package-lock.json frontend/web/pnpm-lock.yaml` returns nothing.
  **Frontend-only — no cargo.**

## Risks

1. **Live regression (highest).** Mitigated: the SSE engine (`useRunStream`) is
   reused verbatim; only the render layer changes. Follow/freeze/resume is
   covered by tests before cutover.
2. **Scenario fetch UX.** Must replicate the `cache_status`-driven fetch trigger
   at route level. Mitigated: read and mirror the current UX before editing.
3. **Chart parity regression.** Kline overlays, markers, position bands, and
   range controls are currently the highest-risk v2 gaps. Mitigated: implement
   and visually verify these before deleting v1.
4. **Hidden v1 importers.** Mitigated: every delete is gated on a zero-importer
   grep; `tsc` + build catch stragglers.

## Out of scope

- Backend chart endpoints / new columnar APIs.
- The `/charts/*` dashboards and `/chart-lab` (already v2).
- Any new chart features beyond parity with the v1 surfaces being replaced.

# Retire TradingView (lightweight-charts) → KlineChart + uPlot v2 surfaces

**Date:** 2026-05-26
**Status:** Design approved, pending spec review → implementation plan
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
- **Full removal.** Delete v1 components + tests and drop `lightweight-charts`.
- **Live:** reuse the existing `useRunStream` SSE hook to feed v2, and
  implement the `LiveChartV2` streaming stub for real.

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

### 3. ScenarioChart → ScenarioChartV2 (`scenarios-detail.tsx`)

New adapter `scenarioChartPayloadToV2(ScenarioChartPayload): ScenarioChartV2Payload`.
The v1 `ScenarioChartPayload` is `{ scenario, bars, indicators, cache_status }` —
**no equity, markers, or positions** (scenarios are a data source, not a
backtest). The adapter maps `bars → candles` and defaults
`equity/markers/positions = []`. ScenarioChartV2's volume pane derives from
candles; its equity pane is empty (layer toggle).

**Fetch-bars affordance:** v1 `ScenarioChart` accepted
`onFetch` / `fetchStatus` / `fetchDisabled` and rendered an inline "fetch bars"
control driven by `cache_status` (scenario bars may not be cached yet).
`ScenarioChartV2` is a pure renderer with no such prop. The control + status
**lift to the route** (`scenarios-detail.tsx`), shown when bars are
uncached/empty; ScenarioChartV2 renders once bars exist. Route-inline, no popup
(per the no-popup UI rule). The exact current UX (button label, status text,
disabled conditions) will be read from the current `scenarios-detail.tsx`
+ `ScenarioChart.tsx` and replicated faithfully during implementation.

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
`MarkerSidePanel.tsx`, `use-chart-layers.ts`.

**Tests:** the matching `*.test.tsx` for all of the above, plus
`routes/scenarios-detail.test.tsx`'s v1-payload fixtures (retarget to v2).

**v2 stub:** `hooks/useChart2Streaming.ts`, `adapters/streaming.ts`.

**Kept:** `components/chart/use-run-stream.ts` (now consumed by the v2 live
container).

**Dependency:** remove `"lightweight-charts": "^4.1"` from
`frontend/web/package.json`.

`StrategyChart` and `CompareChart` have **no production callers** (test files
only) — safe deletes, but grep-confirm before removing.

## Testing & verification

- New adapter unit tests: `scenario-chart-payload`, `scenario-preview-payload`.
- `LiveChartV2Container` test mocking `useRunStream` (asserts adapted payload +
  connection mapping; covers reconnect/closed states).
- Update/retarget route + surface tests that referenced v1 components.
- Verify: `tsc` typecheck + `vitest` run + lint + `vite build`. Confirm
  `grep -r lightweight-charts frontend/web/src` returns nothing and the dep is
  gone from `package.json`. **Frontend-only — no cargo.**

## Risks

1. **Live regression (highest).** Mitigated: the SSE engine (`useRunStream`) is
   reused verbatim; only the render layer changes.
2. **Scenario fetch UX.** Must replicate the `cache_status`-driven fetch trigger
   at route level. Mitigated: read and mirror the current UX before editing.
3. **Hidden v1 importers.** Mitigated: every delete is gated on a zero-importer
   grep; `tsc` + build catch stragglers.

## Out of scope

- Backend chart endpoints / new columnar APIs.
- The `/charts/*` dashboards and `/chart-lab` (already v2).
- Any new chart features beyond parity with the v1 surfaces being replaced.

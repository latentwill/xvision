# Pulse chart: fast load + view switcher — design

Date: 2026-06-11
Status: approved (operator, 2026-06-11)
Surfaces: dashboard home Pulse band (`frontend/web/src/components/home/`),
run chart API (`crates/xvision-engine/src/api/chart.rs`,
`crates/xvision-dashboard/src/routes/eval_runs.rs`)

## Problem

1. **Slow hero chart.** The dashboard home hero chart fetches
   `GET /api/eval/runs/:id/chart`, which returns up to 100K OHLCV bars,
   ~20 indicator series (SMA/EMA ×6, Bollinger, Donchian, RSI, MACD, ATR),
   trade markers, and position data — often multi-MB, with indicators
   recomputed server-side per request. `PulseEquityChart` uses only the
   `equity` array. There is also a request waterfall: runs list → pick hero
   → chart fetch.
2. **"Two earnings lines."** `PulseEquityChart` strokes a second
   "Drawdown" series (`equity − running max`, ≤ 0). Below the running peak
   this is the equity line shifted down — identical wiggles — so it reads
   as a duplicate earnings curve rather than a drawdown band.
3. **Underused custom charting.** The hero shows one static view while the
   backend already has a compare-chart endpoint and the chart-v2 stack has
   candle panes, marker plugins, and gradient/glow treatments going unused
   on the home page.

## Decisions (operator-confirmed 2026-06-11)

- Drawdown on the hero: **band only, no stroke**.
- View set: **Return %** (default), **Price + trades**, **vs Buy & Hold**,
  **Drawdown**, **All runs**.
- Perf scope: **slim payload via `include` param + prefetch**. No
  server-side downsampling in this pass.

## Design

### 1. Slim chart payload (`include` query param)

Extend `GET /api/eval/runs/:id/chart` with an optional `include` param — a
comma-separated set of payload sections:

| `include` | Returns | Server work skipped |
|---|---|---|
| `equity` | equity curve + run metadata | bar loading, indicator computation, markers |
| `equity,baseline` | + `baseline_equity` (buy-and-hold) | bars are loaded internally to compute the baseline but are NOT shipped; indicators skipped |
| `bars,markers` | OHLCV bars + trade/veto/hold markers | indicator computation |
| absent | full payload (today's behavior) | nothing — backward compatible |

Baseline semantics: buy-and-hold of the run's resolved asset over the run's
time window, $100k initial capital (same convention as scenario preview's
`baseline_equity`), sampled at the equity curve's timestamps so the two
series align on one time axis. Shipped as `baseline_equity:
Option<Vec<ChartEquityPoint>>` on `RunChartPayload` (None unless requested).

Unrequested sections are returned as empty vectors (not nulls) to keep the
existing `RunChartPayload` shape and generated TS types stable; the new
`baseline_equity` field is the only schema addition.

### 2. Hero drawdown becomes a band

`PulseEquityChart` keeps the drawdown column feeding the `xvnAreaFill`
plugin (red underwater tint) but the series stroke is removed. One earnings
line; shading shows drawdown depth. `pulse.ts` selectors unchanged.

### 3. Pulse view switcher

A chip row inline in the Pulse band header (no popups; existing chip
styling):

`[ Return % ] [ Price + trades ] [ vs Buy & Hold ] [ Drawdown ] [ All runs ]`

- Same 210px chart slot and KPI rail; only the canvas swaps.
- Selected view persists to `localStorage` key `xvn:pulse-view`.
- Per-view data loads lazily (TanStack Query `enabled` on selection);
  query keys include the `include` set so payload variants cache
  independently.

| View | Data | Rendering |
|---|---|---|
| Return % (default) | `include=equity` (prefetched) | existing chart, band-only drawdown |
| Price + trades | `include=bars,markers`, lazy | candle pane + buy/sell markers reusing chart-v2 `KlineCandlePane` + marker plugins |
| vs Buy & Hold | `include=equity,baseline`, lazy | strategy return % (gold) vs hold return % (muted), zero line, inline labels |
| Drawdown | `include=equity` (cache hit) | dedicated underwater area chart, client-computed |
| All runs | existing `GET /api/eval/runs/compare/chart?ids=…`, last ≤10 completed chartable runs, lazy | faint return-% lines normalized to elapsed fraction of each run; hero run highlighted with gradient/glow |

"All runs" normalization is client-side: each run's equity → return %,
time → elapsed fraction (0..1) of that run's own window, so runs over
different scenarios/windows overlay meaningfully.

### 4. Prefetch

The home route calls `queryClient.prefetchQuery` for the hero run's slim
chart as soon as the runs list resolves, so the chart fetch starts before
`PulseBand` renders. The runs-list → chart waterfall remains structurally
(the hero id comes from the list) but the second hop becomes a few KB with
no server compute.

## Components

- `features/home/pulse.ts`: new pure selectors — field-view normalization
  (return % + elapsed fraction), baseline series mapping, view-type
  constants. Unit-tested like existing selectors.
- `components/home/PulseViewSwitcher.tsx`: chip row + persistence.
- `components/home/views/`: one small component per view, each reusing
  chart-v2 primitives (`usePlot`, `KlineCandlePane`, theme adapters,
  xvn plugins).
- `api/chart.ts`: `getRunChart(runId, include?)` + query keys carrying the
  include set.
- Rust: `include` parsing + conditional payload assembly in
  `build_run_payload`; buy-and-hold baseline computation.

## Error handling

- A failed lazy fetch renders a retry affordance inside the chart slot;
  the band and KPI rail never break.
- Views with no data (e.g. live runs without bars, missing markers) show
  the existing "no samples recorded" empty card, per view.
- Unknown `include` tokens are ignored server-side (forward compatible);
  an `include` with no recognized tokens behaves as `equity`.

## Testing

- Rust: unit tests for include filtering (equity-only performs no bar
  fetch / indicator compute — assert via payload emptiness and a
  test-visible work counter or by stubbing the bar source), baseline
  correctness against a known bar fixture, alignment of baseline to
  equity timestamps.
- Vitest: selector tests (normalization, baseline mapping, view
  persistence), per-view component render tests with mocked queries,
  switcher interaction (chip click → lazy query fires → canvas swaps).
- Coverage per `.coverage-thresholds.json`.

## Out of scope

- Server-side downsampling (LTTB) and cache headers (deferred; revisit if
  slim payloads are still slow on large runs).
- Run-detail page (`RunChartV2`) changes — it keeps the full payload.
- Mobile-specific chart layouts beyond what the existing responsive band
  provides.

# TradingView Charts — Design

> **Status:** Design / spec — accepted, ready for implementation planning. Drafted 2026-05-11.
> **Author:** xvision team.
> **Companion specs:** [Custom-Scenario Eval](./2026-05-11-custom-scenario-eval-design.md) (hard dependency for bar data) · [Plan 2d — Dashboard + Wizard](../plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md) (the original locked-in chart-library decision lives here) · [Eval Engine Design](./2026-05-08-eval-engine-design.md) (original chart-decision context).
> **Tracking:** F32 (this spec) + F33 (Advanced Charts upgrade follow-up). Supersedes the original Plan-2d CDN-loaded approach (the dashboard is now Vite-bundled — npm install replaces CDN script tag).

---

## 1. Purpose

The eval engine renders charts today as 30-line inline-SVG sparklines (`frontend/web/src/routes/eval-runs-detail.tsx:221` and `eval-compare.tsx:194`). No interactivity, no zoom, no axes, no trade markers, no candlesticks. The chart-library decision was made eight months ago (Plan 2d, May 2026): **TradingView Lightweight Charts (Apache 2.0)**. Implementation was deferred. This spec moves it forward.

The work pairs naturally with the custom-scenario eval spec — that spec adds the bar cache + scenario registry; this spec adds the chart surface that makes the bar/decision/equity data legible. Together they close the "operator can't actually see what their strategy did" gap.

**"Trader need"** framing: charts everywhere a trader expects them. Six surfaces ship with charting in v1: run detail, compare, scenario detail, strategy detail, live cockpit, wizard preview.

---

## 2. Locked decisions

| # | Decision |
|---|---|
| 1 | **TradingView Lightweight Charts via npm** (`lightweight-charts@4.x`), not CDN. The Vite-bundled SPA gets proper TS types + tree-shaking. Apache 2.0; ~50 KB gzipped. |
| 2 | **Six chart surfaces** ship in v1: run detail, compare, scenario detail, strategy detail, live cockpit, wizard preview. "Lots of charts" — explicit trader-tool framing. |
| 3 | **Kitchen-sink indicator set, server-computed.** All indicators (`SMA20/50/200`, `EMA20/50/200`, `Bollinger(20,2)`, `Donchian(20)`, `RSI(14)`, `MACD(12,26,9)`, `ATR(14)`) computed via existing `xvision-data::indicators` functions. Client toggles visibility of already-shipped series. |
| 4 | **Server=client** (in-process Axum + embedded SPA). Full payloads in one shot; no chunking, no `?indicators=` gating, no payload-size concerns. |
| 5 | **Same indicator math for agents and humans.** The chart endpoint uses the exact `xvision-data::indicators` functions that the `xvn-mcp` server exposes to LLM agents. Property test enforces parity. |
| 6 | **Layer toggle prefs persist in localStorage**, per-user, per-surface. No server-side persistence in v1. |
| 7 | **Live cockpit streams via SSE.** Initial fetch via `GET /api/eval/runs/:id/chart` returns the snapshot; if the run is `running`, an SSE connection to `/stream` appends events. 250 ms server-side batching. |
| 8 | **Bar-count cap of 100K per chart payload** enforced at the API layer (returns `ApiError::Validation` with a downsample hint). v1 doesn't ship a downsampler; longer/finer scenarios surface a clear error. |
| 9 | **CompareChart caps at 10 overlaid runs** for legibility. > 10 → "narrow your filter" message. |
| 10 | **No backward-compat shim for the existing SVG charts.** They're deleted in the same PR that introduces `<RunChart>` / `<CompareChart>`. |

---

## 3. In scope / out of scope

### 3.1 In scope (v1)

- `lightweight-charts` npm dependency.
- `frontend/web/src/components/chart/` directory: `ChartContainer`, `RunChart`, `CompareChart`, `ScenarioChart`, `StrategyChart`, `LiveChart`, `chart-layers.ts`, `chart-theme.ts`.
- New Rust module `crates/xvision-engine/src/api/chart.rs` with payload builders + ts-rs-generated frontend types.
- New HTTP endpoints: `GET /api/eval/runs/:id/chart`, `GET /api/eval/runs/compare/chart`, `GET /api/scenarios/:id/chart`, `GET /api/strategies/:id/chart`, `GET /api/eval/runs/:id/stream` (SSE), `GET /api/scenarios/preview` (transient — no row yet).
- Replacement of `EquityChart` (`eval-runs-detail.tsx:221`) and `EquityOverlay` (`eval-compare.tsx:194`) with the new components.
- Embed charts in scenario detail / strategy detail / live cockpit / wizard preview routes.
- Multi-pane stack: price + indicator subpane + equity + drawdown + volume.
- Layer toggle panel + localStorage persistence.
- All trade / veto / hold markers with side-panel click expansion.
- Position-size band overlay.
- Range buttons (1d / 1w / 1m / 3m / All).
- Crosshair-synced multi-pane tooltips.
- Accessibility: `role="img"` + descriptive `aria-label` + parallel "Data table" view.
- CI build-size + paint-time budgets.

### 3.2 Out of scope (deferred)

- **TradingView Advanced Charts** (Pine-script studies, drawing tools, multi-chart layouts). Requires application/license. Tracked as F33.
- **Parameter customization** for indicators (e.g. SMA(34) instead of SMA(50)). v1 hard-codes the parameter set.
- **Real-strategy wizard preview** — wizard preview's hypothetical equity overlay uses a baseline arm (`Buy and Hold`) in v1; running a deterministic preview of the actual strategy is a follow-up.
- **Chart downsampling** for very-long-window scenarios (>100K bars). v1 errors instead.
- **Compare view > 10 runs.**
- **Server-side chart preference sync.** localStorage only in v1.
- **Annotation drawing tools** (manual trend lines, fib drags). Lightweight Charts doesn't ship these; Advanced Charts does.
- **Pine-script / custom indicator definitions.** Advanced Charts territory.

---

## 4. Architecture

### 4.1 Surface map

| Surface | Chart shape | Source endpoint |
|---|---|---|
| `/eval-runs/:id` | Multi-pane: price candles + indicators / equity / drawdown / volume; all markers on price pane. | `/api/eval/runs/:id/chart` |
| `/eval-compare` | Single pane: N equity curves overlaid, color-coded; optional price backdrop. | `/api/eval/runs/compare/chart` |
| `/scenarios/:id` | Single pane: price candles only (preview of the underlying window). | `/api/scenarios/:id/chart` |
| `/strategies/:id` | Single pane: N equity curves (one per past run), color-coded by scenario. | `/api/strategies/:id/chart` |
| `/live/<deployment_id>` | Same as run detail, streaming. | `/api/eval/runs/:id/stream` |
| Wizard preview | Inline thumbnail in `/scenarios/new`: scenario chart + optional baseline-arm overlay. | `/api/scenarios/preview` |

### 4.2 Crate boundaries

- **New code:**
  - `crates/xvision-engine/src/api/chart.rs` (payload builders + endpoints).
  - `frontend/web/src/components/chart/` (all chart components + layer registry + theme tokens).
- **Touched code:**
  - `frontend/web/src/routes/eval-runs-detail.tsx` (replace SVG sparkline with `<RunChart>`).
  - `frontend/web/src/routes/eval-compare.tsx` (replace SVG overlay with `<CompareChart>`).
  - `frontend/web/src/routes/scenarios/$id.tsx` (new — created by custom-scenario M3; add `<ScenarioChart>`).
  - `frontend/web/src/routes/strategies.tsx` / `authoring.tsx` (embed `<StrategyChart>`).
  - `frontend/web/src/routes.tsx` (add `/live/:id` route).
  - `frontend/web/package.json` (`+lightweight-charts@^4.1`).
  - `crates/xvision-engine/src/api/mod.rs` (route registration).
- **Existing endpoints unchanged:** `/api/eval/runs/:id`, `/api/eval/runs/compare`, `/api/scenarios/:id`. The chart payloads live at parallel `*/chart` paths so the detail-page metadata fetch stays cheap.

### 4.3 Milestones

| M | Ships | Depends on |
|---|---|---|
| **M1 — Replace existing SVG** | `lightweight-charts` npm dep, `ChartContainer` + `RunChart` + `CompareChart`, `/api/eval/runs/:id/chart` + extended compare endpoint, delete the SVG sparkline + overlay. | Custom-scenario M1 (bars cache). |
| **M2 — Scenario + Strategy charts** | `ScenarioChart` + `StrategyChart`, `/api/scenarios/:id/chart` + `/api/strategies/:id/chart`, embed in detail routes. | Custom-scenario M2 (scenarios table). |
| **M3 — Live cockpit + wizard preview** | `LiveChart` with SSE wiring, `/live/<deployment_id>` route, `/api/eval/runs/:id/stream` SSE endpoint, wizard preview chart, `/api/scenarios/preview` endpoint. | Custom-scenario M3 (wizard route) + Plan 2c (live deployment model) if not yet landed. |

---

## 5. Chart panel anatomy

### 5.1 Run-detail / live-cockpit multi-pane stack

```
┌────────────────────────────────────────────────────────────────────────┐
│ Time range:  [1d] [1w] [1m] [3m] [All]              Theme: ● dark ○ light │
│                                            Layers ▼  Export ⤓  Replay ▶  │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   PRICE PANE (60% height)                                              │
│      • candles (OHLC)                                                  │
│      • SMA20 / SMA50 / SMA200  (default on)                            │
│      • EMA20 / EMA50 / EMA200  (toggle)                                │
│      • Bollinger bands (toggle)                                        │
│      • Donchian channels (toggle)                                      │
│      • Fib retracements (toggle)                                       │
│      • Buy markers (▲ green, sized by order_size)                      │
│      • Sell markers (▼ red, sized by order_size)                       │
│      • Risk-veto markers (✕ yellow)                                    │
│      • Hold markers (· gray, default off)                              │
│      • Position-size band (translucent green/red)                      │
├────────────────────────────────────────────────────────────────────────┤
│   INDICATOR SUBPANE (15% — tabbed: RSI / MACD / ATR)                   │
├────────────────────────────────────────────────────────────────────────┤
│   EQUITY PANE (15%)                                                    │
│      • area chart, baseline = initial capital                          │
│      • drawdown shading (red below previous peak)                      │
├────────────────────────────────────────────────────────────────────────┤
│   VOLUME PANE (10%, toggle in Layers)                                  │
└────────────────────────────────────────────────────────────────────────┘
   ◄── Crosshair synced across all panes ──►
```

### 5.2 Layer toggle panel (slide-out)

```
┌─ PRICE PANE ──────────────┐
│ ☑ Candles                 │
│ ☑ SMA20  SMA50  SMA200    │
│ ☐ EMA20  EMA50  EMA200    │
│ ☐ Bollinger(20, 2)        │
│ ☐ Donchian(20)            │
│ ☐ Fib retracements        │
│ ☑ Buy markers             │
│ ☑ Sell markers            │
│ ☑ Risk-veto markers       │
│ ☐ Hold markers            │
│ ☑ Position-size band      │
├─ INDICATOR SUBPANE ───────┤
│ ◉ RSI(14)                 │
│ ○ MACD                    │
│ ○ ATR                     │
│ ○ (hidden)                │
├─ EQUITY PANE ─────────────┤
│ ☑ Equity                  │
│ ☑ Drawdown shading        │
├─ VOLUME ──────────────────┤
│ ☐ Volume histogram        │
└───────────────────────────┘
```

### 5.3 Interactions (free with Lightweight Charts)

- Drag to pan, scroll to zoom.
- Crosshair synchronized across panes.
- Hover tooltip: bar OHLC + indicator values + equity + open position.
- Range buttons snap to preset windows.
- Click a buy/sell marker → side panel with the full decision row (justification, conviction, fees, pnl).
- Live mode: right-edge auto-scroll follow; pan-to-freeze; "Resume live" button to re-engage.

### 5.4 Layer pref persistence

`localStorage` key: `xvision.chart.layers.<surface>` (e.g. `xvision.chart.layers.run-detail`). Value is the JSON of the toggle state. Per-user, per-surface. No server-side sync in v1.

---

## 6. Per-surface specs

### 6.1 Run detail (`/eval-runs/:id`)

Full multi-pane chart from §5.1. Renders directly below the `RunSummary` block. Decision-row table below the chart stays — clicking a row scrolls the chart to that bar + flashes the marker.

Default layers: Candles · SMA20/50/200 · Buy/Sell/Veto markers · Position bands · RSI(14) subpane · Equity + Drawdown · Volume off.

### 6.2 Compare (`/eval-compare`)

Multi-run equity overlay. Single pane.

- N equity curves overlaid, color-coded per existing run-color assignment; legend at top.
- X-axis: **trade-time elapsed** (`t=0` at run start) by default. Toggle to wall-clock if all runs share a scenario.
- Optional price-backdrop toggle: if all selected runs share a scenario, fade in scenario candles behind the equity overlay. Disabled if runs span scenarios.
- Hover tooltip: per-run equity at crosshair + delta vs leader.
- Click a curve: highlight (others fade), open side panel with run summary + "Open run detail" CTA.
- Cap: 10 runs. > 10 → "narrow your filter" inline message.

Keeps the existing "normalized shape" toggle from `eval-compare.tsx`.

### 6.3 Scenario detail (`/scenarios/:id`)

Single-pane price preview on a "Preview" tab.

- Candles only, full window, default granularity.
- Volume histogram subpane (toggle).
- No indicators on by default.
- Range buttons.
- Background-tinted regime regions if scenario tags include `regime:bull`/`regime:bear`/`regime:chop`/`regime:event`.
- Cache-status badge: green if all bars cached, yellow with "Fetch bars" CTA if not.

### 6.4 Strategy detail (`/strategies/:id`)

Single-pane multi-run equity overlay.

- One line per past run, color-coded by scenario.
- Legend: scenarios with run-count ("Bull Q1 2025 (4 runs)", "Bear Q3 2024 (2 runs)").
- X-axis: trade-time elapsed.
- Hover: per-run tooltip with run-id, final PnL, drawdown, Sharpe.
- Empty state: "This strategy has no completed runs yet. Launch one from `/eval-runs`."

### 6.5 Live cockpit (`/live/<deployment_id>`)

Run-detail shape, streaming.

- All run-detail layers.
- SSE pushes new bars / new markers / new equity points / status changes.
- Right-edge follow mode by default; pan-left freezes; "Resume live" re-engages.
- Connection status: green (streaming), yellow (reconnecting), red (stale).
- Server-side 250 ms batch tick.

### 6.6 Wizard preview (inline in `/scenarios/new`)

~200px tall inline chart, single pane.

- Updates as operator picks asset / date range / granularity.
- Calls `GET /api/scenarios/preview` (no scenario row exists yet).
- Renders candles + volume.
- Optional baseline-arm equity overlay (Buy & Hold, deterministic) for "what would a passive strategy look like on this window."
- Cache-status indicator: "this window is cached" (green) or "will fetch on first run" (yellow).

---

## 7. Data API + payload shape

### 7.1 Rust payload types (`crates/xvision-engine/src/api/chart.rs`)

```rust
#[derive(Serialize, ts_rs::TS)]
#[ts(export)]
pub struct RunChartPayload {
    pub run_id: String,
    pub scenario_id: String,
    pub asset: String,
    pub granularity: String,                  // "1h" | "1d"
    pub time_window: TimeWindow,

    pub bars: Vec<ChartBar>,
    pub indicators: Indicators,
    pub equity: Vec<EquityPoint>,
    pub drawdown: Vec<DrawdownPoint>,
    pub position: Vec<PositionPoint>,
    pub markers: ChartMarkers,
}

#[derive(Serialize, ts_rs::TS)]
pub struct ChartBar {
    pub time: i64,                            // unix seconds — Lightweight Charts format
    pub open: f64, pub high: f64, pub low: f64, pub close: f64,
    pub volume: f64,
}

#[derive(Serialize, ts_rs::TS)]
pub struct Indicators {
    pub sma_20: Vec<IndicatorPoint>,
    pub sma_50: Vec<IndicatorPoint>,
    pub sma_200: Vec<IndicatorPoint>,
    pub ema_20: Vec<IndicatorPoint>,
    pub ema_50: Vec<IndicatorPoint>,
    pub ema_200: Vec<IndicatorPoint>,
    pub bollinger: BollingerSeries,
    pub donchian: DonchianSeries,
    pub rsi_14: Vec<IndicatorPoint>,
    pub macd: MacdSeries,
    pub atr_14: Vec<IndicatorPoint>,
}

#[derive(Serialize, ts_rs::TS)] pub struct IndicatorPoint { pub time: i64, pub value: f64 }
#[derive(Serialize, ts_rs::TS)] pub struct BollingerSeries { pub upper: Vec<IndicatorPoint>, pub middle: Vec<IndicatorPoint>, pub lower: Vec<IndicatorPoint> }
#[derive(Serialize, ts_rs::TS)] pub struct DonchianSeries  { pub upper: Vec<IndicatorPoint>, pub lower: Vec<IndicatorPoint> }
#[derive(Serialize, ts_rs::TS)] pub struct MacdSeries      { pub line: Vec<IndicatorPoint>, pub signal: Vec<IndicatorPoint>, pub histogram: Vec<IndicatorPoint> }
#[derive(Serialize, ts_rs::TS)] pub struct DrawdownPoint   { pub time: i64, pub drawdown_pct: f64 }

#[derive(Serialize, ts_rs::TS)]
pub struct PositionPoint {
    pub time: i64,
    pub size: f64,                            // signed: + long, − short, 0 flat
    pub side: PositionSide,
}

#[derive(Serialize, ts_rs::TS)] pub enum PositionSide { Long, Short, Flat }

#[derive(Serialize, ts_rs::TS)]
pub struct ChartMarkers {
    pub trades: Vec<TradeMarker>,
    pub vetoes: Vec<VetoMarker>,
    pub holds: Vec<HoldMarker>,
}

#[derive(Serialize, ts_rs::TS)]
pub struct TradeMarker {
    pub time: i64,
    pub side: TradeSide,                      // Buy | Sell
    pub price: f64,
    pub size: f64,
    pub fee: f64,
    pub pnl_realized: Option<f64>,
    pub decision_index: u32,
    pub justification: Option<String>,
}

#[derive(Serialize, ts_rs::TS)] pub enum TradeSide { Buy, Sell }

#[derive(Serialize, ts_rs::TS)]
pub struct VetoMarker {
    pub time: i64,
    pub price: f64,
    pub reason: String,
    pub decision_index: u32,
}

#[derive(Serialize, ts_rs::TS)]
pub struct HoldMarker {
    pub time: i64,
    pub price: f64,
    pub conviction: Option<f64>,
    pub decision_index: u32,
}
```

### 7.2 Other surface payloads

```rust
pub struct CompareChartPayload {
    pub runs: Vec<CompareRunSeries>,          // one equity series per run
    pub shared_scenario: Option<ScenarioId>,  // Some(_) if all runs share a scenario
    pub price_backdrop: Option<Vec<ChartBar>>,// only populated if shared_scenario.is_some()
}

pub struct ScenarioChartPayload {
    pub scenario: Scenario,
    pub bars: Vec<ChartBar>,
    pub cache_status: CacheStatus,            // FullyCached | PartiallyCached { fetched_count, total_count } | NotCached
}

pub struct StrategyChartPayload {
    pub strategy_id: String,
    pub run_series: Vec<RunEquitySeries>,     // one per past run, equity normalized to trade-time
    pub scenarios: Vec<(ScenarioId, String)>, // scenario_id → display_name, for legend
}

pub struct ScenarioPreviewPayload {
    pub cache_key: String,                    // would-be cache key (no row yet)
    pub bars: Vec<ChartBar>,
    pub cache_status: CacheStatus,
    pub baseline_equity: Option<Vec<EquityPoint>>,  // Buy-and-Hold overlay
}
```

### 7.3 Indicator computation pipeline

`api::chart::build_run_payload(ctx, run_id)`:

1. Resolve run → scenario_id → load bars via `eval::bars::load_bars` (from custom-scenario spec).
2. Compute all indicators via `xvision-data::indicators::{sma, ema, bollinger, donchian, rsi, macd, atr}`. Same functions exposed by `xvn-mcp` to agents — humans and agents see identical math.
3. Read run's `equity_curve` from existing `runs` schema.
4. Derive `drawdown` from equity (running peak − current, % of peak).
5. Derive `position` from decision sequence (walk decisions, maintain running size).
6. Walk `decisions` and split into trade markers (`fill_price != null`), veto markers (`risk_outcome.verdict == Vetoed`), hold markers (`action == Hold`).
7. Serialize → JSON response.

### 7.4 Payload-size budgets

| Scenario | Bars | Indicators | Markers | JSON | Gzipped |
|---|---|---|---|---|---|
| 1 month, 1h | 720 | 11 × 720 | ~5–50 | ~600 KB | ~120 KB |
| 1 year, 1h | 8,760 | 11 × 8,760 | ~50–500 | ~7 MB | ~1.2 MB |
| 1 year, 1d | 365 | 11 × 365 | ~20–100 | ~300 KB | ~60 KB |

In-process serving (server=client), so wire cost is essentially zero. Browser-memory budget: ~7 MB JSON ≈ ~25 MB JS objects. Lightweight Charts handles this without issue. Bar-count cap of 100K per chart endpoint enforced at API; > 100K returns `ApiError::Validation("payload exceeds 100K bars; downsample granularity or shorten time_window")`.

### 7.5 Streaming protocol (`GET /api/eval/runs/:id/stream`, SSE)

```
event: bar
data: {"time": 1717459200, "open": ..., "high": ..., "low": ..., "close": ..., "volume": ...}

event: indicator_tail
data: {"sma_20": {"time": ..., "value": ...}, "rsi_14": {...}, ...}

event: marker
data: {"kind": "trade", "side": "Buy", "time": ..., "price": ..., "size": ..., "fee": ..., "decision_index": ...}

event: equity
data: {"time": ..., "equity_usd": ...}

event: status
data: {"phase": "running" | "paused" | "completed" | "failed", "message": "..."}
```

Server batches events on a 250 ms tick. Client flushes the batch on the next animation frame.

### 7.6 Initial fetch + stream pattern

1. Page load → `GET /api/eval/runs/:id/chart` returns the full snapshot.
2. Client renders snapshot.
3. If run is `running`, page opens SSE to `/stream`; events append to existing series.
4. If `completed`, no SSE — static.
5. On SSE drop: client re-fetches the snapshot (`/chart` again) and re-opens SSE. Simpler than event-id resumption in v1.

### 7.7 Wizard preview endpoint (`GET /api/scenarios/preview`)

Query params: `asset=ETH&from=...&to=...&granularity=1h`. No scenario row exists yet.

1. Compute `cache_key` from query (same blake3 derivation as custom-scenario spec).
2. `eval::bars::load_bars` (hits cache; fetches from Alpaca on miss).
3. Compute Buy-and-Hold baseline equity if requested.
4. Return `ScenarioPreviewPayload`.

---

## 8. Testing

| Layer | Test type | Coverage |
|---|---|---|
| `xvision-engine::api::chart::build_run_payload` | Unit | All indicator series computed; markers split correctly; drawdown / position derivation correct. |
| `api::chart` HTTP layer | Integration | Endpoints return ts-rs-generated shape; 100K-bar cap enforced; 404 / 400 paths. |
| Indicator parity | Property test | Bar-by-bar parity between `xvision-data::indicators::*` and `xvn-mcp`'s tool outputs. Defends "agents and humans see same math." |
| SSE stream | Integration | Connect, receive snapshot via GET, open stream, server pushes 100 batched events, client receives in order; reconnect after drop resumes via re-snapshot without duplicates. |
| `<RunChart>` component | Vitest + RTL | Renders snapshot; layer toggles add/remove series; localStorage persistence; click-marker side-panel callback. |
| `<LiveChart>` component | Vitest | Snapshot + SSE merge; follow-mode; pan-to-freeze; "Resume live". |
| End-to-end | Playwright | Open completed run → chart renders → toggle SMA off → reload → still off. Open running deployment → streaming → trade marker appears live → click → side panel. |

---

## 9. Performance budgets

| Surface | Budget | Enforcement |
|---|---|---|
| Run-detail initial paint | < 1.5s on a 1-year/1h scenario from warm cache | Playwright timing assertion in CI |
| Compare overlay paint | < 1s for 10 runs | Playwright timing |
| Live update latency | < 250 ms server-event → chart render (p95) | SSE timestamp + `performance.mark` |
| Chart bundle size delta | < 80 KB gzipped (lightweight-charts ~50 KB + ~30 KB components) | Vite build-size budget in CI |

Budget regression fails CI.

---

## 10. Accessibility

- Charts: `role="img"` + summarizing `aria-label` ("Equity curve for run sc_01HQ…, 1.4× growth over 90 days, 12% max drawdown").
- "Data table" toggle in chart header renders the same payload as `<table>` for screen readers / keyboard users — same fetch, no extra round-trip.
- Color tokens follow dashboard theme + pass WCAG AA contrast. No info conveyed by color alone (markers carry shape + color; series carry label + color).

---

## 11. Rollout

- **M1** ships behind no flag — replaces the existing SVG sparkline directly; SVG implementation deleted in same PR.
- **M2** + **M3** ship as new routes / endpoints; no fallback needed (surfaces don't exist yet).

---

## 12. Open questions (resolve during implementation)

- **Wizard preview real-strategy overlay.** v1 uses Buy-and-Hold baseline. Running a deterministic preview of the *actual* strategy needs a fast LLM-free arm or a cached prior-run snapshot. Defer to a follow-up.
- **Live cockpit reconnect semantics.** v1 = re-snapshot on drop. v2 may need event-id resumption if reconnect bandwidth becomes an issue.
- **Indicator parameterization.** Hard-coded set in v1. Operator-customizable parameters (SMA(34) etc.) is a v2 layer-panel feature.
- **Position-size derivation accuracy.** If `ExecutionReceipt` has fills not back-pointed to decisions (e.g. risk-layer auto-flat on kill switch), position bands drift. Audit in M1.
- **CompareChart legend overflow.** v1 caps at 10. v2 may add zoomable / paginated legends.
- **Bundle size budget headroom.** The 80 KB total may need raising if MACD histogram + position bands push our component code over 30 KB. Re-baseline at M1 close.

---

## 13. Acceptance criteria

- **M1**: `/eval-runs/:id` renders the multi-pane chart with default layers within budget. Layer toggles work + persist. SVG sparkline + overlay deleted from the codebase. `/eval-compare` renders N-equity overlay with legend, cap at 10 runs.
- **M2**: `/scenarios/:id` renders price-only candles + cache-status indicator. `/strategies/:id` renders per-run equity overlay color-coded by scenario.
- **M3**: `/live/<deployment_id>` streams updates at < 250 ms p95 latency. `/scenarios/new` shows a live-updating preview chart as operator changes asset/date/granularity.

---

## 14. Related follow-ups

- **F32** — implementation tracker for this spec.
- **F33** — TradingView Advanced Charts upgrade (Pine-script studies, drawing tools, multi-chart layouts). Requires application/license. Post-v1.
- **F30** (custom-scenario eval) — hard dependency; chart spec's M1 needs custom-scenario M1.
- **F25** — Claude Code skill; chart endpoint surface lands as a section in that skill post-M1.

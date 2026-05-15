# TradingView Lightweight Eval Surface - Design

> **Status:** Draft / spec. Drafted 2026-05-14.
> **Author:** xvision team.
> **Companion specs:** [TradingView Charts Design](./2026-05-11-tradingview-charts-design.md) | [Alpaca Paper Eval Surface](./2026-05-14-alpaca-paper-eval-surface-design.md) | [Custom-Scenario Eval](./2026-05-11-custom-scenario-eval-design.md)
> **Tracking:** Expands the existing TradingView chart spec from "replace inline SVG with charts" into a complete Lightweight Charts API adapter and eval visualization surface.

---

## 1. Purpose

The current TradingView chart plan correctly chose Lightweight Charts, but it still treats the library as a rendering component for a few hardcoded eval charts. That misses much of the library surface:

- Top-level creation helpers and plugin helpers.
- Full chart methods for resizing, screenshots, series/pane management, crosshair control, and event subscriptions.
- Full series methods for data mutation, price/coordinate conversion, price lines, data subscriptions, primitives, pane movement, and ordering.
- Full time-scale controls, including logical range, coordinate conversion, size/range subscriptions, and options.
- Full price-scale controls.
- Pane APIs.
- Marker, up/down marker, image watermark, text watermark, custom series, and primitive plugin surfaces.

This spec turns "TradingView Light" into a reusable Xvision chart runtime: a typed adapter around `lightweight-charts` that every eval view can use, not one-off React code per screen.

---

## 2. Source Inventory

Official docs reviewed while drafting:

- Lightweight Charts docs home: `https://tradingview.github.io/lightweight-charts/`
- API reference, version 5.2: `https://tradingview.github.io/lightweight-charts/docs/api`
- `IChartApi`: `https://tradingview.github.io/lightweight-charts/docs/api/interfaces/IChartApi`
- `ISeriesApi`: `https://tradingview.github.io/lightweight-charts/docs/api/interfaces/ISeriesApi`
- `ITimeScaleApi`: `https://tradingview.github.io/lightweight-charts/docs/api/interfaces/ITimeScaleApi`
- `IPriceScaleApi`: `https://tradingview.github.io/lightweight-charts/docs/api/interfaces/IPriceScaleApi`
- `IPaneApi`: `https://tradingview.github.io/lightweight-charts/docs/api/interfaces/IPaneApi`
- `ISeriesMarkersPluginApi`: `https://tradingview.github.io/lightweight-charts/docs/api/interfaces/ISeriesMarkersPluginApi`
- `IPriceLine`: `https://tradingview.github.io/lightweight-charts/docs/api/interfaces/IPriceLine`
- TradingView product comparison: `https://www.tradingview.com/charting-library-docs/latest/getting_started/product-comparison/`

Version note: use `lightweight-charts@^5.2` unless implementation finds a Vite/React/TS constraint. The existing 2026-05-11 spec says `4.x`; this spec updates that to current `5.2` because the API reference now exposes pane support and 5.x helpers that the eval surface needs.

---

## 3. Locked Decisions

| # | Decision |
|---|---|
| 1 | **Use `lightweight-charts@^5.2` via npm.** No CDN, no inline SVG replacement path. |
| 2 | **Build a chart runtime adapter.** React components call Xvision chart runtime functions, not raw `createChart` directly in every route. |
| 3 | **Model every public function we need, even when UI affordances lag.** The adapter exposes chart, series, time-scale, price-scale, pane, marker, price-line, watermark, screenshot, and primitive operations. |
| 4 | **Charts consume Xvision chart payloads, not broker payloads.** Alpaca/Broker data is normalized server-side; frontend remains venue-agnostic. |
| 5 | **Range controls are arbitrary, not fixed.** Presets stay as shortcuts, but the runtime supports any visible time range or logical range supported by Lightweight Charts. |
| 6 | **Panes are first-class.** Run detail uses panes for price, indicators, equity, drawdown, volume, order flow, and broker activity overlays. |
| 7 | **All interactive state is serializable.** Visible range, layer toggles, selected marker, crosshair sync, pane heights, and series visibility can be stored/restored. |
| 8 | **No market data assumption.** Lightweight Charts is a client-side chart library; all market data comes from Xvision API payloads. |
| 9 | **Advanced Charts remains a separate future product.** Pine studies, built-in drawing tools, and hosted datafeed integration are not pulled into this Lightweight Charts plan. |
| 10 | **Export is built in.** `takeScreenshot` becomes a first-class run artifact/export path. |

---

## 4. Lightweight Charts API Surface to Model

### 4.1 Top-level functions and variables

| Xvision adapter | Lightweight Charts API | Eval use |
|---|---|---|
| `createTimeChart` | `createChart` | Standard eval chart. |
| `createCustomHorzChart` | `createChartEx` | Future custom horizontal scale charts. |
| `createOptionsChart` | `createOptionsChart` | Options surface when options eval unlocks. |
| `createYieldCurveChart` | `createYieldCurveChart` | Future fixed-income/yield curve experiments. |
| `createMarkers` | `createSeriesMarkers` | Buy/sell/hold/veto/fill markers. |
| `createUpDownMarkersAdapter` | `createUpDownMarkers` | Compact up/down overlays for dense trades. |
| `createImageWatermarkAdapter` | `createImageWatermark` | Broker/run watermark, export branding. |
| `createTextWatermarkAdapter` | `createTextWatermark` | Scenario/run labels. |
| `createDefaultHorzBehavior` | `defaultHorzScaleBehavior` | Custom horizontal behavior experiments. |
| `isBusinessDayTime` | `isBusinessDay` | Payload validation for daily bars. |
| `isUtcTimestampTime` | `isUTCTimestamp` | Payload validation for intraday bars. |
| `lightweightVersion` | `version` | Diagnostics and support bundle. |
| series definitions | `AreaSeries`, `BarSeries`, `BaselineSeries`, `CandlestickSeries`, `HistogramSeries`, `LineSeries` | All eval series types. |

### 4.2 Chart API

Adapter methods:

- `applyOptions`
- `remove`
- `resize`
- `addSeries`
- `addCustomSeries`
- `removeSeries`
- `subscribeClick` / `unsubscribeClick`
- `subscribeDblClick` / `unsubscribeDblClick`
- `subscribeCrosshairMove` / `unsubscribeCrosshairMove`
- `priceScale`
- `timeScale`
- `options`
- `takeScreenshot`
- `addPane`
- `panes`
- `removePane`
- `swapPanes`
- `autoSizeActive`
- `chartElement`
- `setCrosshairPosition`
- `clearCrosshairPosition`
- `paneSize`
- `horzBehaviour`

Eval uses:

- Crosshair sync across panes and compare charts.
- Click handlers for markers/series points.
- Screenshot export for run reports.
- Pane management for indicators/equity/drawdown/volume/order flow.
- Runtime resizing for dashboard layout and mobile.

### 4.3 Series API

Adapter methods:

- `priceFormatter`
- `priceToCoordinate`
- `coordinateToPrice`
- `barsInLogicalRange`
- `applyOptions`
- `options`
- `priceScale`
- `setData`
- `update`
- `pop`
- `dataByIndex`
- `data`
- `subscribeDataChanged` / `unsubscribeDataChanged`
- `createPriceLine`
- `removePriceLine`
- `priceLines`
- `seriesType`
- `lastValueData`
- `attachPrimitive`
- `detachPrimitive`
- `moveToPane`
- `seriesOrder`
- `setSeriesOrder`
- `getPane`

Eval uses:

- `setData` for initial payload hydration.
- `update` for SSE/live paper mirror appends.
- `historicalUpdate` support for broker corrections and late fills.
- `barsInLogicalRange` for lazy history fetch and "load more bars" UX.
- `priceToCoordinate`/`coordinateToPrice` for custom overlays and hit testing.
- `createPriceLine` for entry, stop, target, liquidation, break-even, and drawdown thresholds.
- `attachPrimitive` for shaded regimes, position bands, custom fills, and annotations.
- `moveToPane`/ordering for user-customizable chart layouts.

### 4.4 Time-scale API

Adapter methods:

- `scrollPosition`
- `scrollToPosition`
- `scrollToRealTime`
- `getVisibleRange`
- `setVisibleRange`
- `getVisibleLogicalRange`
- `setVisibleLogicalRange`
- `resetTimeScale`
- `fitContent`
- `logicalToCoordinate`
- `coordinateToLogical`
- `timeToIndex`
- `timeToCoordinate`
- `coordinateToTime`
- `width`
- `height`
- `subscribeVisibleTimeRangeChange` / `unsubscribeVisibleTimeRangeChange`
- `subscribeVisibleLogicalRangeChange` / `unsubscribeVisibleLogicalRangeChange`
- `subscribeSizeChange` / `unsubscribeSizeChange`
- `applyOptions`
- `options`

Eval uses:

- Arbitrary date windows, not just `[1d, 1w, 1m, 3m, All]`.
- Logical ranges beyond available data for margins and aligned comparisons.
- Range subscriptions for lazy loading and URL/share state.
- `scrollToRealTime` for live paper cockpit.
- Coordinate/time conversion for selecting decisions and broker events.

### 4.5 Price-scale API and price lines

Price-scale methods:

- `applyOptions`
- `options`
- `width`
- `setVisibleRange`
- `getVisibleRange`
- `setAutoScale`

Price-line methods:

- `applyOptions`
- `options`

Eval uses:

- Manual/auto price range control.
- Per-series price lines for stops, take profits, average entry, trailing high-water mark, liquidation levels.
- Price-scale mode toggles: normal, percent, indexed, log where supported by chart options.

### 4.6 Pane API

Adapter methods:

- `getHeight`
- `setHeight`
- `moveTo`
- `paneIndex`
- `getSeries`
- `getHTMLElement`
- `attachPrimitive`
- `detachPrimitive`
- `priceScale`
- `setPreserveEmptyPane`
- `preserveEmptyPane`

Eval uses:

- User-resizable panes.
- Reorder price/indicator/equity/drawdown/volume panes.
- Preserve empty panes while toggling layers.
- Attach pane-level primitives like session shading, drawdown zones, and event spans.

### 4.7 Marker and watermark plugin APIs

Markers:

- `setMarkers`
- `markers`
- `detach`
- `getSeries`
- `applyOptions`

Watermarks:

- Text watermark options for run/scenario labels.
- Image watermark options for exported reports.

Eval marker types:

- Decision markers: buy, sell, hold, veto.
- Broker markers: submitted, new, partial fill, fill, canceled, rejected, replaced, expired.
- Risk markers: stop-loss armed, take-profit armed, kill-switch, order replace rejected, order cancel rejected.
- Finding markers: drawdown concentration, overtrading, regime mismatch.

### 4.8 Custom series and primitives

Use `addCustomSeries`, `attachPrimitive`, and pane/series primitives for:

- Position-size bands.
- Regime background spans.
- Broker order lifecycle ribbons.
- Slippage bands.
- Fill-latency connectors from decision timestamp to fill timestamp.
- Compare chart leader/follower deltas.
- Future custom order-book imbalance charts.

---

## 5. Xvision Chart Runtime

### 5.1 New files

```text
frontend/web/src/components/chart/
  runtime/
    chart-runtime.ts
    chart-registry.ts
    chart-commands.ts
    chart-events.ts
    chart-state.ts
    chart-theme.ts
    chart-time.ts
    chart-series.ts
    chart-panes.ts
    chart-markers.ts
    chart-primitives.ts
    chart-export.ts
  RunChart.tsx
  CompareChart.tsx
  ScenarioChart.tsx
  StrategyChart.tsx
  LiveChart.tsx
  BrokerTimelineChart.tsx
  MiniSparkline.tsx
```

The runtime owns all direct `lightweight-charts` imports. Route components receive declarative payloads and dispatch high-level commands.

### 5.2 Runtime object

```ts
export interface XvnChartRuntime {
  id: string;
  chart: IChartApi;
  panes: Map<string, IPaneApi<Time>>;
  series: Map<string, ISeriesApi<SeriesType, Time>>;
  markers: Map<string, ISeriesMarkersPluginApi<Time>>;
  priceLines: Map<string, IPriceLine>;
  dispose(): void;
  dispatch(command: ChartCommand): void;
  snapshotState(): ChartState;
  restoreState(state: ChartState): void;
}
```

### 5.3 Commands

```ts
export type ChartCommand =
  | { type: "chart.apply_options"; options: DeepPartial<ChartOptions> }
  | { type: "chart.resize"; width: number; height: number; forceRepaint?: boolean }
  | { type: "chart.screenshot"; includeTopLayer?: boolean; includeCrosshair?: boolean }
  | { type: "series.add"; spec: SeriesSpec }
  | { type: "series.remove"; seriesId: string }
  | { type: "series.set_data"; seriesId: string; data: DataItem[] }
  | { type: "series.update"; seriesId: string; item: DataItem; historicalUpdate?: boolean }
  | { type: "series.pop"; seriesId: string; count: number }
  | { type: "series.apply_options"; seriesId: string; options: object }
  | { type: "series.move_to_pane"; seriesId: string; paneId: string }
  | { type: "series.set_order"; seriesId: string; order: number }
  | { type: "price_line.create"; seriesId: string; priceLineId: string; options: PriceLineOptions }
  | { type: "price_line.remove"; seriesId: string; priceLineId: string }
  | { type: "markers.set"; markerLayerId: string; markers: XvnSeriesMarker[] }
  | { type: "time.set_visible_range"; range: TimeRange }
  | { type: "time.set_visible_logical_range"; range: LogicalRange }
  | { type: "time.fit_content" }
  | { type: "time.scroll_to_realtime" }
  | { type: "price_scale.set_visible_range"; priceScaleId: string; range: PriceRange }
  | { type: "price_scale.set_auto_scale"; priceScaleId: string; enabled: boolean }
  | { type: "pane.add"; paneId: string; preserveEmptyPane?: boolean }
  | { type: "pane.remove"; paneId: string }
  | { type: "pane.set_height"; paneId: string; height: number }
  | { type: "pane.move"; paneId: string; index: number }
  | { type: "crosshair.set"; price: number; time: Time; seriesId: string }
  | { type: "crosshair.clear" }
  | { type: "primitive.attach"; target: PrimitiveTarget; primitive: XvnPrimitiveSpec }
  | { type: "primitive.detach"; primitiveId: string };
```

### 5.4 State

```ts
export interface ChartState {
  version: 1;
  visibleRange?: TimeRange;
  visibleLogicalRange?: LogicalRange;
  paneHeights: Record<string, number>;
  paneOrder: string[];
  layerVisibility: Record<string, boolean>;
  seriesOrder: Record<string, number>;
  selectedMarkerId?: string;
  crosshair?: { time: Time; price: number; seriesId: string };
  followRealtime?: boolean;
}
```

Persistence keys:

- `xvision.chart.state.run-detail.<run_id>`
- `xvision.chart.state.compare`
- `xvision.chart.state.scenario.<scenario_id>`
- `xvision.chart.state.strategy.<strategy_id>`
- `xvision.chart.state.live.<deployment_id>`

---

## 6. Eval Chart Payloads

### 6.1 Server-side chart module

Create `crates/xvision-engine/src/api/chart.rs` with payload builders:

- `build_run_chart_payload`
- `build_compare_chart_payload`
- `build_scenario_chart_payload`
- `build_strategy_chart_payload`
- `build_live_snapshot_payload`
- `build_broker_timeline_payload`

Payloads normalize:

- bars,
- equity,
- drawdown,
- indicators,
- decisions,
- broker orders,
- broker events,
- positions,
- activities,
- findings,
- scenario/regime spans.

### 6.2 Payload schema

```rust
pub struct EvalChartPayload {
    pub chart_id: String,
    pub surface: ChartSurface,
    pub time_window: TimeWindow,
    pub timezone: String,
    pub panes: Vec<PaneSpec>,
    pub series: Vec<SeriesPayload>,
    pub marker_layers: Vec<MarkerLayerPayload>,
    pub price_lines: Vec<PriceLinePayload>,
    pub primitives: Vec<PrimitivePayload>,
    pub events: Vec<ChartEventPayload>,
    pub default_state: ChartStatePayload,
}

pub enum ChartSurface {
    RunDetail,
    Compare,
    Scenario,
    Strategy,
    Live,
    BrokerTimeline,
    WizardPreview,
}
```

### 6.3 Series types

Support all built-in series definitions:

- candlestick,
- bar,
- line,
- area,
- baseline,
- histogram,
- custom.

Mapping:

- Price OHLC: candlestick by default; bar optional.
- Equity: area or line.
- Drawdown: baseline or histogram.
- Volume: histogram.
- RSI/MACD/ATR: line/histogram combos.
- Portfolio PnL: baseline.
- Broker activity count/order count: histogram.

---

## 7. Surface Plan

### 7.1 Run detail

Full multi-pane chart:

- Price candles.
- Indicators.
- Decision markers.
- Broker order lifecycle markers.
- Entry/stop/take-profit/trailing price lines.
- Equity pane.
- Drawdown pane.
- Volume pane.
- Broker activity/order-flow pane.

Interactions:

- Click decision row -> `timeToCoordinate`, scroll/select marker.
- Click marker -> side panel with decision/order/activity payload.
- Arbitrary visible range picker plus presets.
- Screenshot/export.

### 7.2 Compare

Compare chart modes:

- normalized equity overlay,
- absolute equity overlay,
- drawdown overlay,
- trade-time elapsed axis,
- wall-clock axis when scenarios align,
- price backdrop when selected runs share scenario/asset.

Use logical ranges for elapsed-time alignment rather than fixed sample index scaling.

### 7.3 Scenario

Scenario chart:

- Price candles.
- Volume.
- Regime spans via pane primitives.
- Calendar/session shading for equities.
- Cache coverage overlay.
- Arbitrary range/zoom.

### 7.4 Strategy

Strategy chart:

- Historical run equity curves grouped by scenario.
- Layer toggles by scenario, model, run mode, and date.
- Findings markers along equity curves.

### 7.5 Live cockpit

Live chart:

- Initial snapshot via API.
- SSE events dispatched as `series.update`, marker updates, price-line updates, and primitive updates.
- `scrollToRealTime` while follow mode is enabled.
- Pan/zoom disables follow mode.
- Reconnect reconciles via snapshot diff.

### 7.6 Broker timeline

New chart surface that visualizes Alpaca paper artifacts:

- order state timeline,
- partial fills,
- replacements/cancels/rejects,
- position changes,
- portfolio history,
- activity ledger annotations.

This pairs directly with the Alpaca Paper Eval Surface spec.

---

## 8. Milestones

### M1 - Runtime adapter and dependency

Ships:

- `lightweight-charts@^5.2`.
- Runtime directory.
- `createChart`, `addSeries`, `setData`, `update`, `timeScale`, `priceScale`, marker, price-line, screenshot wrappers.
- Replace inline `EquityChart` and `EquityOverlay`.

Acceptance:

- No direct `lightweight-charts` imports outside `components/chart/runtime`.
- Run detail and compare no longer use inline SVG charts.
- Existing eval screens render with same data as before.

### M2 - Full chart/time/price/series method coverage

Ships:

- All chart methods listed in section 4.2.
- All series methods listed in section 4.3.
- All time-scale methods listed in section 4.4.
- All price-scale/price-line methods listed in section 4.5.
- Command/state serialization.

Acceptance:

- Unit tests dispatch every command against a mocked runtime.
- Playwright verifies zoom/pan/range restoration and screenshot export.

### M3 - Pane and primitive system

Ships:

- Pane API coverage.
- Pane height/order persistence.
- Regime spans and position bands as primitives.
- Multi-pane run detail chart.

Acceptance:

- User can toggle/reorder panes without losing chart state.
- Regime spans and position bands survive range changes.

### M4 - Broker and stream overlays

Ships:

- Broker order lifecycle markers.
- `trade_updates` overlays.
- Live paper SSE -> chart command pipeline.
- Broker timeline chart.

Acceptance:

- Partial fill event appends a marker without full chart remount.
- Reconnect reconciles from latest server snapshot.

### M5 - Advanced chart interactions

Ships:

- Cross-chart synchronization for compare.
- Lazy bar loading through `barsInLogicalRange` and visible range subscriptions.
- Marker side panel.
- Data table parity for accessibility.
- Export bundle containing screenshot + chart state + raw payload hash.

Acceptance:

- Compare chart can sync crosshair across two separate chart instances.
- Run report export includes a chart screenshot and reproducible chart state.

---

## 9. Test Plan

- Type tests for payload-to-series conversion.
- Runtime unit tests for every command variant.
- State round-trip tests through localStorage shape.
- Playwright screenshots for run detail, compare, scenario, strategy, live, and broker timeline surfaces.
- Canvas nonblank checks for desktop/mobile.
- Event tests for click, double click, crosshair, visible range, logical range, and size subscriptions.
- Performance budget tests for 10k, 50k, and 100k bars.
- Accessibility tests for data-table parity and `aria-label` coverage.

---

## 10. Migration Notes

- Delete `EquityChart` in `frontend/web/src/routes/eval-runs-detail.tsx`.
- Delete `EquityOverlay` in `frontend/web/src/routes/eval-compare.tsx`.
- Keep route components thin; they fetch payloads and render chart components.
- Update the earlier 2026-05-11 chart spec from `lightweight-charts@4.x` to `^5.2`.
- Do not add Advanced Charts concepts to this adapter. If a future Advanced Charts license is used, it gets a separate adapter with a compatible payload input.

---

## 11. Open Questions

- Should Xvision expose custom primitives as a stable plugin API for strategy authors, or keep them internal until after v1?
- Do we allow users to persist named chart layouts, or only per-surface last state?
- Should chart screenshots be stored automatically on run completion, or generated on demand?
- How much of the broker timeline belongs in the main run detail chart versus a separate tab?
- Do we keep all 100K bars client-side, or add downsampling before the full chart rollout?

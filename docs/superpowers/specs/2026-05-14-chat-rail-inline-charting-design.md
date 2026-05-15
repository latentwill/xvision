# Chat Rail Inline Charting - Design

> **Status:** Draft / spec. Drafted 2026-05-14.
> **Author:** xvision team.
> **Prototype source:** `docs/design/xvn/project/mobile-shared.jsx` and `docs/design/xvn/project/mobile-chat.jsx`.
> **Companion specs:** [XVN Mobile-First Framework](./2026-05-14-mobile-first-framework-design.md) | [TradingView Charts Design](./2026-05-11-tradingview-charts-design.md) | [TradingView Lightweight Eval Surface](./2026-05-14-tradingview-lightweight-eval-surface-design.md)

---

## 1. Purpose

The mobile prototype shows charts inside chat responses: compact equity cards, compare cards, dashboard summaries, sparklines, returns histograms, and run-specific KPI strips. These are not the same product surface as the full eval chart.

Full eval charting needs financial chart interactions, panes, markers, and zoom. The chat rail needs fast, small, stable, message-sized visual summaries that can appear many times in a scrolling thread.

This spec defines a separate inline charting module for the chat rail.

**Recommendation:** do not use TradingView Lightweight Charts inside chat messages. Use a purpose-built React SVG renderer for inline cards in v1, keep full TradingView/Lightweight Charts for full eval surfaces, and reserve uPlot for a future expanded inline chart mode if SVG performance is not enough.

---

## 2. Source Inventory

Local files reviewed:

| File | Relevant details |
|---|---|
| `docs/design/xvn/project/mobile-shared.jsx` | `MiniChart`, `ChatChartCard`, `ChatRunListCard`, `ChatStrategyCard`, `ChatActionCard`. |
| `docs/design/xvn/project/mobile-chat.jsx` | Inline 24h combined equity, eval equity curve, findings chips. |
| `docs/design/xvn/project/mobile-eval.jsx` | Full-screen eval list cards, returns histogram, run detail chart anatomy. |
| `docs/design/xvn/project/mobile-responsive.jsx` | Tablet/desktop chat cards in docked rail. |
| `frontend/web/src/components/shell/ChatRail.tsx` | Current rail renders markdown, user bubbles, tool chips, and tool narratives. |
| `frontend/web/src/api/chat_rail.ts` | Current content block union only supports text/tool blocks. |

External sources reviewed while evaluating options:

- TradingView Lightweight Charts documentation: `https://tradingview.github.io/lightweight-charts/`
- uPlot README: `https://github.com/leeoniya/uPlot`
- Apache ECharts home/docs: `https://echarts.apache.org/`
- Recharts docs: `https://recharts.github.io/en-US/`
- visx README/docs: `https://github.com/airbnb/visx`
- Chart.js docs: `https://www.chartjs.org/docs/latest/`

---

## 3. Inline Chart Requirements

Inline chart cards must support:

- Equity curve: one area/line series with KPI strip.
- Compare overlay: two to four equity lines with optional delta.
- Returns histogram: small bar distribution.
- Drawdown strip: negative area or red bars.
- Strategy sparkline: tiny line in strategy cards and run rows.
- Trade-marker summary: markers over equity or a compact event strip.

Non-negotiable constraints:

- Width: 280px to 360px in chat rail; 330px to 390px on phone.
- Height: 54px sparkline, 86px chat chart, 140px expanded inline chart.
- Many instances can exist in a single scrollback.
- No chart instance should steal scroll gestures from the chat thread.
- A card must render a useful static view before any hover/tap interaction.
- Tapping the card opens the canonical route or an expanded detail sheet.
- Data must be serializable in chat history.

---

## 4. Library Evaluation

| Option | Fit | Strengths | Risks |
|---|---|---|---|
| **Custom React SVG renderer** | **Best v1 fit** | Zero dependency; exact prototype match; accessible DOM; easy to render many static cards; straightforward snapshot/unit tests; no canvas lifecycle in scrollback. | We own path generation, tooltip math, downsampling, and edge cases. |
| **uPlot** | Good phase-2 option | Official README positions it as a small, fast Canvas 2D time-series chart around 50 KB, with strong streaming/cursor performance. MIT. | No built-in drag scrolling/panning; touch zoom requires plugins or custom work. React integration is third-party. Canvas instances inside a long chat thread need lifecycle care. |
| **Chart.js** | Acceptable but not ideal | MIT, canvas, active, TypeScript typings, common React wrapper path, good generic chart types. | More general-purpose than needed; styling through chart options/plugins, not CSS; animations/defaults need disabling; heavier than custom SVG for microcharts. |
| **Recharts** | Easy but less disciplined | React-native component model, SVG, MIT, simple line/area/bar charts. | Adds D3 submodules to solve a small rendering problem; many SVG nodes if misused; less control over dense card layout than custom SVG. |
| **visx** | Good for a custom chart package | Low-level React + D3 primitives; pick only packages needed; MIT. | Still requires building our chart system; for this scope, custom SVG plus small local utilities is simpler. |
| **Apache ECharts** | Overkill | Rich chart types, Canvas/SVG renderers, accessibility features, progressive rendering. | Too much library for message cards; configuration surface is large; per-message chart instances are wasteful. |
| **TradingView Lightweight Charts** | Wrong placement | Excellent for full financial chart surfaces; v5.2 docs position it around interactive financial charts and pane support. | Over-specialized for inline summaries; canvas lifecycle and gesture model are a poor match for many small chat cards; duplicates the full eval chart concern. |

Decision:

- **V1 inline cards:** custom React SVG renderer.
- **Full eval/run chart:** TradingView Lightweight Charts remains the right tool per companion specs.
- **Future expanded inline detail:** evaluate uPlot if we need interactive cursor/zoom inside a chat card without opening the full route.

---

## 5. Locked Decisions

| # | Decision |
|---|---|
| 1 | **No TradingView chart instances inside chat messages.** Chat uses its own inline chart module. |
| 2 | **Inline charts are SVG in v1.** Use path/rect/circle primitives, not a generic chart library. |
| 3 | **Cards open canonical detail.** Inline charts summarize; full exploration happens in `/eval-runs/:id`, `/eval/compare`, or an expanded sheet. |
| 4 | **Server sends typed payloads.** Do not infer charts from markdown tables or assistant prose. |
| 5 | **Payloads are history-safe.** Every inline chart payload can be stored in `chat_messages.content_blocks_json` and re-rendered later. |
| 6 | **Render budget is strict.** A card with <=500 total points renders synchronously; larger payloads must be downsampled before entering chat history. |
| 7 | **Cards are accessible.** Each chart has an `aria-label` summary and an expandable tabular data view where useful. |
| 8 | **No inline pinch/zoom in v1.** Tap/keyboard opens detail. Hover/long-press tooltip is allowed but not required for launch. |
| 9 | **Design tokens come from Folio dark.** Chart colors use gold, danger, text-3, warn, info, and transparent fills from the app token set. |

---

## 6. Payload Contract

Extend chat content blocks:

```ts
type ContentBlock =
  | { type: "text"; text: string }
  | { type: "tool_use"; id: string; name: string; input: unknown }
  | { type: "tool_result"; tool_use_id: string; content: string }
  | { type: "inline_chart"; payload: InlineChartPayload }
  | { type: "run_list"; payload: ChatRunListPayload }
  | { type: "strategy_card"; payload: ChatStrategyPayload }
  | { type: "action_card"; payload: ChatActionPayload };
```

Inline chart payload:

```ts
type InlineChartPayload = {
  version: 1;
  chart_id: string;
  kind: "equity" | "compare" | "histogram" | "drawdown" | "sparkline" | "trade_markers";
  title: string;
  subtitle?: string;
  tone?: "positive" | "negative" | "neutral" | "warning";
  primary_value?: string;
  series: InlineChartSeries[];
  metrics?: InlineMetric[];
  legend?: InlineLegendItem[];
  actions?: InlineAction[];
  source?: InlineChartSource;
  a11y_summary: string;
};

type InlineChartSeries = {
  id: string;
  label: string;
  kind: "line" | "area" | "bar" | "marker" | "threshold";
  color_token?: "gold" | "danger" | "warn" | "info" | "muted";
  points: InlinePoint[];
};

type InlinePoint = {
  x: number | string;
  y: number;
  meta?: Record<string, string | number | boolean | null>;
};

type InlineMetric = {
  label: string;
  value: string;
  tone?: "positive" | "negative" | "neutral" | "warning";
};

type InlineAction = {
  label: string;
  href?: string;
  command?: string;
  payload?: unknown;
};

type InlineChartSource = {
  run_id?: string;
  strategy_id?: string;
  scenario_id?: string;
  compare_run_ids?: string[];
  generated_at: string;
};
```

Rules:

- `points` are real data values, not pre-normalized SVG coordinates.
- Client normalizes to the viewbox.
- Server is responsible for downsampling and rounding payloads before storage.
- `a11y_summary` is required and should include the key result in prose.
- `actions` should include the canonical "Open run", "Open compare", or "See breakdown" route where available.

---

## 7. Rendering Contract

### 7.1 Component structure

Proposed files:

```
frontend/web/src/components/chat/inline-chart/
  InlineChartCard.tsx
  InlineChartSvg.tsx
  InlineHistogram.tsx
  InlineSparkline.tsx
  InlineTooltip.tsx
  shape.ts
  downsample.ts
  palette.ts
  types.ts
```

### 7.2 Layout sizes

| Placement | Width | Chart height | Total card height |
|---|---:|---:|---:|
| Phone chat card | container width | 86px | 190px to 230px |
| Desktop rail card | container width | 76px to 86px | 180px to 220px |
| Strategy mini card | container width | 54px | 150px to 190px |
| Run-row sparkline | 120px to 180px | 38px | row-owned |
| Expanded inline sheet | container width | 140px to 220px | sheet-owned |

### 7.3 SVG rendering rules

- Use a fixed internal viewbox, e.g. `0 0 320 86`.
- Use `preserveAspectRatio="none"` for mini chart fills where prototype uses full-width stretch.
- Draw grid lines only for chart cards, not tiny sparklines.
- Area fills use tokenized transparent gradients.
- Lines should be 1.25px to 1.75px.
- Histogram bars use rounded corners only if width stays >=4px.
- Marker circles must be visually present but not become primary tap targets in inline mode.

### 7.4 Interaction

V1 interactions:

- Entire card is focusable if it has a canonical action.
- Enter/Space activates primary action.
- Pointer tap opens route or expanded sheet.
- Optional hover/long-press tooltip can show nearest point, but must not be required to understand the card.
- Action links in the footer are individually focusable.

Deferred interactions:

- Inline pinch/zoom.
- In-card marker selection.
- Brush/range selection.
- Crosshair sync with full dashboard charts.

---

## 8. Data Sources

Inline charts are not new analysis endpoints. They are summaries generated from existing domain responses:

| Intent | Data source | Inline payload |
|---|---|---|
| "Today's P&L" | dashboard/home or paper eval summary | equity card + P&L metrics |
| "How did my last eval run go?" | run detail + metrics summary | equity card + KPI strip + findings chips |
| "Compare A vs B" | compare report | compare overlay card |
| "Show top runs" | run list endpoint | run list card with sparklines |
| "Pause/resume" | action result | action confirmation card |
| "Draft from finding" | finding + strategy context | action card + strategy card |

The assistant can decide which card to emit, but the backend or tool layer should construct the payload using typed helpers. The model should not hand-write chart JSON directly unless schema validation sits between the model and storage.

---

## 9. Backend and Tooling

Add a typed builder layer near chat/tool result creation:

```
crates/xvision-engine/src/chat_session/rich_blocks.rs
```

Suggested helpers:

```rust
pub fn inline_equity_chart(run: &RunDetail) -> RichContentBlock;
pub fn inline_compare_chart(report: &ComparisonReport) -> RichContentBlock;
pub fn inline_returns_histogram(runs: &[RunSummary]) -> RichContentBlock;
pub fn inline_strategy_card(strategy: &StrategySummary, spark: &[EquityPoint]) -> RichContentBlock;
pub fn action_confirmation_card(kind: ActionKind, target: ActionTarget) -> RichContentBlock;
```

Validation:

- Reject payloads without `a11y_summary`.
- Reject >500 total points for inline cards unless the payload is explicitly marked `downsampled: true`.
- Reject unknown `href` patterns; actions should route inside the SPA or map to known command ids.

---

## 10. Frontend Integration

Refactor `ChatRail.tsx`:

- Keep stream/session orchestration in the shell component.
- Move message rendering into `ChatThread`.
- Add `ContentBlockView` that switches on block type.
- Render rich cards directly from stored history.

History conversion should preserve block order. Current `historyToBubbles()` joins text blocks and extracts tool chips; it will need to keep an ordered list:

```ts
type AssistantBubble = {
  role: "assistant";
  blocks: RenderableBlock[];
  tools: Tool[];
};
```

This avoids losing the placement of "text -> chart -> text -> chips".

---

## 11. Performance Budget

Targets on a mid-range mobile device:

| Operation | Budget |
|---|---:|
| Render one inline chart card with 200 points | <16ms scripting/rendering |
| Render thread with 10 chart cards | <100ms added scripting on initial mount |
| Scroll through chart-heavy thread | no visible hitch from chart cards |
| Payload size per chart block | <15 KB preferred, <40 KB hard cap |
| Total points per inline chart | 500 hard cap |

Implementation tactics:

- Downsample server-side for chat history.
- Use `React.memo` for chart cards.
- Use stable gradient ids derived from `chart_id`, not `Math.random()`.
- Lazy-render below-the-fold cards with `IntersectionObserver` if the thread becomes chart-heavy.
- Avoid per-point DOM nodes for lines; one `<path>` per line/area.

---

## 12. Accessibility

Every chart card:

- `role="group"` on the card.
- `aria-label` or visible screen-reader text containing `a11y_summary`.
- Footer action has explicit label, e.g. "Open run 01H8N7Z".
- Expanded detail offers a table view for chart data where useful.
- Color is not the only signal: include signed numeric values and labels.

---

## 13. Testing

Unit tests:

- Path generation for positive, negative, flat, and missing-value series.
- Histogram bin rendering with positive/negative values.
- Payload validation caps.
- `ContentBlockView` renders `inline_chart` in correct order with text.

Visual tests:

- 390px phone chat card.
- 360px desktop rail card.
- 320px narrow rail fallback.
- Long title truncation.
- Negative return card.
- Flat line card.

Manual QA:

- Chat history reload preserves charts.
- Tapping chart opens route.
- Keyboard focus order reaches card and footer actions.
- Screen reader announces chart summary.
- Thread scroll remains smooth with at least 10 chart cards.

---

## 14. Milestones

| Milestone | Ships |
|---|---|
| M1 | `InlineChartPayload` TS type, local mock cards, SVG renderers, visual fixture page. |
| M2 | Refactor `ChatRail.tsx` to preserve ordered assistant blocks. |
| M3 | Backend rich block builders for run detail, compare, run list, and action confirmation. |
| M4 | Tool/assistant integration so eval and compare intents emit inline chart blocks. |
| M5 | Accessibility/table expansion and performance pass. |
| M6 | Optional uPlot prototype for expanded inline detail if V1 telemetry shows SVG is insufficient. |

---

## 15. Open Questions

1. Should rich blocks be part of the existing LLM `ContentBlock` union or a dashboard-only `ChatDisplayBlock` layered over stored messages?
2. Should chart payloads store raw rounded values forever, or store only a reference to a run plus a cached visual summary?
3. Should expanded inline cards open as bottom sheets on phone and popovers on desktop, or always route to canonical detail?
4. Should the model be allowed to request a chart block by tool name, or should only backend tools construct chart blocks after successful domain calls?

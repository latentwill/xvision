# Handoff: XVN Trading Chart Designs

## Overview

Five chart concepts for the xvision (xvn) trading platform, presented on a single
design canvas at **`source/XVN Chart Designs.html`**. Three are full dashboard
frames (sidebar + topbar + KPIs + chart pane); two are chart-only frames meant
to be embedded inside the larger product shell.

| # | Name | Frame | Purpose |
|---|---|---|---|
| 01 | Dark Minimal Strategy Dashboard | Full chrome, 1440×900 | Default xvn dashboard — multi-strategy equity overlay, drawdown, monthly returns heatmap |
| 02 | Comparison AB (Scalable) | Full chrome, 1440×900 | Compare 2–12 strategies side-by-side. Auto-wraps card grid. |
| 03 | AI Annotation Chart | Chart-only, 1200×680 | Single-asset candlestick view with floating AI callouts anchored to specific candles + collapsible insight log |
| 04 | Liquidation Heatmap | Chart-only, 1200×680 | Bookmap-style horizontal heat bands behind candles + top liquidation level rail |
| 05 | Gradient Warm Dashboard | Full chrome, 1440×900 | "Hero" dashboard variant — radial gold/ember background washes, gradient-filled lead equity, performance radar, market context |

## About the Design Files

The files in `source/` are **design references** — interactive HTML prototypes
showing the intended look, layout, interactions, and data densities. They are
**not production code to copy directly**. The job is to recreate these designs
in xvision's existing frontend environment (`frontend/web/`) using:

- **KlineCharts** for candlestick views (Charts 03, 04)
- **uPlot** for line/equity views (Charts 01, 02, 05)
- The existing xvn React component patterns, theme tokens, and routing

The custom canvas candle component in `source/charts/candle-chart.jsx` exists
**only because** KlineCharts and lightweight-charts didn't render inside the
preview sandbox. In production, **use KlineCharts** for everything candle —
the design assumes that. The custom component is just a faithful visual stand-in
so the surrounding callout/heatband layout can be evaluated.

## Fidelity

**High-fidelity.** Final colors, typography, spacing, and the structure of every
panel are designed. Recreate pixel-faithfully against xvn's design tokens.
Minor adjustments are fine where xvn's existing components already cover a
piece (e.g. shared `<Button>` or `<Pill>` should be used as-is).

---

## Design Tokens

These are the canonical values used across all five charts. Mirror them into
xvn's theme file (likely `frontend/web/src/theme.ts` or equivalent). Full list
in `source/charts/chart-theme.css`.

### Background / surface

| Token | Hex | Usage |
|---|---|---|
| `--bg` | `#000000` | App background, chart pane bg |
| `--bg-deep` | `#000000` | Design canvas / outside-app void |
| `--surface-sidebar` | `#000000` | Left navigation rail |
| `--surface-card` | `#0A0A0A` | Default card / panel surface |
| `--surface-elev` | `#0E0E0E` | Elevated row inside a card (e.g. insight log entry) |
| `--surface-panel` | `#121212` | Slightly raised UI element |
| `--surface-hover` | `#121212` | Row hover |

### Border

| Token | Hex | Usage |
|---|---|---|
| `--border` | `#1A1A1A` | Card/panel border |
| `--border-strong` | `#2A2A2A` | Button border |
| `--border-soft` | `#141414` | Internal separator |
| `--border-hair` | `rgba(255, 255, 255, 0.06)` | Hairline glass border |

### Text

| Token | Hex | Usage |
|---|---|---|
| `--text` | `#FFFFFF` | Primary cream-white |
| `--text-2` | `#9CA3AF` | Secondary |
| `--text-3` | `#5F6670` | Tertiary / labels / axis text |
| `--text-4` | `#3A3F47` | Disabled / faint marks |

### Palette (warm-led with cool counterpoint)

| Token | Hex | Notes |
|---|---|---|
| `--gold` | `#00E676` | **Primary accent**. Lead strategy, EMA line, "live" pulse, hero highlights |
| `--amber` | `#38BDF8` | Gradient top stop, 2nd-tier accent |
| `--bronze` | `#5EEAD4` | Quiet warm |
| `--ember` | `#FB923C` | Gradient mid-stop |
| `--copper` | `#F472B6` | Deep warm |
| `--plum` | `#C084FC` | Cool counterpoint for one strategy |
| `--teal` | `#22D3EE` | Cool counterpoint #2 |
| `--info` | `#5FA8FF` | Informational chip |
| `--warn` | `#FFB020` | Warning |
| `--danger` | `#FF4D4D` | Downside / drawdown / down-candle |

### Candle colors (intentionally **not** brand colors)

The candles must read as standard market semantics. Use these, not the gold/copper palette:

| Token | Hex | Usage |
|---|---|---|
| `--candle-up` | `#3FAE6B` | Up bar/wick |
| `--candle-down` | `#FF4D4D` | Down bar/wick |

### Strategy color rotation

Eight strategies with deliberately spread hues for legibility on multi-line
charts. Defined in `source/charts/chart-data.js` (`XVN_STRATEGIES`):

```
1. Fibonacci Golden Cross   #00E676  gold    (lead — heaviest weight)
2. EMA Pullback              #FBBF24  cream
3. Breakout Retest           #FB923C  orange
4. Momentum Swing            #A78BFA  plum
5. Mean Reversion AI         #22D3EE  teal
6. Volatility Scalper        #D67B5C  coral
7. Liquidation Hunter        #8C6024  bronze
8. BTC Buy & Hold            #5F6670  gray  (benchmark — dashed)
```

### Liquidation heat ramp (Bookmap-style, red — NOT gold)

| Heat | Hex | Alpha |
|---|---|---|
| Scorching (>0.75) | `#FF6B5C` | 0.48 |
| Hot (0.55–0.75) | `#E04A3A` | 0.42 |
| Warm (0.35–0.55) | `#A93428` | 0.36 |
| Cool (0.15–0.35) | `#6A2A22` | 0.30 |
| Cold (<0.15) | `#3A1E1A` | 0.22 |

### Spacing & radius

| Token | Value |
|---|---|
| `--radius-card` | `6px` |
| `--radius-sm` | `4px` |
| Card padding | `14–18px` |
| Section header padding | `12–14px 14–18px` |
| KPI grid gap | `12px` |
| Outer dashboard padding | `28px 32px 24px` |

### Typography

Three families. Load via Google Fonts (already done in `XVN Chart Designs.html`).

| Family | Usage | Weights |
|---|---|---|
| **Cormorant Garamond** | Display headlines (`<h1>`, section `<h2>`, KPI values, callout titles, last-price tags inside the data display) | 500 regular, 500 italic |
| **Inter** | Body, navigation, button labels, captions | 400, 500, 600 |
| **JetBrains Mono** | All numerics (axis labels, KPI feet, pill values, OHLC tooltips, %s, timestamps) | 400, 500 |

Sizes:

| Element | Size / Line / Letter-spacing |
|---|---|
| Display `h1` (dashboard title) | 30–34px / 1.1 / -0.015em |
| Section `h2` | 17–20px / 1.2 / -0.01em |
| KPI value | 30–32px / 1.05 / -0.015em (Cormorant) |
| Card title (strategy) | 15.5–17px / 1.1 (Cormorant) |
| Body | 12.5–13px / 1.45 (Inter) |
| `.caps` micro-label | 10–10.5px tracking 0.10–0.12em UPPERCASE (Inter) |
| Numeric (table, axis, KPI foot) | 10.5–14px (JetBrains Mono, tabular-nums) |

A `.caps` helper exists for the tracked-uppercase eyebrow labels — use it for every micro-label (`TOTAL RETURN`, `LAST 6H`, `BTC · SPOT`, etc.).

---

## Library Wiring

### uPlot (Charts 01, 02, 05)

Used for any line/area equity, drawdown, and small sparkline chart. All shared helpers in `source/charts/chart-helpers.js`:

- **`window.xvnUplotTheme`** — global axis stroke / grid color / font defaults
- **`window.xvnAxes(opts)`** — returns a `[xAxis, yAxis]` config with xvn styling. Pass `{ yValues: (u, vals) => ... }` to customise percent vs. integer formatting.
- **`window.xvnLine(label, color, opts)`** — series factory. `opts.dashed=true` for benchmark lines.
- **`window.xvnAreaFill(seriesIdx, topColor, bottomAlpha)`** — uPlot plugin: gradient area fill under a series.
- **`window.xvnLastDot(seriesIdx, color)`** — uPlot plugin: halo+dot on the last data point of a series. Used for the "where we are now" mark.
- **`window.useUplot(buildOpts, deps)`** — React hook. `buildOpts(parent)` returns the uPlot opts object (including a `data` field). Mounts with a ResizeObserver and rebuilds on width changes.

Production wiring: install `uplot` from npm (`npm i uplot`), import as a module, drop the global `window.xvn*` helpers into a `src/charts/uplot/` module.

### KlineCharts (Charts 03, 04)

The design assumes KlineCharts for candle rendering. Install `klinecharts` from npm and theme it with the styles spec defined in `source/charts/chart-helpers.js` under `window.xvnKLineTheme` (already in the spec'd format `{ candle, grid, xAxis, yAxis, crosshair, indicator }`).

Key candle settings:
```js
candle: {
  type: 'candle_solid',
  bar: {
    upColor: '#3FAE6B', downColor: '#FF4D4D',
    upBorderColor: '#3FAE6B', downBorderColor: '#FF4D4D',
    upWickColor: '#3FAE6B', downWickColor: '#FF4D4D',
    noChangeColor: '#5F6670',
  },
  priceMark: {
    last: { upColor: '#3FAE6B', downColor: '#FF4D4D',
      line: { show: true, style: 'dashed', dashedValue: [3,3], size: 1 },
      text: { color: '#000000', backgroundColor: 'matches direction' },
    },
  },
}
```

EMA(21) overlay on the candle pane is part of the default look (gold line).
Add via `chart.createIndicator({ name: 'EMA', calcParams: [21] }, false, { id: 'candle_pane' })`.

**Heads up:** during this design pass, KlineCharts (9.5 and 9.8.10) did not draw
inside our preview iframe sandbox — likely a `ResizeObserver` or DPR detection
quirk specific to that sandbox. The custom canvas component is a stand-in. In
the real frontend, KlineCharts should work fine; use it.

### Data shapes

Equity series (uPlot expects parallel arrays, `time` in seconds since epoch):
```ts
{
  time: Float64Array,          // unix seconds, one per data point
  series: {                    // keyed by strategy id
    [strategyId: string]: Float64Array   // % return, baselined at 0
  }
}
```

Drawdown series: same length as equity, all values ≤ 0 (peak-to-trough, %).

Candle (production — KlineCharts wants `timestamp` in **ms**):
```ts
{
  timestamp: number,   // ms since epoch
  open: number,
  high: number,
  low: number,
  close: number,
  volume: number,
}
```

Annotations (Chart 03):
```ts
{
  idx: number,                                 // candle index this is anchored to
  side: 'top' | 'bottom',                      // which row of callouts
  type: 'PATTERN'|'FLOW'|'RISK'|'REVERSION'|'STRUCTURE',
  title: string,
  body: string,                                // 12–25 words
  conf: number,                                // 0..1
  action: 'WATCH'|'LONG'|'SHORT'|'CAUTION',
  danger?: boolean,                            // tints callout red
}
```

Liquidation level (Chart 04):
```ts
{
  price: number,        // BTC price
  heat: number,         // 0..1
  notional: number,     // millions USD
  side: 'long' | 'short',
}
```

---

## Chart Specifications

### 01 — Dark Minimal Strategy Dashboard (`source/charts/chart-dark-minimal.jsx`)

**Layout** — `1440×900`, `grid-template-columns: 200px 1fr` (sidebar / main).

- **Sidebar (200px)** — `--surface-sidebar`. Nav items use a 2px left-edge accent bar that turns gold when active; active row also gets a `rgba(0, 230, 118, 0.06)` background tint. User row pinned to bottom with avatar + paper/localhost status.
- **Topbar** — Page eyebrow (`.caps`) + Cormorant `h1` "Strategy Comparison" + Cormorant italic tagline. Right side: `paper · localhost` pill (gold dot), timeframe toggle row (`1D 1W 1M 3M YTD 1Y ALL`, `ALL` active), `⤓ Export` ghost button.
- **KPI row** — 5 equal cards: Total Return / Sharpe / Max Drawdown (red) / Win Rate / Profit Factor. Value uses Cormorant 32px; label is `.caps` 10px; foot is JetBrains Mono 11px in `--text-3`.
- **Equity Curve card** — uPlot multi-line. 5 strategies overlaid; lead (Fibonacci GC) at 1.7px stroke, others at 1.15px; BTC bench is **dashed**. Legend chips in section header, color swatches as 10×2px bars. Plugin: `xvnLastDot` on the lead series.
- **Bottom row** — 2 cards:
  - **Drawdown · Fibonacci GC** card: red line + red area fill on uPlot; footer with Max DD, Avg DD, Duration, Recovery (mono 14px).
  - **Monthly Returns** heatmap: 17 months × 5 strategies. Cell color is `rgba(212,165,71, α)` for positive, `rgba(200,68,58, α)` for negative; α scales with magnitude (0.10–0.65). Legend bar at bottom: `-15% ←→ +15%`.

### 02 — Comparison AB · Scalable (`source/charts/chart-comparison-ab.jsx`)

**Same shell as 01.** The body is:

- **Topbar headline** — Cormorant 30px "`N` strategies, one frame" (live count).
- **Hero overlay card** — uPlot, all selected strategies. Legend chips include the strategy's return as a JetBrains Mono micro-stat in `--text-3`.
- **Roster pill rail** — Every available strategy as a pill. On-state has `${color}14` background + `${color}55` border. Off pills are muted. Click toggles inclusion. Minimum 2 strategies (× remove button disabled at 2).
- **Strategy card grid** — auto-wraps based on count:
  - `n ≤ 2` → 2 columns
  - `n ≤ 4` → 4 columns
  - `n ≤ 6` → 3 columns
  - `n ≥ 7` → 4 columns
- Each card: head (color dot + Cormorant name + `.caps` kind line + LEAD badge if first, `×` button if removable) → mini equity sparkline (uPlot, area-filled in that strategy's color) → 2×2 metrics (Return / Sharpe / MaxDD / Win) → indicator chip strip (`RSI 56`, `MACD ↑`, `EMA`, `Fib · 0.618`).
- Lead card has subtle gold gradient backdrop (`linear-gradient(180deg, rgba(0, 230, 118, 0.04), var(--surface-card) 38%)`) and gold-tinted border + a 1px gold gradient line across its top.

### 03 — AI Annotation Chart (`source/charts/chart-ai-annotation.jsx`)

**Layout** — `1200×680`, `grid-template-columns: 1fr 280px` (chart / insight log), animated to `1fr 36px` when log is collapsed.

- **Header** — xvn lozenge (Cormorant italic 22px) · divider · symbol/timeframe eyebrow + price (Cormorant 20px) + 24h change (green/red mono). Right: animated "AI Engine · live" pill (gold dot with `aiPulse` keyframe 1.8s ease-out — `scale(1, opacity:0.7) → scale(3.4, opacity:0)`), `model · xvn-annot-v3` ghost pill, filter toggle row (Patterns / Risk / Flow / All).
- **Chart pane** — KlineCharts candle pane filling the column, EMA(21) gold overlay, volume sub-pane (~18% of height) with green/red bars. Last-price tag in `--candle-up` or `--candle-down`. Crosshair shows OHLC label in the top-left corner of the candle area.
- **Annotation overlay** — Five callouts in two rows: top row at `y=24`, bottom row at `y=H-180`. Callouts are spread evenly across the chart width to avoid horizontal overlap; their connector lines (SVG, dashed 3 3) point from the nearest corner of the callout to the actual candle index. A small ring + dot marks the anchor on the candle (`r=6` ring at 55% opacity, `r=2.4` solid dot in gold or red).
- **Callout card** — `~210px` wide. Border `rgba(0, 230, 118, 0.32)` (or red for `danger`). Inner: `.callout-head` eyebrow (type + `conf 74%`), Cormorant title 14px, body 11.5px in `--text-2`, foot row showing `idx · 22` + action verb in accent color.
- **Insight log** — Right-side panel, list of cards: title row (Cormorant 14px / Mono timestamp), body (11.5px), footer with category pill + confidence. Left edge has a 2px accent bar (gold or red). Collapse button (`›`) shrinks the panel to a 36px rail showing vertical "Insight Log · N events" text + 6 colored dots; click `‹` to reopen.
- **Footer** — JetBrains Mono 10.5px status line: `EMA(21) · candle_pane · drag to pan · callouts follow candles` / `5 annotations · 6 indicators streaming · 1.8ms tick` / `xvn-annot-v3 · build a7c2f1`.

### 04 — Liquidation Heatmap (`source/charts/chart-liquidation-heatmap.jsx`)

**Layout** — `1200×680`, `grid-template-columns: 1fr 280px`.

- **Header** — same shell as 03 but title says `LIQUIDATION HEATMAP`. Right: timeframe pills (`1h 4h 1d 1w`), and two legend pills: `● longs at risk` (`#FF6B5C` dot), `● shorts at risk` (`#A93428` dot).
- **Chart pane** — KlineCharts candles + EMA(21) at base. Heat-band overlay layer **above** the candles (`z-index: 2`), `pointer-events: none`. Each band:
  - Height scales with heat: `6px + heat * 18px`.
  - Background is a horizontal gradient: `transparent 0% → fillColor@α 18% → fillColor@1.4α 60% → fillColor@0.6α 92% → transparent 100%`. Gives a smooth glow with darkest area in the middle of the chart.
  - Color from the Bookmap red ramp (see Design Tokens). **No gold here.**
- **Top-3 price tags** — Inline absolute-positioned badges at the right edge of the chart pane, color `#FF6B5C`, format `$210M · $63391`.
- **Right rail** —
  - "Top Liquidation Levels" — table of top 10. Cols: Price (mono), heat bar (gradient sized by heat), USD notional (color by side).
  - "Cascade Analytics" — 2×2 metric grid: Long Exposure (`#FF6B5C`), Short Exposure (`#A93428`), Nearest Wall, Cascade Risk.
  - "Heat Scale" legend bar — horizontal gradient from cold to scorching matching the heat ramp.

### 05 — Gradient Warm Dashboard (`source/charts/chart-gradient-dashboard.jsx`)

**The hero / showpiece variant.** Same `1440×900` shell as 01 but with:

- **Aura background washes** — 3 large radial blurs absolutely positioned behind the content layer (`opacity 0.30–0.55`, `filter: blur(20px)`):
  - 520×520 gold/ember wash, top-left of main area
  - 680×680 ember/plum wash, bottom-right (-260, -100)
  - 380×380 amber wash, top-right
- **Grain overlay** — `repeating-linear-gradient(0deg, rgba(255, 255, 255, 0.012) 0 1px, transparent 1px 3px)` at `opacity: 0.5`, full-bleed, behind content.
- **Glass cards** — `.glass` utility: `linear-gradient(180deg, rgba(34,30,20,0.62), rgba(20,18,14,0.78))`, `border: 1px solid rgba(255, 255, 255, 0.07)`, `backdrop-filter: blur(8px)`, subtle `inset` highlight `0 1px 0 rgba(255,255,255,0.02)`.
- **Topbar headline** — Crypto · Strategy Hub eyebrow (gold tracked uppercase). Headline `The [Golden Cross] is up [82.41%]` — "Golden Cross" is Cormorant italic with a linear-gradient text fill `90deg, #38BDF8 0% → #00E676 35% → #FB923C 80%`. "82.41%" is JetBrains Mono in gold at 26px. Whole headline is on one line (`white-space: nowrap`, `font-size: 30px`).
- **KPI row** — 5 glass cards. The Total Return card has a radial gold glow in the top-right corner (`radial-gradient(closest-side, rgba(0, 230, 118, 0.30), transparent 70%)`, 120×120, offset `-30 -30 auto auto`).
- **Hero equity** — uPlot with a **multi-stop warm gradient area fill** on the lead series (custom plugin):
  ```
  0.00 → rgba(0, 230, 118, 0.42)   gold (top)
  0.25 → rgba(229,184,106,0.30)  amber
  0.55 → rgba(193,106,58,0.18)   ember
  0.85 → rgba(140,74,46,0.06)    copper
  1.00 → rgba(0,0,0,0)           fade
  ```
  Plus a horizontal sheen highlight in the top 40% (`rgba(255, 255, 255, 0.06) → 0`) for a glass-on-curve effect. `xvnLastDot` halo on the lead.
- **Performance Radar** — pure SVG, 260×220, 6 axes (Return, Sharpe, Stability, Win Rate, Consistency, Drawdown). Renders top 3 strategies as overlaid polygons (`fillOpacity 0.10`, stroke 1.4px in strategy color, vertex dots `r=2.2`). 4 ring polygons at 25/50/75/100%. Axis labels at 1.18× radius, tracked uppercase.
- **Drawdown Comparison** — uPlot showing top 3 strategies' underwater curves overlaid, area fill on the lead in gold-tinted red.
- **Market Context** card — 2×2 grid of BTC stats: Price (Cormorant 24px), Funding (gold for positive), Open Interest, Liq 24h (copper). Below: REGIME chip row (`BULL · 62%`, `SIDEWAYS · 22%`, `BEAR · 9%`, `HIGH VOL · 7%`).

---

## Interactions & Behavior

- **Pan / zoom** on candle charts uses KlineCharts native handling — preserve it; the callout overlay and heat bands re-anchor through KlineCharts's `convertToPixel` API and a subscription to `onVisibleRangeChange`.
- **Cursor sync** on multi-line equity charts via uPlot cursor sync key (`'hero'`, `'cmp'`).
- **Strategy roster** in Chart 02 is fully interactive — clicking a roster pill adds/removes that strategy from the hero overlay AND the card grid; grid column count reflows.
- **Insight log collapse** (Chart 03) animates via CSS `grid-template-columns` transition, 200ms ease. State is local to the chart.
- **Timeframe pills** are visually styled but inert in the prototype; in production they should drive the chart's time range.
- **Tweaks** are not built in. If you want them, fold them in via the existing xvn tweaks pattern.

## State Management

Per-chart state is minimal — most charts derive everything from prop data:

- **Chart 02** — `selectedIds: string[]` (controlled list of strategy IDs included in the overlay + grid).
- **Chart 03** — `logOpen: boolean` (collapsed/expanded right rail); a `recompute` counter that fires when the candle layout settles, so the callout overlay can re-anchor.
- **Chart 04** — same `recompute` pattern to re-position heat bands when KlineCharts re-lays-out.

In production, the higher-level page should own:
- Selected symbol / timeframe
- Selected strategies (which strategies are included in any comparison chart)
- Selected timeframe / date range
- Whether AI annotations are visible / filtered by type
- Backtest run id (so the equity series is the right one)

## Responsive Behavior

These prototypes are **desktop-first**, designed at 1440 and 1200 widths. Mobile
variants are a separate workstream — see the follow-up handoff once v2/v3 are
selected.

Charts are size-aware: every chart instance uses a ResizeObserver and the
chart libs' native `resize()` to track its container. Don't apply `height: 100%`
inside `overflow: auto/scroll` containers — these are designed as fixed-size
panes within fixed-size dashboard regions.

## Assets

- All five charts use only system / web-loaded resources — no images.
- Google Fonts: Cormorant Garamond, Inter, JetBrains Mono.
- uPlot CSS: `https://unpkg.com/uplot@1.6.30/dist/uPlot.min.css` (or npm).

## Files in the Handoff

```
design_handoff_charts/
├── README.md                  (this file)
└── source/
    ├── XVN Chart Designs.html (entry — opens all 5 on the design canvas)
    └── charts/
        ├── chart-theme.css           (design tokens + utility classes)
        ├── chart-data.js             (synthetic data generators + strategy roster)
        ├── chart-helpers.js          (uPlot helpers, plugins, useUplot hook, KlineCharts theme)
        ├── candle-chart.jsx          (custom canvas candle component — REPLACE with KlineCharts in production)
        ├── chart-dark-minimal.jsx    (Chart 01)
        ├── chart-comparison-ab.jsx   (Chart 02)
        ├── chart-ai-annotation.jsx   (Chart 03)
        ├── chart-liquidation-heatmap.jsx (Chart 04)
        └── chart-gradient-dashboard.jsx  (Chart 05)
```

## Open Questions / Next Steps

1. **Which directions stay?** Narrow to v2/v3 (1–3 favorites) before doing mobile.
2. **Tweaks** — if a chart should expose runtime knobs (e.g. compare lookback, annotation filters, palette), wire them through xvn's existing tweak system.
3. **Animation pass** — entrances (last-dot fade-in, heat-band shimmer) are not currently designed; flag for a follow-up motion pass if desired.
4. **Empty / loading / error states** — not designed. Use xvn's existing skeleton/empty patterns.

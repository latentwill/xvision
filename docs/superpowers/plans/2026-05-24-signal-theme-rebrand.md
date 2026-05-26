# Signal Theme Rebrand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace xvision's warm gold/serif "folio-dark" identity with the **Signal** theme (pure-black surfaces, Signal-green `#00E676`, Geist + Geist Mono typography), collapse the theme set to **Signal Dark + a new Signal Light**, redesign the Settings theme picker, rebrand all charts, and rebuild the Eval Run Detail page chrome per the design handoff.

**Architecture:** Tokens live in two coupled places — `src/theme/themes.ts` (TS source of truth, consumed by chart code + the theme picker) and `src/styles/tokens.css` (the `:root` default + `[data-theme]` blocks the browser actually applies). Both must change together. Tailwind (`tailwind.config.ts`) maps utility classes onto the `--gold`/`--text`/`--surface-*` CSS variables and onto font families; the `--gold` token name is **kept** (its value becomes green) so the hundreds of `text-gold`/`bg-gold`/`var(--gold)` consumers need no edits. Theme IDs change from `light|folio-dark|black` to `light|dark`, with a one-time localStorage migration. The Eval Run Detail page gets new components (TopBar, MetaChip, PhaseChip, ActionPill, DecisionTimeline density strip) translated from the HTML/JSX design references.

**Tech Stack:** React + Vite + TypeScript SPA (`frontend/web/`), Tailwind CSS (CSS-variable-backed), `lightweight-charts` + uPlot (`chart2`), Vitest, Fontsource + Google Fonts. No Rust/backend changes.

**Design source of truth:** `docs/design/themes/signaltheme_rebrand_XVN_extracted/design_handoff_signal/` — `README.md` (token table + per-component anatomy + acceptance checklist), `css/signal.css` + `css/signal-mobile.css` (executable token spec), `charts/chart-theme.css` (chart palette), `eval-run-detail/app.jsx` (the full Eval Run Detail composition: TopBar, BrandMark, MetaChip, SummaryCard, DecisionsTable, DecisionTimeline, ActionPill, PhaseChip, Meta card, ReviewPanel), `screens/*.jsx`, `components/*.jsx`, `references/*.html`.

---

## Locked design decisions (do not relitigate)

1. **Theme set collapses to two.** `ThemePreference = "auto" | "light" | "dark"`; `ResolvedTheme = "light" | "dark"`. Default dark theme is Signal. `folio-dark` and `black` are removed. Old stored preferences (`"folio-dark"`, `"black"`) migrate to `"dark"`; `"light"`/`"auto"` are preserved. The separate "dark theme memory" (`THEME_DARK_KEY`) concept is removed — there is exactly one dark theme now.
2. **Light mode = "cool white / clinical."** Designed in this plan (the handoff ships dark only). Pure-white cards on a near-white cool-gray bg, cool-gray neutrals, darkened green accent (`#00A15C`) so accent fills/borders read on white. Accent-as-text uses the darker `--gold-soft` (`#007A48`).
3. **`--gold` token name stays.** Value becomes green. Never rename `--gold` → `--green`; it would touch every consumer.
4. **No italic anywhere.** Geist conveys emphasis via weight/size/color.
5. **No popups** (CLAUDE.md hard rule). Everything new docks/rails/inline-expands. The Eval Run Detail redesign introduces no overlays.

---

## Final token values (authoritative — copy these exactly)

### Signal Dark (`[data-theme="dark"]` and `:root` default)

```
--bg:               #000000
--surface-sidebar:  #000000
--surface-card:     #0A0A0A
--surface-elev:     #0E0E0E
--surface-panel:    #121212
--surface-hover:    rgba(255,255,255,0.04)
--border:           #1A1A1A
--border-strong:    #2A2A2A
--border-soft:      #141414
--text:             #FFFFFF
--text-2:           #9CA3AF
--text-3:           #5F6670
--text-4:           #3A3F47
--gold:             #00E676
--gold-soft:        #00B85F
--gold-bg:          rgba(0,230,118,0.10)
--gold-bg-strong:   rgba(0,230,118,0.18)
--warn:             #FFB020
--danger:           #FF4D4D
--info:             #5FA8FF
--radius-card:      6px
--radius-sm:        4px
```

### Signal Light (`[data-theme="light"]`) — designed here

```
--bg:               #F7F8FA
--surface-sidebar:  #FFFFFF
--surface-card:     #FFFFFF
--surface-elev:     #F2F4F7
--surface-panel:    #EAEDF1
--surface-hover:    rgba(0,0,0,0.04)
--border:           #E3E6EA
--border-strong:    #CBD1D8
--border-soft:      #EDEFF2
--text:             #0B0E11
--text-2:           #4A5560
--text-3:           #6B7682
--text-4:           #9AA4AE
--gold:             #00A15C
--gold-soft:        #007A48
--gold-bg:          rgba(0,161,92,0.10)
--gold-bg-strong:   rgba(0,161,92,0.16)
--warn:             #B45309
--danger:           #D92D20
--info:             #1D6FD6
--radius-card:      6px
--radius-sm:        4px
```

> **Light-mode contrast note:** `#00A15C` on white is ~3.4:1 — fine for fills, borders, dots, and large/bold labels (UI 3:1 bar), below AA for small body text. Where the accent is used as *text* (e.g. positive PnL numerals, links), use `--gold-soft` (`#007A48`, ~5:1). This split is already how the dark theme behaves (`--gold-soft` for label tone in MetaChip).

### Chart palette — Signal multi-hue rotation (README §"Chart palette")

```
primary/up: #00E676   (dark) / #00A15C (light)
down:       #FF4D4D   (dark) / #D92D20 (light)
sky:        #38BDF8   (dark) / #0284C7 (light)
mint:       #5EEAD4   (dark) / #0D9488 (light)
yellow:     #FBBF24   (dark) / #CA8A04 (light)
orange:     #FB923C   (dark) / #EA580C (light)
pink:       #F472B6   (dark) / #DB2777 (light)
violet:     #A78BFA   (dark) / #7C3AED (light)
cyan:       #22D3EE   (dark) / #0891B2 (light)
```

Positive/negative gradient fills: positive `rgba(0,230,118,α)` (dark) / `rgba(0,161,92,α)` (light); negative `rgba(255,77,77,α)` (dark) / `rgba(217,45,32,α)` (light).

### Typography

```
--font-display / --font-sans / --font-brand : 'Geist', sans-serif
--font-mono                                  : 'Geist Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace
```

Google Fonts link (replaces the Cormorant/Inter/JetBrains link):
```
https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700;800&family=Geist+Mono:wght@400;500;600;700&display=swap
```

---

## File structure

**Phase A — Foundation (token + type + font core). Must land and pass typecheck before any Phase B work.**
- Modify `frontend/web/src/theme/themes.ts` — collapse to 2 themes; Signal Dark + Signal Light token values; rebrand all `chart`/`chart2` blocks; Geist typography constants; new `themePreferenceOptions`; `coerce*`/`resolveTheme` + migration.
- Modify `frontend/web/src/styles/tokens.css` — `:root` default = Signal Dark; `[data-theme="dark"]` + `[data-theme="light"]` blocks; delete `folio-dark`/`black` blocks.
- Modify `frontend/web/src/theme/useTheme.ts` — drop `darkTheme`/`THEME_DARK_KEY`; simplify `setDarkTheme`→`setPreference("dark")`; run migration on read.
- Modify `frontend/web/src/theme/ThemeProvider.tsx` — already generic; verify `data-theme` + `metaColor` flow works with new IDs.
- Modify `frontend/web/tailwind.config.ts` — `fontFamily.sans`/`serif` → Geist, `mono` → Geist Mono.
- Modify `frontend/web/index.html` — Geist Google Fonts link; `theme-color` meta → `#000000`.
- Modify `frontend/web/src/main.tsx` — swap `@fontsource` imports to Geist.
- Modify `frontend/web/package.json` — add `@fontsource/geist-sans`, `@fontsource/geist-mono`; remove Cormorant/Inter/JetBrains fontsource deps.
- Modify `frontend/web/src/styles/globals.css` — `.serif-i` → Geist weight (no italic); body font-family → Geist; `.caps`/`.dec-pill`/`.dec-pos` font-family → Geist Mono; `::selection` unchanged (uses `--gold-bg-strong`).
- Modify theme unit tests (see Task A8).

**Phase B — Parallelizable after A commits (disjoint file sets):**
- **B1 Settings picker:** `frontend/web/src/routes/settings/general.tsx`.
- **B2 Italic + serif sweep:** every `*.tsx` using `font-serif`, `italic`, or `.serif-i` *except* files owned by B1/B4 (those tracks fix their own). Includes `Sidebar.tsx`, `MobileDrawer.tsx`, review/docs/pull-quote components.
- **B3 Chart hardcoded-hex sweep:** chart components/adapters referencing literal warm hexes (`#2a2618`, `#3a3322`, `#d4a547`, `#b8862e`, `#f0c75e`, `#8a5f16`) or `'Inter'`/`'JetBrains Mono'` axis fonts.
- **B4 Eval Run Detail redesign:** `frontend/web/src/routes/eval-runs-detail.tsx` + new components under `frontend/web/src/components/eval-detail/` (or the established eval components dir). Owns its own italic removal.
- **B5 Mobile + scrollbar polish:** mobile components + verify themed green scrollbars resolve via `--gold`.

**Phase C — Verification & evidence (Task C1).**

---

## Phase A — Foundation

### Task A1: Rewrite theme constants & chart palette in `themes.ts` (shared blocks)

**Files:**
- Modify: `frontend/web/src/theme/themes.ts`

- [ ] **Step 1: Replace the type aliases at the top of the file**

Replace lines 1–3:
```ts
export type ThemePreference = "auto" | "light" | "folio-dark" | "black";
export type ResolvedTheme = "light" | "folio-dark" | "black";
type ThemeMode = "light" | "dark";
export type SystemTheme = "light" | "dark";
```
with:
```ts
export type ThemePreference = "auto" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
type ThemeMode = "light" | "dark";
export type SystemTheme = "light" | "dark";
```

- [ ] **Step 2: Replace the shared chart constant blocks**

Replace `CHART2_STRATEGY_ROTATION`, `CHART2_HEAT_RAMP`, `CHART2_TYPOGRAPHY`, `CHART2_RADIUS`, `CHART2_WARM_DARK` (current lines ~186–229) with the Signal versions. The `warm` property name on the type is kept (consumers read `chart2.warm.gold`), but values are now the Signal multi-hue rotation:

```ts
// Signal multi-hue rotation. Stable across themes; chosen mid-tone so each
// hue survives both pure-black and near-white backgrounds. The lead (`color`)
// of each strategy is Signal green; the rest are the chart palette hues.
export const CHART2_STRATEGY_ROTATION: Chart2StrategyRotationEntry[] = [
  { id: "fib", name: "Fibonacci Golden Cross", short: "Fib · GC", color: "#00C16A", kind: "Trend" },
  { id: "ema", name: "EMA Pullback", short: "EMA · 50/200", color: "#5EEAD4", kind: "Trend" },
  { id: "brk", name: "Breakout Retest", short: "BRK · 4h", color: "#FB923C", kind: "Momentum" },
  { id: "msw", name: "Momentum Swing", short: "MSW · 1d", color: "#A78BFA", kind: "Momentum" },
  { id: "mvr", name: "Mean Reversion AI", short: "MVR · 15m", color: "#22D3EE", kind: "Reversion" },
  { id: "vsc", name: "Volatility Scalper", short: "VSC · 5m", color: "#F472B6", kind: "Vol" },
  { id: "lqh", name: "Liquidation Hunter", short: "LQH · 1h", color: "#E0B341", kind: "Vol" },
  { id: "btc", name: "BTC Buy & Hold", short: "BTC · HOLD", color: "#6B7682", kind: "Bench", dashed: true },
];

// Heat ramp = loss/intensity ramp (monthly-returns "hot" cells). Negative
// returns ride this red ramp; positive returns use the green fill in the
// component. Unchanged hues — this is a semantic loss ramp, not the brand.
export const CHART2_HEAT_RAMP: Chart2HeatRamp = {
  scorching: { color: "#FF6B5C", alpha: 0.48 },
  hot: { color: "#E04A3A", alpha: 0.42 },
  warm: { color: "#A93428", alpha: 0.36 },
  cool: { color: "#6A2A22", alpha: 0.30 },
  cold: { color: "#3A1E1A", alpha: 0.22 },
};

export const CHART2_TYPOGRAPHY: Chart2Typography = {
  fontSerif: "'Geist', sans-serif",
  fontSans: "'Geist', sans-serif",
  fontMono: "'Geist Mono', ui-monospace, monospace",
};

export const CHART2_RADIUS: Chart2Radius = {
  card: "6px",
  sm: "4px",
};

// Signal chart palette — dark surfaces. `warm` is a legacy property name
// kept for consumer compatibility; values are the Signal multi-hue rotation.
export const CHART2_SIGNAL_DARK: Chart2WarmPalette = {
  gold: "#00E676",
  amber: "#38BDF8",
  bronze: "#5EEAD4",
  ember: "#FB923C",
  copper: "#F472B6",
  plum: "#A78BFA",
  teal: "#22D3EE",
  info: "#5FA8FF",
  warn: "#FFB020",
  danger: "#FF4D4D",
};

// Signal chart palette — light surfaces (darker, legible on white).
export const CHART2_SIGNAL_LIGHT: Chart2WarmPalette = {
  gold: "#00A15C",
  amber: "#0284C7",
  bronze: "#0D9488",
  ember: "#EA580C",
  copper: "#DB2777",
  plum: "#7C3AED",
  teal: "#0891B2",
  info: "#1D6FD6",
  warn: "#B45309",
  danger: "#D92D20",
};
```

- [ ] **Step 3: Run typecheck — expect failures only in the theme map (next task) and tests**

Run: `cd frontend/web && npm run typecheck`
Expected: errors referencing removed `CHART2_WARM_DARK`, `"folio-dark"`, `"black"` in `themeDefinitions`, `coerce*`, and tests. These are resolved in A2–A8.

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/theme/themes.ts
git commit -m "refactor(theme): Signal chart palette constants + 2-theme type"
```

---

### Task A2: Replace `themeDefinitions` with Signal Dark + Signal Light

**Files:**
- Modify: `frontend/web/src/theme/themes.ts` (the `themeDefinitions` record, current lines ~259–758, and `themePreferenceOptions` ~251–257)

- [ ] **Step 1: Replace `themePreferenceOptions`**

```ts
export const themePreferenceOptions: { value: ThemePreference; label: string }[] =
  [
    { value: "auto", label: "Auto" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ];
```

- [ ] **Step 2: Replace the entire `themeDefinitions` record with exactly two entries**

Use the authoritative token values from the plan header. Full block:

```ts
export const themeDefinitions: Record<ResolvedTheme, ThemeDefinition> = {
  dark: {
    id: "dark",
    label: "Dark",
    mode: "dark",
    metaColor: "#000000",
    swatch: {
      bg: "#000000",
      surface: "#0A0A0A",
      border: "#1A1A1A",
      text: "#FFFFFF",
      accent: "#00E676",
    },
    cssVars: {
      "--bg": "#000000",
      "--surface-sidebar": "#000000",
      "--surface-card": "#0A0A0A",
      "--surface-elev": "#0E0E0E",
      "--surface-panel": "#121212",
      "--surface-hover": "rgba(255,255,255,0.04)",
      "--border": "#1A1A1A",
      "--border-strong": "#2A2A2A",
      "--border-soft": "#141414",
      "--text": "#FFFFFF",
      "--text-2": "#9CA3AF",
      "--text-3": "#5F6670",
      "--text-4": "#3A3F47",
      "--gold": "#00E676",
      "--gold-soft": "#00B85F",
      "--gold-bg": "rgba(0,230,118,0.10)",
      "--gold-bg-strong": "rgba(0,230,118,0.18)",
      "--warn": "#FFB020",
      "--danger": "#FF4D4D",
      "--info": "#5FA8FF",
    },
    chart: {
      background: "#0A0A0A",
      text: "#FFFFFF",
      grid: "#1A1A1A",
      series: {
        sma20: "#7dd3fc", sma30: "#a7f3d0", sma50: "#fbbf24", sma60: "#6ee7b7",
        sma90: "#34d399", sma200: "#f87171",
        ema20: "#a78bfa", ema30: "#bae6fd", ema50: "#fbbf24", ema60: "#7dd3fc",
        ema90: "#38bdf8", ema200: "#f87171",
        bollUpper: "#34d399", bollMiddle: "#94a3b8", bollLower: "#34d399",
        donchianUpper: "#fb923c", donchianLower: "#fb923c",
        equity: "#00E676",
        equityTop: "rgba(0,230,118,0.30)",
        equityBottom: "rgba(0,230,118,0)",
        drawdown: "#FF4D4D",
        drawdownTop: "rgba(255,77,77,0.30)",
        drawdownBottom: "rgba(255,77,77,0)",
        candleUp: "#00E676", candleDown: "#FF4D4D",
        positionLong: "rgba(0,230,118,0.08)", positionShort: "rgba(255,77,77,0.08)",
        markerBuy: "#00E676", markerSell: "#FF4D4D", markerVeto: "#FFB020", markerHold: "#9CA3AF",
        rsi: "#a78bfa", guide: "#2A2A2A",
        macdLine: "#38BDF8", macdSignal: "#FB923C", macdHistogram: "#5F6670", atr: "#fbbf24",
      },
    },
    chart2: {
      surface: {
        bg: "#000000", panelBg: "#0E0E0E", gridStrong: "#2A2A2A", gridSoft: "#141414",
        axisText: "#9CA3AF", axisTick: "#2A2A2A", crosshair: "#00E676",
      },
      candle: {
        up: "#00E676", down: "#FF4D4D", wickUp: "#00E676", wickDown: "#FF4D4D",
        borderUp: "#00B85F", borderDown: "#D43A3A",
      },
      overlay: {
        sma20: "#7dd3fc", sma30: "#a7f3d0", sma50: "#fbbf24", sma60: "#6ee7b7",
        sma90: "#34d399", sma200: "#f87171",
        ema20: "#a78bfa", ema30: "#bae6fd", ema50: "#fbbf24", ema60: "#7dd3fc",
        ema90: "#38bdf8", ema200: "#f87171",
        bollUpper: "#34d399", bollMiddle: "#94a3b8", bollLower: "#34d399",
        donchianUpper: "#fb923c", donchianLower: "#fb923c",
      },
      marker: {
        buy: "#00E676", sell: "#FF4D4D", veto: "#FFB020", hold: "#9CA3AF",
        halo: "rgba(0,230,118,0.18)", textOnAccent: "#001A0A",
      },
      position: {
        longBand: "rgba(0,230,118,0.08)", shortBand: "rgba(255,77,77,0.08)",
        longLine: "#00E676", shortLine: "#FF4D4D",
      },
      panes: {
        equity: "#00E676", equityFillTop: "rgba(0,230,118,0.30)", equityFillBottom: "rgba(0,230,118,0)",
        drawdown: "#FF4D4D", drawdownFillTop: "rgba(255,77,77,0.30)", drawdownFillBottom: "rgba(255,77,77,0)",
        volumeUp: "rgba(0,230,118,0.45)", volumeDown: "rgba(255,77,77,0.45)",
        rsi: "#a78bfa", rsiGuide: "#2A2A2A",
        macdLine: "#38BDF8", macdSignal: "#FB923C", macdHist: "#5F6670", atr: "#fbbf24",
      },
      compare: {
        palette: ["#00E676", "#38BDF8", "#5EEAD4", "#FBBF24", "#FB923C", "#F472B6", "#A78BFA", "#22D3EE"],
      },
      motion: { hoverMs: 80, animMs: 160 },
      density: { axisFont: "11px 'Geist Mono', ui-monospace, monospace", axisGap: 6, paneGap: 8 },
      warm: CHART2_SIGNAL_DARK,
      strategyRotation: CHART2_STRATEGY_ROTATION,
      heatRamp: CHART2_HEAT_RAMP,
      typography: CHART2_TYPOGRAPHY,
      radius: CHART2_RADIUS,
    },
  },
  light: {
    id: "light",
    label: "Light",
    mode: "light",
    metaColor: "#F7F8FA",
    swatch: {
      bg: "#F7F8FA",
      surface: "#FFFFFF",
      border: "#E3E6EA",
      text: "#0B0E11",
      accent: "#00A15C",
    },
    cssVars: {
      "--bg": "#F7F8FA",
      "--surface-sidebar": "#FFFFFF",
      "--surface-card": "#FFFFFF",
      "--surface-elev": "#F2F4F7",
      "--surface-panel": "#EAEDF1",
      "--surface-hover": "rgba(0,0,0,0.04)",
      "--border": "#E3E6EA",
      "--border-strong": "#CBD1D8",
      "--border-soft": "#EDEFF2",
      "--text": "#0B0E11",
      "--text-2": "#4A5560",
      "--text-3": "#6B7682",
      "--text-4": "#9AA4AE",
      "--gold": "#00A15C",
      "--gold-soft": "#007A48",
      "--gold-bg": "rgba(0,161,92,0.10)",
      "--gold-bg-strong": "rgba(0,161,92,0.16)",
      "--warn": "#B45309",
      "--danger": "#D92D20",
      "--info": "#1D6FD6",
    },
    chart: {
      background: "#FFFFFF",
      text: "#0B0E11",
      grid: "#E3E6EA",
      series: {
        sma20: "#0284c7", sma30: "#047857", sma50: "#a16207", sma60: "#059669",
        sma90: "#16a34a", sma200: "#b91c1c",
        ema20: "#7c3aed", ema30: "#0369a1", ema50: "#a16207", ema60: "#0284c7",
        ema90: "#0ea5e9", ema200: "#b91c1c",
        bollUpper: "#15803d", bollMiddle: "#64748b", bollLower: "#15803d",
        donchianUpper: "#c2410c", donchianLower: "#c2410c",
        equity: "#00A15C",
        equityTop: "rgba(0,161,92,0.22)",
        equityBottom: "rgba(0,161,92,0)",
        drawdown: "#D92D20",
        drawdownTop: "rgba(217,45,32,0.20)",
        drawdownBottom: "rgba(217,45,32,0)",
        candleUp: "#16a34a", candleDown: "#D92D20",
        positionLong: "rgba(0,161,92,0.10)", positionShort: "rgba(217,45,32,0.10)",
        markerBuy: "#16a34a", markerSell: "#D92D20", markerVeto: "#ca8a04", markerHold: "#475569",
        rsi: "#7c3aed", guide: "#94a3b8",
        macdLine: "#0284c7", macdSignal: "#ea580c", macdHistogram: "#64748b", atr: "#a16207",
      },
    },
    chart2: {
      surface: {
        bg: "#FFFFFF", panelBg: "#F2F4F7", gridStrong: "#E3E6EA", gridSoft: "#EDEFF2",
        axisText: "#4A5560", axisTick: "#CBD1D8", crosshair: "#00A15C",
      },
      candle: {
        up: "#16a34a", down: "#D92D20", wickUp: "#16a34a", wickDown: "#D92D20",
        borderUp: "#15803d", borderDown: "#b91c1c",
      },
      overlay: {
        sma20: "#0284c7", sma30: "#047857", sma50: "#a16207", sma60: "#059669",
        sma90: "#16a34a", sma200: "#b91c1c",
        ema20: "#7c3aed", ema30: "#0369a1", ema50: "#a16207", ema60: "#0284c7",
        ema90: "#0ea5e9", ema200: "#b91c1c",
        bollUpper: "#15803d", bollMiddle: "#64748b", bollLower: "#15803d",
        donchianUpper: "#c2410c", donchianLower: "#c2410c",
      },
      marker: {
        buy: "#16a34a", sell: "#D92D20", veto: "#ca8a04", hold: "#475569",
        halo: "rgba(0,161,92,0.18)", textOnAccent: "#FFFFFF",
      },
      position: {
        longBand: "rgba(0,161,92,0.10)", shortBand: "rgba(217,45,32,0.10)",
        longLine: "#16a34a", shortLine: "#D92D20",
      },
      panes: {
        equity: "#00A15C", equityFillTop: "rgba(0,161,92,0.22)", equityFillBottom: "rgba(0,161,92,0)",
        drawdown: "#D92D20", drawdownFillTop: "rgba(217,45,32,0.20)", drawdownFillBottom: "rgba(217,45,32,0)",
        volumeUp: "rgba(22,163,74,0.6)", volumeDown: "rgba(217,45,32,0.6)",
        rsi: "#7c3aed", rsiGuide: "#94a3b8",
        macdLine: "#0284c7", macdSignal: "#ea580c", macdHist: "#64748b", atr: "#a16207",
      },
      compare: {
        palette: ["#00A15C", "#0284C7", "#0D9488", "#CA8A04", "#EA580C", "#DB2777", "#7C3AED", "#0891B2"],
      },
      motion: { hoverMs: 80, animMs: 160 },
      density: { axisFont: "11px 'Geist Mono', ui-monospace, monospace", axisGap: 6, paneGap: 8 },
      warm: CHART2_SIGNAL_LIGHT,
      strategyRotation: CHART2_STRATEGY_ROTATION,
      heatRamp: CHART2_HEAT_RAMP,
      typography: CHART2_TYPOGRAPHY,
      radius: CHART2_RADIUS,
    },
  },
};
```

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/theme/themes.ts
git commit -m "feat(theme): Signal Dark + Signal Light theme definitions"
```

---

### Task A3: Rewrite `coerce*` / `resolveTheme` + migration in `themes.ts`

**Files:**
- Modify: `frontend/web/src/theme/themes.ts` (bottom helpers, current lines ~760–783; `THEME_DARK_KEY` ~249)

- [ ] **Step 1: Remove `THEME_DARK_KEY`, add a migration map, rewrite the helpers**

Delete the `THEME_DARK_KEY` export (line 249). Replace `coerceThemePreference`, `coerceDarkTheme`, `resolveTheme` with:

```ts
// One-time migration of pre-Signal stored preferences.
// "folio-dark" and "black" both collapse to the single "dark" theme.
export function coerceThemePreference(value: string | null): ThemePreference {
  switch (value) {
    case "auto":
    case "light":
    case "dark":
      return value;
    case "folio-dark":
    case "black":
      return "dark";
    default:
      return "dark";
  }
}

export function resolveTheme(
  preference: ThemePreference,
  systemTheme: SystemTheme,
): ResolvedTheme {
  if (preference === "auto") {
    return systemTheme === "light" ? "light" : "dark";
  }
  return preference;
}
```

(`coerceDarkTheme` is deleted — there is one dark theme now.)

- [ ] **Step 2: Run typecheck**

Run: `cd frontend/web && npm run typecheck`
Expected: remaining errors only in `useTheme.ts` (imports `coerceDarkTheme`/`THEME_DARK_KEY`) and tests — fixed in A4/A8.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/theme/themes.ts
git commit -m "feat(theme): collapse coerce/resolve + migrate folio-dark/black → dark"
```

---

### Task A4: Simplify `useTheme.ts` (drop dark-theme memory)

**Files:**
- Modify: `frontend/web/src/theme/useTheme.ts`

- [ ] **Step 1: Replace the file body**

```ts
import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { safeStorageGet, safeStorageSet } from "@/lib/storage";
import {
  coerceThemePreference,
  resolveTheme,
  themeDefinitions,
  THEME_PREFERENCE_KEY,
  type SystemTheme,
  type ThemePreference,
} from "./themes";

type Snapshot = {
  preference: ThemePreference;
  systemTheme: SystemTheme;
};

const listeners = new Set<() => void>();
let snapshot: Snapshot = readSnapshot();

function readSystemTheme(): SystemTheme {
  if (
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-color-scheme: dark)").matches
  ) {
    return "dark";
  }
  return "light";
}

function readSnapshot(): Snapshot {
  return {
    preference: coerceThemePreference(safeStorageGet(THEME_PREFERENCE_KEY)),
    systemTheme: readSystemTheme(),
  };
}

function sameSnapshot(a: Snapshot, b: Snapshot) {
  return a.preference === b.preference && a.systemTheme === b.systemTheme;
}

function refreshSnapshot() {
  const next = readSnapshot();
  if (sameSnapshot(snapshot, next)) return;
  snapshot = next;
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function getSnapshot() {
  refreshSnapshot();
  return snapshot;
}

function setThemePreference(preference: ThemePreference) {
  safeStorageSet(THEME_PREFERENCE_KEY, preference);
  refreshSnapshot();
}

export function useTheme() {
  const current = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const resolvedTheme = resolveTheme(current.preference, current.systemTheme);
  const definition = themeDefinitions[resolvedTheme];

  useEffect(() => {
    const query = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!query) return;
    const onChange = () => refreshSnapshot();
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const setPreference = useCallback((preference: ThemePreference) => {
    setThemePreference(preference);
  }, []);
  const setLightTheme = useCallback(() => setThemePreference("light"), []);
  const setDarkTheme = useCallback(() => setThemePreference("dark"), []);

  return useMemo(
    () => ({
      preference: current.preference,
      resolvedTheme,
      definition,
      setPreference,
      setLightTheme,
      setDarkTheme,
    }),
    [current.preference, definition, resolvedTheme, setDarkTheme, setLightTheme, setPreference],
  );
}
```

> Note: `setLightTheme`/`setDarkTheme` are kept because `Sidebar.tsx` (and possibly mobile) call them. If a consumer reads the removed `darkTheme` field, fix that consumer in B2/B5 (grep `\.darkTheme`).

- [ ] **Step 2: Typecheck**

Run: `cd frontend/web && npm run typecheck`
Expected: theme module clean; remaining errors only in tests (A8) and any `.darkTheme` consumer (resolve in B2/B5).

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/theme/useTheme.ts
git commit -m "refactor(theme): single dark theme, drop dark-theme memory"
```

---

### Task A5: Rewrite `tokens.css`

**Files:**
- Modify: `frontend/web/src/styles/tokens.css`

- [ ] **Step 1: Replace the entire file**

```css
/* xvision — Signal theme tokens.
 *
 * `:root` defaults to Signal Dark. ThemeProvider sets data-theme="dark|light".
 * Tailwind (tailwind.config.ts) maps utility classes onto these variables, so
 * components use either `bg-bg` (Tailwind) or `var(--bg)` (raw CSS).
 *
 * Token NAMES are unchanged from the prior theme — `--gold` is the brand accent
 * (now green) so existing `var(--gold)` / `text-gold` consumers work unchanged.
 */

:root {
  --bg: #000000;
  --surface-sidebar: #000000;
  --surface-card: #0a0a0a;
  --surface-elev: #0e0e0e;
  --surface-panel: #121212;
  --surface-hover: rgba(255, 255, 255, 0.04);

  --border: #1a1a1a;
  --border-strong: #2a2a2a;
  --border-soft: #141414;

  --text: #ffffff;
  --text-2: #9ca3af;
  --text-3: #5f6670;
  --text-4: #3a3f47;

  --gold: #00e676;
  --gold-soft: #00b85f;
  --gold-bg: rgba(0, 230, 118, 0.1);
  --gold-bg-strong: rgba(0, 230, 118, 0.18);
  --warn: #ffb020;
  --danger: #ff4d4d;
  --info: #5fa8ff;

  --radius-card: 6px;
  --radius-sm: 4px;
}

[data-theme="dark"] {
  --bg: #000000;
  --surface-sidebar: #000000;
  --surface-card: #0a0a0a;
  --surface-elev: #0e0e0e;
  --surface-panel: #121212;
  --surface-hover: rgba(255, 255, 255, 0.04);

  --border: #1a1a1a;
  --border-strong: #2a2a2a;
  --border-soft: #141414;

  --text: #ffffff;
  --text-2: #9ca3af;
  --text-3: #5f6670;
  --text-4: #3a3f47;

  --gold: #00e676;
  --gold-soft: #00b85f;
  --gold-bg: rgba(0, 230, 118, 0.1);
  --gold-bg-strong: rgba(0, 230, 118, 0.18);
  --warn: #ffb020;
  --danger: #ff4d4d;
  --info: #5fa8ff;
}

[data-theme="light"] {
  --bg: #f7f8fa;
  --surface-sidebar: #ffffff;
  --surface-card: #ffffff;
  --surface-elev: #f2f4f7;
  --surface-panel: #eaedf1;
  --surface-hover: rgba(0, 0, 0, 0.04);

  --border: #e3e6ea;
  --border-strong: #cbd1d8;
  --border-soft: #edeff2;

  --text: #0b0e11;
  --text-2: #4a5560;
  --text-3: #6b7682;
  --text-4: #9aa4ae;

  --gold: #00a15c;
  --gold-soft: #007a48;
  --gold-bg: rgba(0, 161, 92, 0.1);
  --gold-bg-strong: rgba(0, 161, 92, 0.16);
  --warn: #b45309;
  --danger: #d92d20;
  --info: #1d6fd6;
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/web/src/styles/tokens.css
git commit -m "feat(theme): Signal Dark/Light CSS variable blocks"
```

---

### Task A6: Fonts — `index.html`, `main.tsx`, `package.json`, `tailwind.config.ts`

**Files:**
- Modify: `frontend/web/index.html`
- Modify: `frontend/web/src/main.tsx`
- Modify: `frontend/web/package.json`
- Modify: `frontend/web/tailwind.config.ts`

- [ ] **Step 1: `index.html` — swap font link and theme-color**

Replace the comment + `<link rel="stylesheet" ...>` (lines 11–20) with:
```html
    <!-- Signal typography stack: Geist (display/sans/brand) + Geist Mono
         (numerals, IDs, timestamps, code). One <link> so globals.css and the
         theme tokens resolve the families. -->
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
    <link
      rel="stylesheet"
      href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700;800&family=Geist+Mono:wght@400;500;600;700&display=swap"
    />
```
Change line 9 `<meta name="theme-color" content="#0F0E0C" />` → `content="#000000"`.

- [ ] **Step 2: `package.json` — swap fontsource deps**

Run:
```bash
cd frontend/web
npm rm @fontsource/inter @fontsource/cormorant-garamond @fontsource/jetbrains-mono
npm i @fontsource/geist-sans @fontsource/geist-mono
```
Expected: `package.json` + `package-lock.json` updated; `node_modules/@fontsource/geist-sans` and `geist-mono` present.

- [ ] **Step 3: `main.tsx` — swap imports**

Replace lines 3–11 (the `@fontsource/*` imports) with:
```ts
import "@fontsource/geist-sans/400.css";
import "@fontsource/geist-sans/500.css";
import "@fontsource/geist-sans/600.css";
import "@fontsource/geist-sans/700.css";
import "@fontsource/geist-sans/800.css";
import "@fontsource/geist-mono/400.css";
import "@fontsource/geist-mono/500.css";
import "@fontsource/geist-mono/600.css";
import "@fontsource/geist-mono/700.css";
```

> If Vitest/Vite errors that a weight file path doesn't exist for these packages, fall back to the package's documented import paths (`@fontsource/geist-sans/index.css`). Verify resolvable paths with `ls node_modules/@fontsource/geist-sans/`.

- [ ] **Step 4: `tailwind.config.ts` — fonts**

Replace `fontFamily` (lines 33–37):
```ts
      fontFamily: {
        // `serif` retained as a Tailwind utility name for compatibility with
        // existing `font-serif` classes — Signal has no serif, it maps to Geist.
        serif: ["'Geist'", "ui-sans-serif", "system-ui", "sans-serif"],
        sans: ["'Geist'", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["'Geist Mono'", "ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
```
Update the file header comment line 3 from "Folio dark theme" to "Signal theme".

- [ ] **Step 5: Build to confirm fonts resolve and Tailwind compiles**

Run: `cd frontend/web && npm run build`
Expected: build succeeds; no unresolved `@fontsource` import errors.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/index.html frontend/web/src/main.tsx frontend/web/package.json frontend/web/package-lock.json frontend/web/tailwind.config.ts
git commit -m "feat(theme): load Geist + Geist Mono, map Tailwind font families"
```

---

### Task A7: `globals.css` — Geist, kill `.serif-i` italic

**Files:**
- Modify: `frontend/web/src/styles/globals.css`

- [ ] **Step 1: Body font-family → Geist**

Line 15: `font-family: "Inter", ui-sans-serif, system-ui, sans-serif;` → `font-family: "Geist", ui-sans-serif, system-ui, sans-serif;`

- [ ] **Step 2: `.caps` font-family → Geist**

Line 38: `font-family: "Inter", ui-sans-serif, system-ui, sans-serif;` → `font-family: "Geist", ui-sans-serif, system-ui, sans-serif;`

- [ ] **Step 3: `.serif-i` → Geist weight, no italic**

Replace lines 46–50:
```css
  .serif-i {
    font-family: "Geist", ui-sans-serif, system-ui, sans-serif;
    font-style: normal;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
```

- [ ] **Step 4: `.dec-pill` / `.dec-pos` / `.dec-filter__count` mono font → Geist Mono**

Replace every `font-family: "JetBrains Mono", ui-monospace, SFMono-Regular, monospace;` in this file (lines ~131, 143, 201) with `font-family: "Geist Mono", ui-monospace, SFMono-Regular, monospace;`.

- [ ] **Step 5: Build**

Run: `cd frontend/web && npm run build`
Expected: success.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/styles/globals.css
git commit -m "feat(theme): Geist in globals, remove serif-i italic"
```

---

### Task A8: Fix theme unit tests

**Files:**
- Find + Modify: theme tests. Run `cd frontend/web && grep -rl "folio-dark\|coerceDarkTheme\|THEME_DARK_KEY\|coerceThemePreference\|resolveTheme" src --include=*.test.ts --include=*.test.tsx`

- [ ] **Step 1: Locate failing theme tests**

Run: `cd frontend/web && npm test 2>&1 | tail -40`
Expected: failures in theme-preference / resolveTheme tests referencing removed `"folio-dark"`, `"black"`, `coerceDarkTheme`, or `THEME_DARK_KEY`.

- [ ] **Step 2: Update assertions to the new model**

For each failing test: replace `"folio-dark"`/`"black"` expectations with `"dark"`; assert `coerceThemePreference("folio-dark") === "dark"` and `coerceThemePreference("black") === "dark"` (migration); assert `resolveTheme("auto","dark") === "dark"` and `resolveTheme("auto","light") === "light"`; delete any `coerceDarkTheme`/`THEME_DARK_KEY` tests (the concept is gone). Add a migration test if none exists:
```ts
it("migrates retired theme ids to dark", () => {
  expect(coerceThemePreference("folio-dark")).toBe("dark");
  expect(coerceThemePreference("black")).toBe("dark");
  expect(coerceThemePreference("light")).toBe("light");
  expect(coerceThemePreference(null)).toBe("dark");
});
```

- [ ] **Step 3: Run the theme tests green**

Run: `cd frontend/web && npm test 2>&1 | tail -40`
Expected: theme tests PASS. (Other suites may still reference old hexes — those are addressed in their owning tracks; note any remaining failures for Phase C.)

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src
git commit -m "test(theme): update for 2-theme Signal model + migration"
```

---

### Task A9: Phase A gate — full typecheck + build

- [ ] **Step 1: Typecheck, build, test**

Run:
```bash
cd frontend/web && npm run typecheck && npm run build && npm test 2>&1 | tail -30
```
Expected: typecheck + build clean. Tests: theme suite green; record any unrelated red suites for the owning Phase-B track.

- [ ] **Step 2: Visual smoke (dark + light)**

Run `npm run dev`, open `http://localhost:5180`. Confirm: black surfaces, green accents (no gold), Geist everywhere, no Cormorant serif. In Settings → General, switch theme to Light: cool-white surfaces, green accent, legible text. Capture screenshots `docs/design/themes/evidence/A-dark.png` and `A-light.png`.

- [ ] **Step 3: Tag the foundation commit** (for clean Phase-B branching)

```bash
git log --oneline -1
```

---

## Phase B — Parallelizable tracks (after Phase A is committed)

### Task B1: Redesign the Settings theme picker

**Files:**
- Modify: `frontend/web/src/routes/settings/general.tsx`

**Spec:** The picker now offers Auto / Light / Dark (driven by `themePreferenceOptions`, already 3 entries after A2). Redesign per Signal: remove the `font-serif` (italic-prone) headings, use Geist weight; richer swatches that preview real surfaces.

- [ ] **Step 1: Fix `swatchFor` for the new IDs**

Replace (lines 17–21):
```tsx
function swatchFor(value: string) {
  const id: ResolvedTheme = value === "auto" ? "dark" : (value as ResolvedTheme);
  return themeDefinitions[id].swatch;
}
```

- [ ] **Step 2: Replace the three `font-serif font-medium` headings**

For each of the 3 `<h3 className="m-0 font-serif font-medium text-[20px] tracking-tight">` (lines 72, 125, 139): change to `className="m-0 font-sans font-semibold text-[18px] tracking-tight"`.

- [ ] **Step 3: Upgrade the swatch preview to a 3-cell strip (bg / surface / accent) with a bordered text dot**

Replace the swatch `<span aria-hidden ...>` block (lines 105–114) with:
```tsx
                <span
                  aria-hidden
                  className="flex h-7 w-10 overflow-hidden rounded-sm border border-border"
                  style={{ background: swatch.bg }}
                >
                  <span className="flex-1" style={{ background: swatch.surface }} />
                  <span className="flex-1" style={{ background: swatch.accent }} />
                  <span
                    className="flex-1 grid place-items-center"
                    style={{ background: swatch.bg }}
                  >
                    <span
                      className="h-2 w-2 rounded-full"
                      style={{ background: swatch.text }}
                    />
                  </span>
                </span>
```

- [ ] **Step 4: Update the Appearance helper copy**

Line 76 `Choose the dashboard color theme. This changes colors only.` → `Signal theme. Auto follows your system; Light is the cool-white variant; Dark is pure-black Signal.`

- [ ] **Step 5: Typecheck + visual**

Run: `cd frontend/web && npm run typecheck`. Then `npm run dev`, open Settings → General, toggle Auto/Light/Dark, confirm swatches + live theme switch, no serif/italic. Screenshot `docs/design/themes/evidence/B1-settings.png`.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/routes/settings/general.tsx
git commit -m "feat(settings): Signal theme picker (Auto/Light/Dark)"
```

---

### Task B2: Italic + serif sweep (shared/chrome components)

**Files:**
- Modify: all `*.tsx` containing `font-serif`, ` italic`, or `serif-i` **except** files owned by B1 (`settings/general.tsx`) and B4 (`eval-runs-detail*.tsx` + `components/eval-detail/**`). Known: `src/components/shell/Sidebar.tsx`, `src/components/mobile/MobileDrawer.tsx`, review components (`ReviewContent.tsx`, `ReviewPanel.tsx`), `FilterSummaryPanel.tsx`, `DocsMarkdown.tsx`, `PullQuote.tsx`.

- [ ] **Step 1: Enumerate the targets**

Run:
```bash
cd frontend/web && grep -rln "font-serif\|\bitalic\b\|serif-i" src --include=*.tsx \
  | grep -vE "settings/general\.tsx|eval-runs-detail|components/eval-detail/"
```
Record the list.

- [ ] **Step 2: For each file, remove italic + retone serif**

Rules (apply per occurrence):
- `font-serif italic` → `font-sans font-semibold` (Geist semibold replaces serif-italic emphasis).
- standalone `font-serif` → `font-sans` (keep any existing weight; if none, add `font-medium`).
- standalone Tailwind ` italic` (not on serif) → remove the class; if it was the only emphasis, add `font-medium`.
- `serif-i` className → `font-sans font-semibold` (the `.serif-i` CSS is already de-italicised in A7, but prefer explicit classes in TSX).
- `PullQuote.tsx`: the conditional `font-serif italic` branch → `font-sans font-semibold`; keep the `font-mono` branch.

- [ ] **Step 3: Fix any `.darkTheme` consumer**

Run: `cd frontend/web && grep -rn "\.darkTheme" src`. For each (e.g. a Sidebar that read `darkTheme`), replace with `setDarkTheme()` (sets `"dark"`).

- [ ] **Step 4: Typecheck + build + grep clean**

Run:
```bash
cd frontend/web && npm run typecheck && \
  grep -rn "font-serif\|\bitalic\b" src --include=*.tsx | grep -vE "settings/general\.tsx|eval-runs-detail|components/eval-detail/" || echo "CLEAN"
```
Expected: typecheck clean; grep prints `CLEAN`.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src
git commit -m "refactor(ui): remove italic, serif→Geist across chrome components"
```

---

### Task B3: Chart hardcoded-hex + axis-font sweep

**Files:**
- Modify: chart components/adapters under `frontend/web/src/components/chart/**` that embed literal warm hexes or the old axis font. Find them in Step 1.

- [ ] **Step 1: Find literal warm hexes + Inter/JetBrains axis fonts in chart code**

Run:
```bash
cd frontend/web && grep -rniE "#2a2618|#3a3322|#d4a547|#b8862e|#f0c75e|#8a5f16|#221f15|#17150f|#0f0e0c" src/components/chart src/theme --include=*.ts --include=*.tsx
grep -rn "'Inter'\|\"Inter\"\|JetBrains Mono\|Cormorant" src/components/chart --include=*.ts --include=*.tsx
```
Record results. (Most chart color comes from `theme.chart2` already rebranded in A2; this catches stragglers baked into adapters/option builders.)

- [ ] **Step 2: Replace each literal**

For each hit: route through the theme instead of a literal where a theme token exists (`theme.chart2.surface.gridStrong` etc.). If a literal must stay (e.g. a fixed neutral), map warm→Signal: `#2a2618`/`#3a3322` → `var(--border-strong)`/theme grid; `#d4a547`/`#b8862e`/`#f0c75e`/`#8a5f16` → theme accent. Replace axis-font literals `'Inter'`/`'JetBrains Mono'` with `'Geist Mono'` (use `theme.chart2.density.axisFont` where the adapter reads the theme).

- [ ] **Step 3: Monthly-returns heatmap green/red check**

Run: `cd frontend/web && grep -rln "monthly\|heatmap\|MonthlyReturn" src/components --include=*.tsx`. Open the component; confirm positive cells use `rgba(0,230,118,α)` (via `theme.chart2.warm.gold` / positive token) and negative use `rgba(255,77,77,α)` — not gold. Fix if it still keys off the old gold.

- [ ] **Step 4: Typecheck + build + chart fixtures (if present)**

Run: `cd frontend/web && npm run typecheck && npm run build`. If chart tests exist: `npm test -- chart 2>&1 | tail -20`.

- [ ] **Step 5: Visual — open a chart-heavy screen**

`npm run dev` → open an eval run with a chart + a compare view. Confirm grid lines are cool gray (not warm), series rotation is the Signal palette, candles green/red, no gold lines. Screenshot `docs/design/themes/evidence/B3-charts.png`.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src
git commit -m "refactor(charts): route warm literals through Signal theme; Geist axes"
```

---

### Task B4: Eval Run Detail redesign

**Files:**
- Create: `frontend/web/src/components/eval-detail/BrandMark.tsx`, `EvalTopBar.tsx`, `MetaChip.tsx`, `ActionPill.tsx`, `PhaseChip.tsx`, `DecisionTimeline.tsx`
- Modify: `frontend/web/src/routes/eval-runs-detail.tsx` (compose the new chrome), and its mobile sibling `eval-runs-detail-mobile.tsx` for parity where applicable.
- Reference (read, do not import): `docs/design/themes/signaltheme_rebrand_XVN_extracted/design_handoff_signal/eval-run-detail/app.jsx` (full composition — `App`, `TopBar`, `BrandMark`, `MetaChip`, `SummaryCard`, `Stat`, `EquityCurve`, `DecisionsTable`, `DecisionTimeline`, `ActionPill`, `PhaseChip`, `Meta`, `ReviewPanel`). The README §1–§13 is the written spec; `app.jsx` is the working code to translate into idiomatic codebase components (Tailwind utilities + theme tokens, no inline-Babel).

> **Translation rule:** the design `app.jsx` uses inline styles + `var(--token)`. Port to the codebase's conventions — Tailwind utilities (`bg-surface-elev`, `text-text-2`, `font-mono`, `border-border`) for static styling, inline `style` only for computed values (density-strip pixel math, progress-bar widths, dynamic colors). Reuse existing primitives (`Card`, `Pill`) where they already match. **No popups/overlays** — tooltips on the density strip are the existing toast/inline-positioned pattern, not a focus-stealing layer (an absolutely-positioned non-interactive hover label is fine; it does not steal focus).

- [ ] **Step 1: Read the reference + current route**

Read `design_handoff_signal/eval-run-detail/app.jsx` end-to-end and README §1–§13. Read `frontend/web/src/routes/eval-runs-detail.tsx` and note the existing pieces (`Topbar`, `SummaryCard`, `InspectorContextStrip`, `FilterSummaryPanel`, `FilterEventTimeline`, `DecisionsPanel`, `RunChart`) so the redesign maps onto real data, not mock data.

- [ ] **Step 2: `BrandMark.tsx`** — 14×14 `--gold` square (`borderRadius:2`) + "XVN" in `font-mono` 14px/700, `letterSpacing:0.18em`. (README §Brand mark.)

- [ ] **Step 3: `ActionPill.tsx`** — props `{ action: "BUY"|"SELL"|"HOLD"|"CLOSE" }`. Inline-flex, gap 1.5, padding `3px 7px 3px 6px`, `rounded-[3px]`, `min-w-[50px]`, centered, `font-mono` 10px/600, `tracking-[0.1em]`, line-height 1; 9×9 leading glyph SVG. Variants (README §5 table): BUY ↑ `#001A0A` on `--gold` solid; SELL ↓ `#1A0000` on `--danger` solid; HOLD — `--text-2` transparent, border `--border-strong`; CLOSE × `--warn` on `rgba(255,176,32,0.10)`, border `rgba(255,176,32,0.45)`.

- [ ] **Step 4: `PhaseChip.tsx`** — props `{ phase: "engaged"|"filtered" }`. ENGAGED: `bg-surface-elev`, border `--border-strong`, text `--text`, 5×5 solid `--gold` dot, weight 600. FILTERED: transparent bg, border `--border-strong`, text `--text-3`, 5×5 hollow ring (1px `--text-3`), weight 500. (README §6 — must read quieter than engaged but NOT as error: no red/amber.)

- [ ] **Step 5: `MetaChip.tsx`** — props `{ label, value, tone: "neutral"|"gold"|"info", onClick? }`. Height 28px, padding `0 10px`, `rounded-[4px]`, 1px border, inline-flex gap 8px; UPPERCASE label `font-mono` 10px/600 `tracking-[0.16em]` + value `font-mono` 12px/500 + 9×9 chevron-right SVG at 50% opacity. Tones (README §3 table): neutral (label `--text-3`/value `--text`/border `--border`/bg `--surface-elev`); gold (label `--gold-soft`/value `--gold`/border `--gold-soft`/bg `--gold-bg`); info (label+value `--info`/border `rgba(95,168,255,0.40)`/bg `rgba(95,168,255,0.10)`).

- [ ] **Step 6: `DecisionTimeline.tsx`** (the new density strip — README §8 + Implementation notes)

Behavior: absolute-positioned ticks in a relative 36px-tall container (`bg-surface-elev`, 1px `--border-soft`, `rounded-[3px]`), `ResizeObserver` measures width; `tickW = min(6, max(1, floor(containerWidth / n)))`, `gap = tickW >= 4 ? 1 : 0`, `slot = tickW + gap`. Per decision: full 36px transparent hit-area (click → `onJump(i)`); engaged → 32px filled column anchored `top:2px` colored by action (`--gold`/`--danger`/`--text-2`); filtered → 10px stub anchored `bottom:1px` color `--text-3`. Header (`mb-2.5`): left `DENSITY` (`font-mono` 10/500 `tracking-[0.18em]` `--text-3`) + `{n} steps · {window}`; right 4-swatch legend (buy/sell/hold/filtered; filtered swatch = 9×4 stub). Focused marker: 5px down-triangle `--gold` at `top:-6px` over the focused tick. Active-filter dimming: non-matching ticks → opacity 0.45 + color `--border-strong` (do NOT remove them). Hover tooltip: absolutely-positioned label `transform: translate(-50%, calc(-100% - 10px))`, clamped 80px from each container edge, `bg-surface-card`, 1px `--border-strong`, shadow `0 8px 20px rgba(0,0,0,0.5)`; content `# {i} · {timestamp} · {ACTION} · {conv%}` + justification line (≤280px) when engaged. (Tooltip is a non-interactive hover label — not a popup.)

- [ ] **Step 7: `EvalTopBar.tsx`** (README §1) — 48px tall, full-width, `bg-surface-sidebar`, 1px `--border` bottom, single flex row `px-4 gap-3`: `BrandMark` · `/` (`--text-4`) · `EVAL RUNS` (`font-mono` 11px uppercase `tracking-[0.18em]` `--text-3`) · `/` · full Run ID (`font-mono` 12px `--text-2`, no truncation) · spacer · status pill right (COMPLETED: `--gold-bg`/border `--gold-soft`/text `--gold`/solid green 6px dot; RUNNING: `rgba(95,168,255,0.12)`/border `rgba(95,168,255,0.40)`/text `--info`/blue dot `animate-pulse`; label `font-mono` 10px `tracking-[0.16em]` uppercase; pill `px-2.5 py-1 rounded-sm`). **Removed (do not add): POST-HOC⇄EVAL toggle, ⌘K, the duplicate "Run abc… · scenario …" middle section.**

- [ ] **Step 8: Recompose `eval-runs-detail.tsx`**

Replace the current generic `Topbar` with `EvalTopBar` (breadcrumb + status). Body header: H1 = run ID only in `font-mono` 28px/500 `tracking-[-0.03em] tabular-nums` (no "Run" prefix); baseline-right meta `font-mono` 12px `--text-3` (`started … · budget … · commit …`); `mt-4` MetaChip row → Strategy (gold) / Scenario (neutral) / Agent (info), wired to the existing route jumps that `InspectorContextStrip` used. Grid `grid-cols-12 gap-5`: left `col-span-8` SummaryCard + DecisionsTable; right `col-span-4` Meta card + ReviewPanel. DecisionsTable gains: PHASE column (`PhaseChip`), search input (focus border `--gold-soft`), SORT `<select>`, mutually-exclusive action filter-pill row (All/Buy/Sell/Hold/Filtered), and the `DecisionTimeline` density strip above the table. Filtered rows: opacity 0.78, engaged-only cells show `—` in `--text-4`. Clicking a row or a density tick calls the existing decision-jump/filter handler. Meta card shows only `seed, mode, region, budget, commit, started, duration` — NOT id/strategy/scenario/agent (those live in H1 + MetaChips). Run ID appears in exactly two places (TopBar breadcrumb + H1). ReviewPanel lead paragraph is plain Geist 15px (no serif/italic).

> Map design fields to real data: the route already loads decisions/positions via `DecisionsPanel`. `phase` = engaged vs filtered should derive from existing filter-event/decision data (a decision intercepted by a risk/freshness/regime filter = `filtered`; otherwise `engaged`). If the backing data has no explicit phase, derive it from whether an engaged action exists for the step (a filtered step has no action/conviction/justification — mirror the `Decision` shape in README §"Data shape"). Do not invent backend changes; compute phase in the adapter from fields already present.

- [ ] **Step 9: Typecheck + build**

Run: `cd frontend/web && npm run typecheck && npm run build`. Expected: clean.

- [ ] **Step 10: Visual verification against the reference**

`npm run dev` → open a completed eval run (`/eval/runs/:id`) and a running one. Verify against README §"Acceptance checklist": TopBar has no POST-HOC/⌘K/duplicate; H1 = id only; PHASE column with ENGAGED(filled)/FILTERED(outlined); filtered rows dimmed with `—`; search+sort+pill row present; density strip renders, scales, click+hover work, filtered ticks hit-targetable across full height; no persistent labels under the strip. Compare side-by-side with `references/` HTML. Screenshots `docs/design/themes/evidence/B4-eval-completed.png` and `B4-eval-running.png`.

- [ ] **Step 11: Commit**

```bash
git add frontend/web/src/components/eval-detail frontend/web/src/routes/eval-runs-detail.tsx frontend/web/src/routes/eval-runs-detail-mobile.tsx
git commit -m "feat(eval-detail): Signal TopBar, MetaChip, PhaseChip, ActionPill, DecisionTimeline"
```

---

### Task B5: Mobile + scrollbar polish

**Files:**
- Modify: mobile components found in Step 1; verify `globals.css` scrollbars (already token-driven via `--gold` color-mix — should auto-rebrand green).

- [ ] **Step 1: Confirm themed scrollbars resolve to green**

The `.xvn-scroll` / `.scrollbar-stable` thumbs use `color-mix(... var(--gold) ...)` (globals.css) — now green automatically. Verify visually in dark + light; no code change unless a mobile component hardcodes the old gold rgba. Run: `cd frontend/web && grep -rniE "212,\s*165,\s*71|240,\s*199,\s*94|rgba\(212|rgba\(240" src/components/mobile src/styles` and replace any hit with `rgba(0,230,118,…)` (dark) / token.

- [ ] **Step 2: Mobile brand square + accents**

Run: `cd frontend/web && grep -rln "font-serif\|\bitalic\b" src/components/mobile`. Apply the B2 rules (serif→Geist, drop italic) to any mobile chrome (e.g. `MobileDrawer` logo). Confirm the mobile brand dot uses `--gold`.

- [ ] **Step 3: Typecheck + mobile viewport visual**

Run: `cd frontend/web && npm run typecheck`. `npm run dev`, open with a phone viewport (dev tools), confirm green scrollbars + accents, Geist, no gold/serif. Screenshot `docs/design/themes/evidence/B5-mobile.png`.

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src
git commit -m "feat(mobile): Signal accents + green scrollbars"
```

---

## Phase C — Verification & evidence

### Task C1: Repo-wide acceptance sweep + evidence bundle

- [ ] **Step 1: Grep for any surviving warm/gold literals + italic + old fonts**

Run:
```bash
cd frontend/web && echo "== warm hexes ==" && \
  grep -rniE "#d4a547|#b8862e|#f0c75e|#8a5f16|#0f0e0c|#17150f|#14120e|#2a2618|#3a3322" src --include=*.ts --include=*.tsx --include=*.css ; \
  echo "== italic ==" && grep -rniE "font-style:\s*italic|fontStyle:\s*[\"']italic|\bitalic\b" src --include=*.ts --include=*.tsx --include=*.css ; \
  echo "== retired fonts ==" && grep -rniE "Cormorant|'Inter'|\"Inter\"|JetBrains Mono" src --include=*.ts --include=*.tsx --include=*.css
```
Expected: no remaining brand-gold literals; no italic; no Cormorant/Inter/JetBrains (mono fallback strings inside a Geist Mono stack are acceptable only if explicitly a fallback — confirm each). Fix stragglers in the owning file and recommit.

- [ ] **Step 2: Full gate**

Run: `cd frontend/web && npm run typecheck && npm run build && npm test 2>&1 | tail -40`
Expected: typecheck + build clean; tests green (or pre-existing-unrelated failures explicitly noted with justification).

- [ ] **Step 3: Walk the README acceptance checklist (README §"Acceptance checklist")**

Tick each of the 18 items against the running app (dark + light). Record pass/fail per item in `docs/design/themes/evidence/acceptance.md` with the screenshot filenames as evidence.

- [ ] **Step 4: Final evidence summary**

Confirm `docs/design/themes/evidence/` contains: `A-dark.png`, `A-light.png`, `B1-settings.png`, `B3-charts.png`, `B4-eval-completed.png`, `B4-eval-running.png`, `B5-mobile.png`, `acceptance.md`. Write a one-paragraph completion note at the top of `acceptance.md` (what changed, what's verified, any deferred item).

- [ ] **Step 5: Commit**

```bash
git add docs/design/themes/evidence frontend/web/src
git commit -m "chore(theme): Signal rebrand acceptance sweep + evidence"
```

---

## Self-review (spec coverage)

| README / request requirement | Task |
|---|---|
| Remove warm gold/serif palette → Signal black+green | A1, A2, A5 |
| `--gold` token kept, value green | A2, A5 (header decision 3) |
| Geist + Geist Mono; load from Google Fonts; Fontsource fallback | A6 |
| No italic anywhere; `.serif-i` de-italicised | A7, B2, B4, B5, C1 |
| Chart palette = Signal multi-hue rotation | A1, A2 |
| Monthly returns heatmap green+/red− | B3 §3 |
| TopBar: no POST-HOC / ⌘K / duplicate run middle | B4 §7 |
| Body H1 = run ID only | B4 §8 |
| Run ID in exactly 2 places | B4 §8 |
| MetaChip row Strategy/Scenario/Agent tones | B4 §5,§8 |
| Decisions PHASE column (ENGAGED filled / FILTERED outlined) | B4 §4,§8 |
| Filtered rows `—` + opacity 0.78 | B4 §8 |
| Decisions search + sort + action-pill filter row | B4 §8 |
| DecisionTimeline density strip (scales, click+hover, full-height hit) | B4 §6 |
| No persistent labels under the strip | B4 §6 |
| Settings "themes" redesigned | B1 |
| Light mode included (designed) | A2, A5 (decisions 1–2) |
| Theme set collapsed + migration | A1, A3, A4, A8 |
| Mobile green scrollbars (not system) | A7 (token), B5 |
| No popups (CLAUDE.md) | B4 translation rule |
| Evidence of completion | A9, B*, C1 |

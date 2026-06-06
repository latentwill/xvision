# XVN · Signal theme rebrand + Eval Run Detail redesign

## Overview

This handoff package is the **Signal theme** rebrand of the XVN trading platform. The product is a Rust binary plus embedded React/Vite dashboard for running LLM-driven trading agents against a broker, with backtesting, paper trading, and live execution modes.

The rebrand:

1. **Removes the warm "folio-dark" gold/amber/cream palette** (which read too close to Binance and clashed with standard red/green chart semantics).
2. **Replaces it with Signal** — pure black surfaces, Signal-green `#00E676` as the brand/positive accent, standard `#FF4D4D` red for negative/danger, cool gray neutrals.
3. **Swaps the typography** from Cormorant Garamond (display serif) + Inter (sans) + JetBrains Mono → **Geist** (display + sans) + **Geist Mono** (everything monospace).
4. **Redesigns the Eval Run Detail page** with new chrome: cleaner topbar, action chips, a Phase column (Engaged / Filtered), a search-filter bar, and a per-decision density-strip timeline.

---

## About the design files

The files in this bundle are **design references built in HTML/CSS/JSX (React via in-browser Babel)** — they are prototypes showing intended look, behavior, and exact token values. They are **not production code to ship directly**.

Your task is to **recreate these designs in the target codebase** (`xvision/frontend/web/`, which already uses React + Vite + a `theme/themes.ts` token file) using its established patterns and libraries. The CSS files here are an executable spec for the token values, and the JSX files show the component compositions that need to be translated into the codebase's component library.

If a particular pattern doesn't have a clear home in the existing codebase (e.g. the `DecisionTimeline` density strip is new), implement it as a new component in the appropriate location and reuse the same token names.

## Fidelity

**High-fidelity (hifi)** — colors, typography, spacing, sizing, and interactions are all final. Match them pixel-perfectly. The token values in `css/signal.css` are the source of truth for colors and sizes.

---

## Design tokens (drop-in replacement for `theme/themes.ts`)

These are the new Signal token values. Replace the existing folio-dark palette with these. Token **names** are kept identical to the previous theme so existing consumers (CSS variables, theme references in JSX) work without code changes — only values change.

```ts
// Surfaces — pure black, hairline borders
'--bg':                '#000000',
'--surface-sidebar':   '#000000',
'--surface-card':      '#0A0A0A',
'--surface-elev':      '#0E0E0E',
'--surface-panel':     '#121212',
'--surface-hover':     'rgba(255,255,255,0.04)',

// Borders — cool dark gray
'--border':            '#1A1A1A',
'--border-strong':     '#2A2A2A',
'--border-soft':       '#141414',

// Text — cool gray ramp
'--text':              '#FFFFFF',
'--text-2':            '#9CA3AF',
'--text-3':            '#5F6670',
'--text-4':            '#3A3F47',

// Accents
'--gold':              '#00E676',    // Signal green — the brand accent (token kept named "gold" for compat)
'--gold-soft':         '#00B85F',
'--gold-bg':           'rgba(0, 230, 118, 0.10)',
'--gold-bg-strong':    'rgba(0, 230, 118, 0.18)',

// Semantic
'--warn':              '#FFB020',
'--danger':            '#FF4D4D',
'--info':              '#5FA8FF',

// Radii
'--radius-card':       '6px',
'--radius-sm':         '4px',
```

### Chart palette (multi-line strategy comparison)

For charts with multiple series (line comparisons, strategy A/B, etc.), use this rotation. The lead is Signal green; the rest are distinguishable cool/warm hues that don't conflict with the up/down semantic colors.

```ts
chart: {
  primary: '#00E676',   // green — lead, also matches "up"
  sky:     '#38BDF8',
  mint:    '#5EEAD4',
  yellow:  '#FBBF24',
  orange:  '#FB923C',
  pink:    '#F472B6',
  violet:  '#A78BFA',
  cyan:    '#22D3EE',
  up:      '#00E676',
  down:    '#FF4D4D',
}
```

For positive/negative gradients (heatmaps, monthly returns, drawdown fills):
- Positive: `rgba(0, 230, 118, alpha)`
- Negative: `rgba(255, 77, 77, alpha)`

---

## Typography

```ts
// Display / brand / headlines / body
'--font-display':  "'Geist', sans-serif",
'--font-sans':     "'Geist', sans-serif",
'--font-brand':    "'Geist', sans-serif",

// All monospace usage (numerals, code, IDs, timestamps, KPIs)
'--font-mono':     "'Geist Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
```

Load from Google Fonts:
```html
<link href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700;800&family=Geist+Mono:wght@400;500;600;700&display=swap" rel="stylesheet"/>
```

### Type scale (used throughout)

| Role | Family | Size | Weight | Letter-spacing |
|---|---|---|---|---|
| Display H1 | Geist | 28–34px | 600 | -0.03em |
| Card title (h2) | Geist | 15–22px | 600 | -0.01em |
| Section label (UPPERCASE) | Geist Mono | 10–11px | 500 | 0.16–0.18em |
| Body | Geist | 13–14px | 400 | normal |
| Body small | Geist | 11–12px | 400 | normal |
| Numerals (KPI hero) | Geist | 28–56px | 600 | -0.025 to -0.04em, tabular-nums |
| Numerals (table) | Geist Mono | 11–13px | 400/500 | tabular-nums |
| Brand mark wordmark | Geist Mono | 14px | 700 | 0.18em |

**No italic.** The previous theme used Cormorant italic for emphasis and brand. Signal has no italic anywhere — emphasis is conveyed through weight, size, and color.

### Brand mark

The wordmark "XVN" is rendered in **Geist Mono 700 with 0.18em letter-spacing**, preceded by a 14×14px solid square in `--gold`. See `BrandMark` in `eval-run-detail/app.jsx`.

```jsx
<div className="flex items-center gap-2">
  <span style={{ width: 14, height: 14, background: 'var(--gold)', borderRadius: 2 }}/>
  <span className="font-mono" style={{ fontSize: 14, fontWeight: 700, letterSpacing: '0.18em' }}>XVN</span>
</div>
```

---

## Files in this bundle

```
design_handoff_signal/
├── README.md                                   ← this file
├── css/
│   ├── signal.css                              ← all desktop theme tokens + base components
│   └── signal-mobile.css                       ← mobile-specific layer
├── components/
│   ├── shared.jsx                              ← Icon, Sidebar, Topbar, Sparkline primitives
│   └── list-toolbar.jsx                        ← ListSearch, ListSelect, ListActiveChips, ListToolbar, useListState, ListCard
├── eval-run-detail/
│   ├── Eval Run Detail.html                    ← entrypoint
│   ├── app.jsx                                 ← page composition (TopBar, MetaChip, SummaryCard, DecisionsTable, DecisionTimeline, ActionPill, PhaseChip)
│   ├── data.jsx                                ← mock data shape
│   ├── strip.jsx                               ← floating evals capsule (bottom of screen)
│   ├── flame.jsx                               ← flame graph + Inspector panel
│   ├── dock.jsx                                ← bottom trace dock (FilterBar, DecisionJump, FlameGraph host)
│   └── tweaks-panel.jsx                        ← edit-mode tweaks (dev-only chrome)
├── screens/
│   ├── screen-home.jsx                         ← Control Tower (home dashboard)
│   ├── screen-eval-runs.jsx                    ← /eval/runs list
│   └── screen-strategies.jsx                   ← /strategies list
├── charts/
│   ├── chart-theme.css                         ← uPlot / KlineCharts overrides
│   └── chart-data.js                           ← XVN_PALETTE export, mock generators
└── references/
    ├── xvn v1 design.html                      ← Control Tower + 8 sub-screens (canvas)
    ├── xvn mobile design.html                  ← Mobile/tablet/desktop responsive frames
    ├── XVN Chart Designs.html                  ← 6 chart variants
    ├── Lists.html                              ← List component family
    ├── Calendar Picker.html                    ← Date range picker
    └── Capsule · Multi-Eval.html               ← Floating multi-eval capsule
```

---

## Pages / Views

### 1. Topbar (global chrome) — `app.jsx :: TopBar`, `BrandMark`

**Layout**: 48px tall, full-width, `--surface-sidebar` background, 1px `--border` bottom rule. Single row, flex, `px-4 gap-3`.

**Contents (left → right):**
- `BrandMark` — 14×14px solid green square + "XVN" wordmark in Geist Mono 14/700, letter-spacing 0.18em
- `/` divider — `text-text-4`
- `EVAL RUNS` — Geist Mono 11px uppercase, letter-spacing 0.18em, `--text-3`
- `/` divider
- Run ID — Geist Mono 12px, `--text-2`, no truncation (`abc1234de7f6` full)
- Spacer → push to right
- Status pill (right side): `EVAL COMPLETED` or `EVAL RUNNING`
  - Completed: `--gold-bg` background, `--gold-soft` border, `--gold` text, solid green dot
  - Running: `rgba(95,168,255,0.12)` background, `rgba(95,168,255,0.40)` border, `--info` text, blue dot with `animate-pulse`
  - Dot: 6×6px circle. Label: Geist Mono 10px, letter-spacing 0.16em, uppercase
  - Pill padding: `px-2.5 py-1`, `rounded-sm2` (4px)

**Removed from previous version** (do NOT include these — they were redundant or unhelpful):
- "POST-HOC ⇄ EVAL" toggle
- `⌘K` command-K button
- Duplicate "Run abc1234… · scenario flash-crash-2024-08" middle section

### 2. Eval Run Detail body — `app.jsx :: App`

**Layout**: full-height column under TopBar. `flex-1 overflow-auto px-6 py-6`, content centered in `max-w-[1400px]`.

**Body header (above the grid):**
- **H1**: `abc1234de7f6` — Geist Mono 28px / 500, letter-spacing -0.03em, tabular-nums. *Just the ID*, no "Run" prefix (the breadcrumb already says "EVAL RUNS").
- Right of H1, baseline-aligned: meta string in Geist Mono 12px `--text-3` → `started 2026-05-17 10:13:31Z · budget $0.18 / $1.00 · commit 7f2b1ad`
- Below (mt-4): **MetaChip row** — 3 chips for Strategy / Scenario / Agent. See § MetaChip.

**Grid**: `grid-cols-12 gap-5`
- Left column (`col-span-8`): SummaryCard, DecisionsTable
- Right column (`col-span-4`): Meta card, ReviewPanel

### 3. `MetaChip` — `app.jsx :: MetaChip`

Pill button used under the H1 for the run's contextual metadata. Three tones: `neutral` (Scenario), `gold` (Strategy, the brand-coded element), `info` (Agent).

**Spec:**
- Height 28px, padding `0 10px`, `border-radius: 4px`, 1px border
- Inline-flex, gap 8px
- Inner structure: UPPERCASE label (Geist Mono 10px / 600, letter-spacing 0.16em, label-tone color) + value (Geist Mono 12px / 500, value-tone color) + chevron right (9×9 SVG, 50% opacity)
- Clickable (it's the affordance for a switch/picker, even if not wired to one in this prototype)

**Tones:**

| Tone | Label color | Value color | Border | Background |
|---|---|---|---|---|
| `neutral` | `--text-3` | `--text` | `--border` | `--surface-elev` |
| `gold` | `--gold-soft` | `--gold` | `--gold-soft` | `--gold-bg` |
| `info` | `--info` | `--info` | `rgba(95,168,255,0.40)` | `rgba(95,168,255,0.10)` |

### 4. `SummaryCard` — `app.jsx :: SummaryCard`

Card with title "Summary" and right-aligned `PASS` chip. Body:
- `EquityCurve` — 140px tall sparkline of the run's PnL%, full-width, with:
  - Equity line: `--gold` 0.9px stroke
  - Filled area: vertical gradient `--gold` (0.5 → 0) at 30% opacity
  - Drawdown shock: highlighted rectangle in `rgba(255,77,77,0.10)` over the shock window
  - 4 horizontal grid lines: `#2a2618` 0.3px (this was the gold-tinted dark in the old theme — **replace with `--border-soft`** when porting)
  - Labels: top-left "EQUITY · pnl%" (Geist Mono 10px, 0.18em tracked, `--text-3`); top-right run-event annotation; bottom-left "−0.0%"; bottom-right "+6.42%" in `--gold`
- 4-column `Stat` row (PNL, MAX DRAWDOWN, SHARPE, WIN RATE). Each Stat has a UPPERCASE label, large mono value (24px / 500, tabular-nums), tiny sub-label.

### 5. `ActionPill` — `app.jsx :: ActionPill`

The Buy/Sell/Hold chip used in the Decisions table. Filled or outlined chip with a leading direction glyph and the action label.

**Spec:**
- Inline-flex, gap 1.5, padding `3px 7px 3px 6px`, `border-radius: 3px`, `min-width: 50px`, centered
- Geist Mono 10px / 600, letter-spacing 0.1em, line-height 1
- Glyph: 9×9px SVG positioned before the label

**Variants:**

| Action | Glyph | Foreground | Background | Border |
|---|---|---|---|---|
| `BUY` | up-arrow ↑ | `#001A0A` (dark on green) | `--gold` solid | `--gold` |
| `SELL` | down-arrow ↓ | `#1A0000` (dark on red) | `--danger` solid | `--danger` |
| `HOLD` | horizontal bar — | `--text-2` | transparent | `--border-strong` |
| `CLOSE` | × glyph | `--warn` | `rgba(255,176,32,0.10)` | `rgba(255,176,32,0.45)` |

Solid fills for action; outlined-only for inaction (HOLD). Critical: `BUY` and `SELL` are the "loud" states; `HOLD` is intentionally quiet.

### 6. `PhaseChip` — `app.jsx :: PhaseChip`

New chip introduced in this redesign. Sits in a new `PHASE` column in the Decisions table. Represents whether a step engaged a decision or was filtered out by a risk/freshness/regime filter.

**ENGAGED** (engaged decision was made):
- Background: `--surface-elev`
- Border: 1px `--border-strong`
- Foreground: `--text`
- Dot: 5×5px solid `--gold` circle
- Weight: 600

**FILTERED** (filter intercepted, no action taken):
- Background: transparent
- Border: 1px `--border-strong`
- Foreground: `--text-3`
- Dot: 5×5px **hollow ring** (1px `--text-3` border, transparent fill)
- Weight: 500

Critical design intent: **Filtered must read quieter than Engaged but NOT as an error or warning.** No red, no amber. The filter doing its job is a normal outcome, not a failure. The hollow ring + lighter text + transparent background achieves this without ambiguity.

### 7. `DecisionsTable` — `app.jsx :: DecisionsTable`

The main decisions list. Card with toolbar + filter pill row + density timeline + table.

**Card title**: `Decisions`. Subtitle: `{filteredCount} of {totalCount} steps · {engagedCount} engaged` (live counts).

**Toolbar row** (`px-5 pt-4 pb-3`, border-bottom):
- Search input — 32px tall, `--surface-elev` background, `--border` border, `border-radius: 4px`, max-width 320px
  - Search icon (13px) + input + clear "×" + (no `/` kbd hint here — keep it minimal)
  - Placeholder: "Search decisions… (id, justification, action)"
  - Focus state: border becomes `--gold-soft`
- Spacer (push right)
- Sort label "SORT" (Geist Mono 10px, 0.16em tracked, `--text-3`) + native `<select>` styled to match input
  - Options: Time ↑/↓, Conviction high→low, PnL high→low

**Filter pill row** (`px-5 py-3`, border-bottom): single row of colored-dot pills — `All / Buy / Sell / Hold / Filtered`. Each pill:
- Height 28px, padding `px-2.5`, `border-radius: 9999px` (full pill)
- Inline-flex gap 2: dot + label + count-badge
- Dot: 6×6px circle; filled for action pills (Buy=green, Sell=red, Hold=gray), hollow ring for Filtered
- Label: Geist Mono 11.5px
- Count badge: 16px tall, padding `px-1.5`, `border-radius: 2px`, `rgba(0,0,0,0.35)` background, tabular-nums

**Active state**: pill takes on the action's tinted background + matching border + tinted text. Inactive: transparent background, `--border` border, `--text-2` text.

**Decision density strip** — see § DecisionTimeline below.

**Table**:

| Column | Width | Content |
|---|---|---|
| `#` | 40px | Index, Geist Mono, `--text-3` |
| `TIMESTAMP` | 128px | `HH:MM:SS.mmm` Geist Mono, `--text-2` |
| `PHASE` | 96px | `PhaseChip` |
| `ACTION` | 64px | `ActionPill` (or `—` text-4 if filtered) |
| `CONVICTION` | 112px | `XX%` text + 70px gold-filled progress bar (or `—` if filtered) |
| `JUSTIFICATION` | flex | Truncated single line, `--text-2` (or `—` if filtered) |
| `PNL` | 96px right | `+$X,XXX` in `--gold` / `−$X,XXX` in `--danger` / `—` |

**Row interactions:**
- Click row → calls `onJump(d.i)` (filters the trace dock to this decision)
- Hover row → `--surface-hover` background (skipped when row is focused or filtered)
- Focused row: `--gold-bg` background, `--gold` text on the left rule
- Filtered rows: opacity 0.78, all engaged-only cells show `—` in `--text-4`

### 8. `DecisionTimeline` (density strip) — `app.jsx :: DecisionTimeline`

**New component.** A horizontal density bar that scales from ~10 to ~1000+ decisions without changing layout. Designed for trading runs that emit per-bar/per-tick decisions.

**Layout:**
- Container: 36px tall, full-width, `--surface-elev` background, 1px `--border-soft` border, `border-radius: 3px`
- One thin column per decision, ordered by index
- Column width auto-scales: `min(6, max(1, floor(containerWidth / n)))`
- Gap between columns: 1px when col ≥ 4px, else 0

**Per-decision tick:**
- The hit-area is the full 36px column height (so filtered ticks remain clickable).
- The visible "ink" inside the hit-area depends on phase:
  - **Engaged**: 32px tall filled column, anchored top: 2px, color = action color (`--gold` / `--danger` / `--text-2`)
  - **Filtered**: 10px tall stub, anchored to the bottom (bottom: 1px), color `--text-3`
- Click → `onJump(i)`; Hover → tooltip with `# · timestamp · ACTION · conviction% · justification`

**Header above the strip** (`mb-2.5`):
- Left: `DENSITY` (Geist Mono 10/500, 0.18em tracked, `--text-3`) + `{n} steps · {minute} window`
- Right: 4-swatch legend (buy/sell/hold/filtered) — each a 9×9px square (filtered swatch is a 9×4 ink stub) + label

**Focused-decision marker:** 5px equilateral triangle pointing down, `--gold` filled, positioned above the focused tick at `top: -6px`.

**Filter dimming:** when a filter pill is active (Buy/Sell/Hold/Filtered), non-matching ticks dim to opacity 0.45 and color shifts to `--border-strong`. The density context is preserved — operators can still see the full timeline shape while focused on a subset.

**Tooltip** (on hover):
- Position: `transform: translate(-50%, calc(-100% - 10px))` above the strip, clamped to container width
- Background: `--surface-card`, border: 1px `--border-strong`, shadow `0 8px 20px rgba(0,0,0,0.5)`
- Content: `# {i} · {timestamp} · {ACTION colored by tone} · {conv%}` on row 1; justification line below (truncated to 280px) when present and not filtered

**Why this design over alternatives:**
- Bigger per-decision "squares" with text inside don't scale past ~30 decisions before they wrap or overflow.
- The trace dock uses a similar `density-glyph` pattern (▓▒░) — this is the visual sibling for decisions.
- Filtered ticks must be clickable but quiet: the full-height transparent hit area solves both at once. Don't be tempted to revert to a half-height clickable column — operators couldn't grab it.
- No persistent time labels under the ticks: they were unreadable at tick widths < 8px and added noise. Timestamps live in the tooltip.

### 9. `Meta` card

Right-rail card showing run config as a key/value list. Geist Mono 11px. Each row: 80px UPPERCASE key in `--text-3` (letter-spacing 0.14em) + value in `--text` (`break-all` so long values wrap).

Show these keys (in order): `seed`, `mode`, `region`, `budget`, `commit`, `started`, `duration`.

**Do NOT show**: `run.id`, `strategy`, `scenario`, `agent` — these now live in the H1 + MetaChip row. The ID used to appear in 5 places; now it appears in exactly 2 (TopBar breadcrumb + H1).

### 10. `ReviewPanel` — `app.jsx :: ReviewPanel`

Right-rail card titled "Review", subtitle "supervisor · claude-haiku-4-5", right pill `2 NOTES` in warn-tone (`--warn`). Body:
- Lead paragraph in Geist 15px regular `--text` (the previous theme styled this as serif italic — now plain).
- Two numbered notes in `grid-cols-[60px_1fr]`, Geist Mono 12px. `NOTE 1` / `NOTE 2` labels in `--text-3` 0.16em tracked; body text in `--text` with inline highlight (`<span className="text-gold">#14</span>`).

### 11. Floating Strip — `strip.jsx`

The multi-eval capsule that floats at `bottom: 14px`, horizontally centered. Single-row pill when collapsed (one eval); rounded-rect stack when expanded (one row per concurrent eval).

**Spec preserved from the existing component** — the rebrand only changes colors via CSS variables. Key visual details:
- Background `--surface-elev`, border 1px (color varies by alert state), `backdrop-filter: blur(8px)`, shadow `0 14px 40px rgba(0,0,0,0.55)`
- Border-radius animates: 999px (pill, collapsed) → 12px (card, expanded), 180ms ease
- Status colors per eval lifecycle:
  - `eval` (running): `--info`, pulsing
  - `pass` (completed): `--gold`, static
  - `warn`: `--warn`, static
  - `error`: `--danger`, pulsing — auto-pops the capsule open on new error
  - `queued`: `--text-3`, static

### 12. Trace Dock — `dock.jsx`, `flame.jsx`

The expandable bottom panel that shows the spans flame graph + Inspector. Behavior unchanged from previous version; rebrand only changes colors.

Note the `density-glyph` pattern at the top of the dock — a 7-character row of `▓▒░` block glyphs in graduated greens. This is the visual ancestor of the `DecisionTimeline` density strip and the same idiom (compact span/decision density indicator).

### 13. Control Tower (Home), Strategies, Eval Runs, Lists, Calendar, Mobile, Charts

See the HTML reference files in `references/`. These were all rebranded via:
1. Replacing `styles.css` import with `signal.css`.
2. Replacing the warm color tokens in any inline `:root` blocks with the Signal tokens above.
3. Swapping `'Cormorant Garamond', serif` → `'Geist', sans-serif` and `'JetBrains Mono'` → `'Geist Mono'`.
4. Removing all `font-style: italic` usage (no italics in Signal).
5. For charts: the multi-line color rotation was rebuilt around the Signal lead with the chart palette above.

---

## Interactions & behavior

### Filtering decisions
- Search is debounce-free; filters on every keystroke (small dataset assumption).
- Search hay = `${i} ${timestamp} ${phase} ${action} ${justification}`, all lowercased.
- The 5 action-pill filters are **mutually exclusive radio buttons** (only one active at a time); the pill row IS the filter UI, not a multi-select.
- `Filtered` pill shows phase=filtered decisions. `Buy/Sell/Hold` show engaged decisions of that action only. `All` shows everything.
- Active filter dims non-matching ticks in the density strip; **it does NOT remove them**. Operators retain full-run context.

### Density strip
- ResizeObserver re-measures container width on layout changes and re-renders with appropriate `tickW`.
- Hover state lives in component-local `useState`; tooltip clamped to container width (80px margin from each edge).
- Click any tick → calls `onJump(i)`, same handler as clicking a table row.

### TopBar
- The `EVAL COMPLETED` / `EVAL RUNNING` pill is a status display, not a toggle. Pulsing dot indicates active stream.
- Brand mark + breadcrumb has no nav behavior in this prototype but should link to `/eval/runs` (the parent route) in production.

### MetaChip
- Each chip should be a route-jump or open a picker drawer (`Strategy` → strategy detail, `Scenario` → scenario detail, `Agent` → agent config). The chevron-right glyph is the affordance.

---

## State management

The Eval Run Detail page state (in `app.jsx :: App`):

```ts
isLive: boolean              // live trace mode vs post-hoc
dockOpen: boolean            // is the bottom dock expanded
height: 'sm'|'md'|'lg'       // dock height preset
selected: string | null      // selected span id
liveDur: number              // seconds elapsed if live
toasts: Toast[]
focusedDecision: number      // currently focused decision index

// Filter state (DecisionsTable-local)
search: string
actionFilter: 'all'|'BUY'|'SELL'|'HOLD'|'FILTERED'
sortKey: 'time-asc'|'time-desc'|'conv-desc'|'pnl-desc'

// Trace dock-level filters
query: string                // free-text search across spans
kinds: Set<SpanKind>         // empty = all
status: 'green'|'blue'|'amber'|'red'   // visual filter (legacy "strip state")
```

`onJumpDecision(i)` syncs the focused decision with the dock's `decisionFilter`, so clicking a Decisions row or a density-strip tick instantly filters the trace dock to that decision's spans.

---

## Data shape

`window.MOCK.decisions` is an array of:

```ts
type Decision = {
  i: number;              // decision index (1-based, monotonic per run)
  t: string;              // ISO-ish "HH:MM:SS.mmm"
  phase: 'engaged' | 'filtered';
  action?: 'BUY' | 'SELL' | 'HOLD';   // omitted when phase='filtered'
  conv?: number;          // 0..1, omitted when filtered
  just?: string;          // justification, omitted when filtered
  pnl?: number;           // realized PnL for this step, omitted when filtered
}
```

When `phase === 'filtered'`, the row contains only `i`, `t`, and `phase`. The other fields are intentionally missing because no engaged decision happened.

`window.MOCK.spans` is the flame graph data — see `data.jsx` for the full schema.

---

## Implementation notes

### Drop-in token swap
The CSS variable names are unchanged from the previous theme — `--gold` still means "the brand accent" even though its value is now green. This was intentional to minimize the diff: existing JSX/CSS that referenced `var(--gold)` continues to work. Don't rename `--gold` → `--green` unless you also update every consumer.

### Italic removed everywhere
Sweep for `font-style: italic`, `fontStyle: "italic"`, and Tailwind `italic` className. Replace with weight changes (700 instead of 400-italic) or no change at all. The previous theme used italic for emphasis; Signal uses size and weight only.

### Chart rebrand
For uPlot / KlineCharts / candle charts: the gold-tinted dark grid lines (`#2a2618`, `#3a3322`) must become `--border-soft` / `--border-strong`. Search for these hex codes specifically — they were used in dozens of chart axis configurations.

### Mobile
`signal-mobile.css` mirrors the desktop tokens and adds mobile-specific patterns (themed scrollbars, top bar, drawer, action sheet, density-aware quick chips). Themed scrollbars use a green gradient thumb — this is intentional brand reinforcement and should NOT be replaced with system scrollbars.

### Density strip — implementation hint
Render with absolute positioning inside a relative container, not flex. The auto-scaling `tickW` calculation expects exact pixel layout. Don't use `gap` — use the calculated `slot = tickW + gap` offset. See `DecisionTimeline` in `app.jsx`.

### Geist
Geist supports tabular-nums via OpenType feature `tnum` — apply `font-variant-numeric: tabular-nums` on any numeric column or KPI to lock digit widths. This is essential for the density strip's timestamp tooltip and the entire Decisions table.

---

## Acceptance checklist

When you've ported this to the codebase, verify:

- [ ] All `--gold` hex codes (`#D4A547`, `#B8862E`) replaced with Signal values (`#00E676`, `#00B85F`) throughout `theme.ts` / theme files.
- [ ] All warm dark surfaces (`#0F0E0C`, `#17150F`, etc.) replaced with pure black / cool dark.
- [ ] Cormorant Garamond removed from all font stacks; Geist loaded from Google Fonts; Geist Mono loaded.
- [ ] All `font-style: italic` and Tailwind `italic` classes removed.
- [ ] Chart palette in `chart-data.js` / equivalent uses the Signal multi-hue rotation, not the warm rotation.
- [ ] Monthly returns heatmap renders green for positive / red for negative (no gold).
- [ ] Eval Run Detail TopBar: no POST-HOC toggle, no ⌘K button, no duplicate "Run …" middle section.
- [ ] Body H1 shows only the run ID (no "Run" prefix).
- [ ] Run ID appears in only 2 places: TopBar breadcrumb + body H1. Not in Summary card sub, not in Meta card.
- [ ] MetaChip row under H1 shows Strategy / Scenario / Agent with correct tones (gold / neutral / info).
- [ ] Decisions table has a PHASE column with FILTERED (outlined) and ENGAGED (filled) chips.
- [ ] Filtered rows show `—` in Action/Conviction/Justification/PnL cells, opacity 0.78.
- [ ] Decisions card has search + sort + action-pill filter row.
- [ ] `DecisionTimeline` density strip renders above the table, scales to thousands, click + hover work, filtered ticks are still hit-targetable across full column height.
- [ ] No persistent time labels below the density strip (they're in the hover tooltip only).

---

## Out of scope

- Any backend / Rust binary changes.
- The trace flame-graph internals (`flame.jsx`) — keep behavior; rebrand only changes colors.
- Authentication / session UI.
- The "draft variant" / re-run / compare flows — these are buttons that already exist; they should just inherit the new button styles.

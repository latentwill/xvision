# Design Improvement Sweep — QA Spec

**Date:** 2026-06-08  
**Status:** ready-to-work (A-series), scoped-pending-decision (B7/C-series)  
**Source material:** `docs/design/DesignImprovementSweep/` — 5 standalone HTML prototypes (Moves 04–06 + Accent Explorer + List Ergonomics) and 2 detailed handoff READMEs (`design_handoff_list_ergonomics/README.md`, `design_handoff_optimizer_redesign/README.md`).

Investigation B1–B7 completed 2026-06-08. Findings are folded into each section below. New items A5, A6, and the C-series were derived from the investigation.

---

## Part A — Small items (direct, clear spec, ≤ 1 day each)

These can be worked in parallel by independent agents.

---

### A1 · Legibility token lift (Move 06)

**Source:** `DesignImprovementSweep/XVN Legibility Pass (standalone).html`  
**Effort:** ~1 hour  
**Status:** ready

**Problem:** `--text-3` on `--bg` is ~3.6:1 (below WCAG AA 4.5:1). It carries the heaviest load in the UI — column headers, timestamps, IDs. `--border` hairlines are near-invisible at 1.06:1.

**Dark theme changes** in `frontend/web/src/styles/tokens.css` under `:root` (and `[data-theme="dark"]` if it shadows these):

| Token | Current | Proposed | WCAG on --bg |
|---|---|---|---|
| `--bg` | `#000000` | `#070809` | n/a |
| `--surface-card` | `#0a0a0a` | `#111318` | n/a |
| `--text-3` | `#5f6670` | `#9aa3b2` | ~3.6:1 → ~7:1 |
| `--text-2` | `#9ca3af` | `#aeb6c2` | higher |
| `--border` | `#1a1a1a` | `#2c313b` | hairline → readable |

**Light theme note** (from B6): Light theme also needs a parallel pass. It's token-driven so the same approach applies. Additional constraint: once the heatmap KPI tiles ship, their low-alpha fills (`gold-bg 0.10`, `info testing cells`) must be checked on the light background — bump alpha or go border-forward if they wash out. Include light theme tuning in this PR; it's the same one-file change.

**Target file:** `frontend/web/src/styles/tokens.css`

**Acceptance criteria:**
- [ ] Dark `--text-3` on `--bg` ≥ 4.5:1 (WCAG AA small text)
- [ ] Dark `--border` visibly separates rows in dense tables
- [ ] Light theme receives parallel contrast review; no new wash-out on light surfaces
- [ ] No layout changes — tokens only

---

### A2 · Focal metric on eval-run detail header (Move 04)

**Source:** `DesignImprovementSweep/XVN Focal Metric (standalone).html`  
**Effort:** ~½ day  
**Status:** ready

**Problem:** `eval-runs-detail.tsx` line 261 leads with the ULID as `<h1>`. Total return / Sharpe / drawdown are small equal-weight figures. The reader hunts for the verdict.

**Change:** Layout + type-scale only. No new colors.

- ULID → breadcrumb row (~line 258)
- Total return → `<h1>` at `text-5xl font-bold text-pos tabular-nums`
- Supporting stat rail: Sharpe · Max drawdown · Win rate · Trades · Cost — `text-sm text-text-2`, single horizontal row
- Equity curve inline on the right of the header (already available via `getRunChart`)
- Mobile (`eval-runs-detail-mobile.tsx`): stack vertically, equity curve below stat rail

**Target files:**
- `frontend/web/src/routes/eval-runs-detail.tsx` (~line 252–300)
- `frontend/web/src/routes/eval-runs-detail-mobile.tsx`

**Acceptance criteria:**
- [ ] Total return is the largest text element in the header
- [ ] ULID in breadcrumb, not H1
- [ ] Stat rail is single horizontal row on desktop
- [ ] Equity curve visible in header on desktop
- [ ] Mobile: no truncation, single-column stacking
- [ ] No new tokens introduced

---

### A3 · Optimizer phantom Start — kill the scroll-only button

**Source:** `DesignImprovementSweep/design_handoff_optimizer_redesign/README.md`  
**Effort:** ~30 min  
**Status:** ready

**Problem:** `OptimizerHome.tsx:192–193` — `ConfigureSection`'s `Start` button calls `document.getElementById("optimizer-run-controls").scrollIntoView()` and does NOT start a run. The real launch is `LaunchStrip` in `LiveCycleView.tsx`.

**Change:** Delete the `scrollIntoView` button from `ConfigureSection` (~line 191–199). Do not attempt the launch drawer consolidation yet (that's C1).

Also: update `ConfigureSection`'s primary button to `bg-accent text-on-accent` (instead of any `bg-gold` styling) — this exercises the accent token wiring from A5 and ensures the optimizer uses `--accent` for interactive chrome, keeping `--gold` for running-state signals. Gate this on A5 completing first.

**Target file:** `frontend/web/src/features/autooptimizer/screens/OptimizerHome.tsx` (~line 169–210)

**Acceptance criteria:**
- [ ] No `scrollIntoView` in `ConfigureSection`
- [ ] Page renders correctly; `LaunchStrip` unaffected
- [ ] Primary action buttons in `ConfigureSection` use `bg-accent text-on-accent`

**Dependency:** A5 (accent token wiring) should land first so `bg-accent` is available in Tailwind.

---

### A4 · List ergonomics — column picker + scroll affordance

**Source:** `DesignImprovementSweep/design_handoff_list_ergonomics/README.md`  
**Effort:** ~2–3 days  
**Status:** ready

Full implementation spec is in the handoff README. Summary:

**Files:**

| File | Action |
|---|---|
| `src/components/lists/useListState.ts` | Extend `ListColumn` type; add `useListColumns` hook |
| `src/components/lists/ListCard.tsx` | Scroll affordance (fades, sticky, nudge arrows); wire column visibility |
| `src/components/lists/ListToolbar.tsx` | Add Columns menu button using `SignalMenu` |
| `src/components/lists/ResponsiveListCard.tsx` | Pass column-picker props through |
| `src/routes/eval-runs.tsx` | Add metadata to each column; fold ULID into name cell |
| `src/components/primitives/SignalMenu.tsx` | Reuse as picker popover (likely no changes) |

**`ListColumn` type extensions:**
```ts
key: string;
essential?: boolean;    // always visible, locked in picker
defaultOff?: boolean;   // hidden by default
priority?: number;      // higher = drop first on auto-hide
estWidth?: number;      // estimated px width
```

**`useListColumns(listId)` hook:**
- `visibleKeys: Set<string>` — localStorage at `xvn:list:${listId}:columns`
- `toggle(key)`, `reset()`, `isEssential(key)` — parse failures fall back to defaults silently

**Scroll affordance (`ListCard.tsx`):**
- `ResizeObserver` on scroll container
- Left/right gradient fades when overflowing; sticky `<thead>`
- Nudge arrows: `scrollBy({ left: ±240, behavior: 'smooth' })`; hidden at boundaries
- Auto-hide: when container overflows, drop non-essentials by `priority` (highest first). Auto-hidden columns remain in picker tagged "auto"
- Respect `prefers-reduced-motion`

**Acceptance criteria:**
- [ ] Columns button in eval-runs toolbar; picker opens on click
- [ ] Toggling a column persists across reload
- [ ] Essential columns (Run ID) cannot be hidden
- [ ] Auto-hide engages on overflow; "auto" tag visible in picker
- [ ] Left/right fade overlays on overflow
- [ ] Nudge arrows scroll 240px; hide at boundaries
- [ ] Sticky header on vertical scroll
- [ ] ULID folded into name cell

---

### A5 · Wire `--accent` / `--on-accent` tokens into Tailwind + tokens.css

**Source:** B1 investigation finding  
**Effort:** ~1 hour  
**Status:** ready — prerequisite for A3 and C1

**Finding (from B1):** `--accent`/`--on-accent` CSS vars are defined in `theme/themes.ts` and injected at runtime by `ThemeProvider`, but they are **not in `tailwind.config.ts`** and **not in `tokens.css`**. This means `bg-accent` / `text-on-accent` Tailwind utilities don't exist today. `OptimizerHome.tsx:141/212` and `RunDetail`, `ScheduleStrip` reference them — confirm whether those are painting correctly or silently falling back.

**Change:**
1. Add to `tailwind.config.ts` `theme.extend.colors`:
   ```ts
   accent: "var(--accent)",
   "on-accent": "var(--on-accent)",
   ```
2. Add fallback values to `tokens.css` `:root` (so the tokens resolve before ThemeProvider mounts):
   ```css
   --accent: #00e676;        /* matches current --gold dark value */
   --on-accent: #000000;     /* dark text on green accent */
   ```
3. Verify `[data-theme="light"]` in `tokens.css` has a light-appropriate `--accent` (e.g. `#00A15C` from `themes.ts` — confirm they match)

**Target files:**
- `frontend/web/tailwind.config.ts`
- `frontend/web/src/styles/tokens.css`

**Acceptance criteria:**
- [ ] `bg-accent` utility resolves to the theme accent color in both dark and light
- [ ] `text-on-accent` resolves correctly (readable text on accent background)
- [ ] Existing `bg-accent text-on-accent` usages in `OptimizerHome.tsx`, `RunDetail`, `ScheduleStrip` are visually unchanged (they already painted via ThemeProvider vars, just making Tailwind aware)
- [ ] No new visual regressions

---

### A6 · Extract `useCycleEventStream` hook from LiveCycleView

**Source:** B3 investigation finding  
**Effort:** ~½ day  
**Status:** ready — prerequisite for C1 (optimizer decomposition)

**Finding (from B3):** The SSE connection in `LiveCycleView.tsx` lives at lines 1035–1052 in a single self-contained `useEffect`: opens `EventSource("/api/autooptimizer/events")`, registers `SSE_EVENT_NAMES` listeners, parses via `parseSsePayload`, pushes via `appendEvent`. No other subscribers to this endpoint. The running-state derivation (`isRunning`, `activeCycleId`) is computed from the event buffer at the component root.

**Change:** Extract into `features/autooptimizer/hooks/useCycleEventStream.ts`:
```ts
function useCycleEventStream(): { events: OptimizerEvent[]; connected: boolean }
```
Move the `isRunning`/`activeCycleId` derivation into the hook so the heatmap and command bar read one source. Extract the auto-relaunch loop as a separate optional `useOptimizerLoop()` hook.

Keep `LiveCycleView.tsx` consuming the new hook — no UI changes in this PR. The extracted hook is the seam that makes the optimizer zone decomposition in C1 safe.

**Target files:**
- New: `frontend/web/src/features/autooptimizer/hooks/useCycleEventStream.ts`
- Modified: `frontend/web/src/features/autooptimizer/LiveCycleView.tsx` (consume the hook)

**Acceptance criteria:**
- [ ] `LiveCycleView.test.tsx` passes without modification
- [ ] SSE behavior is unchanged (same connection, same event types)
- [ ] `useCycleEventStream` is exported and importable from other optimizer components
- [ ] No UI changes; this is a pure refactor

---

## Part C — Scoped multi-day work (post-investigation)

These are larger items now fully scoped after the B-series. Each is a standalone track.

---

### C1 · Optimizer surface redesign — 3-zone layout

**Source:** `DesignImprovementSweep/design_handoff_optimizer_redesign/README.md`  
**Effort:** ~1 week  
**Status:** ready to spec after A3, A5, A6 land  
**Dependencies:** A3 (phantom Start), A5 (accent tokens), A6 (SSE hook)

**Architecture (from B3/B4/B5 findings):**

The redesign proceeds in this order. Each phase is a PR:

**Phase 1 — Foundation (A6 already covers this):** `useCycleEventStream` hook extracted.

**Phase 2 — Zone 1: Command bar + Zone 5: Launch drawer (~2 days)**
- `OptimizerHome.tsx`: replace today's stacked layout with the 3-zone shell
- Zone 1 command bar: state pill + active-run identity + contextual primary button (`Launch run` / `Pause` / `Resume` / `Cancel`)
- Zone 5 launch drawer: merge `ConfigureSection + LaunchStrip + ScheduleStrip` into one inline-expanding drawer
  - `ScheduleStrip` is functional; drop in as "Schedule" tab unchanged (B5 finding)
  - `ModePicker` is standalone; embed in the drawer
  - Single real `startRunCycle()` submit; delete the `scrollIntoView` path (already done in A3)
  - Strategy `<select>` + budget + window fields from `LaunchStrip`; model pickers from `preferences.ts`
- Remove `Configure run` anchor link from header

**Phase 3 — Zone 2: Phase spine + Zone 3: Live work band (~2 days)**
- Promote `PhaseStepper.tsx` to full-width strip under command bar (visible when running)
- Zone 3: 2-up grid replacing old `[300px·1fr·260px]`
  - Left (~60%): `LiveEvalHeatmap` (new component — see below)
  - Right (~40%): `EventLogCard` from `LiveCycleView` + `LiveCostTicker`
- `KeptNextCard` rail removed; its data moves to Zone 4

**Phase 4 — Zone 4: Outcome strip + Zone 6: History (~1 day)**
- Zone 4: `grid-cols-2 sm:grid-cols-4` KPI tiles (Kept/Suspect/Dropped/Top Δ)
- Zone 6: keep `RecentCyclesTable`; delete `RecentCyclesSectionFull` from `LiveCycleView`; add Runs ⇄ Cycles toggle

**LiveEvalHeatmap component (new, from B4 finding):**
`EvalMatrix.tsx` is a static numeric table (done/queued only, no animation, cycle-detail data). It is NOT ready to promote as the live anchor. Build a new `panels/LiveEvalHeatmap.tsx`:
- Takes per-cell live state from `useCycleEventStream` events (`queued → testing → done/failed`)
- Reuses `EvalMatrix`'s regime-union layout logic
- Animated `HeatCell` with `bar-flow` shimmer from `ar-home.jsx` (in `docs/design/XVN_optimizer/`)
- `done` → gold fill; `testing` → info-blue shimmer; `queued` → surface-panel; `failed` → danger
- Keep `EvalMatrix` as-is for the cycle-detail (completed runs) view

**State management:** No new global state — all from `useOptimizerStatus()`, `useOptimizerStats()`, `useSessionList()`, `useCycleEventStream()`.

---

### C2 · Chart craft — dashboard "Chart snapshot" upgrade (Move 05)

**Source:** `DesignImprovementSweep/XVN Chart Craft (standalone).html`  
**Effort:** ~1–2 days  
**Status:** needs surface identification before work starts

**Finding (from B2):** No component is literally named "Chart snapshot." Two distinct chart systems exist:
- `chart-v2` (`components/chart/v2/…`, `useChart2Theme`, chart2 tokens) — main trading/kline charts
- `usePlotSimple` (`features/autooptimizer/ui/usePlotSimple.ts`) — optimizer-specific, intentionally minimal and independent of chart-v2

For the optimizer redesign: **do not introduce chart2 tokens** into optimizer charts. Use `usePlotSimple` + standard tokens.

**The "Chart snapshot" surface** is one of these candidates on the home dashboard:
- `OptimizerDigestStrip`
- `LiveStrategiesSection`
- `StrategyOutcomesList`

**Blocked on:** Identifying the exact home surface that shows a flat equity line today. Once identified, the upgrade is:
- Compose chart2 primitives: gradient fill + drawdown pane + buy/sell/veto/hold markers + SMA 20/50 + monthly returns heat ramp
- Use existing `Chart2ThemeDefinition` tokens from `themes.ts` (equityTop, drawdown, markerBuy/Sell/Veto/Hold, sma20/50, CHART2_HEAT_RAMP)
- Same tokens, composition only — no new colors

**Next step:** Identify the component (a 30-min file read). Once found, add to this section and mark ready.

---

### C3 · Accent color picker in Settings

**Source:** `DesignImprovementSweep/XVN Accent Explorer (standalone).html`  
**Effort:** ~1 day  
**Status:** ready (decision made: user-selectable in settings)  
**Dependencies:** A5

**Presets (from the Explorer):**

| Key | Dark | Light | Character |
|---|---|---|---|
| `green` | `#00e676` | `#00A15C` | Current default (keep as option) |
| `azure` | `#3B82F6` | `#2563EB` | Classic interactive blue |
| `cyan` | `#22D3EE` | `#0E96B3` | Electric, data-signal |
| `teal` | `#14C8AE` | `#0D9488` | Calm blue-green |
| `amber` | `#F5A524` | `#B7770C` | Warm, premium |
| `magenta` | `#D946EF` | `#A21CAF` | Bold, distinctive |
| `mono` | `#9ca3af` | `#6b7280` | Off-white, minimal |

Each preset also needs an `--on-accent` (text on the accent background): dark presets use `#000000`; light presets use `#ffffff`. Exception: `mono` light uses `#000000`.

**Implementation:**

**1. `frontend/web/src/theme/themes.ts`** — add:
```ts
export const ACCENT_PREFERENCE_KEY = "xvn.accent.preference";
export type AccentKey = "green" | "azure" | "cyan" | "teal" | "amber" | "magenta" | "mono";
export const ACCENT_PRESETS: Record<AccentKey, { dark: string; light: string; onAccentDark: string; onAccentLight: string; label: string }> = { /* table above */ };
export function coerceAccentPreference(raw: string | null): AccentKey { /* default "green" */ }
```

**2. New `frontend/web/src/theme/useAccent.ts`** — mirrors `useTheme.ts` pattern exactly:
- Module-level snapshot + `useSyncExternalStore`
- `safeStorageGet/Set(ACCENT_PREFERENCE_KEY, ...)`
- Exports `useAccent()` → `{ accentKey, setAccent }`

**3. `frontend/web/src/theme/ThemeProvider.tsx`** — extend `useEffect` to also write accent vars:
```ts
const { accentKey } = useAccent();
// inside useEffect:
const preset = ACCENT_PRESETS[accentKey];
const isDark = definition.mode === "dark";
root.style.setProperty("--accent", isDark ? preset.dark : preset.light);
root.style.setProperty("--on-accent", isDark ? preset.onAccentDark : preset.onAccentLight);
```
Add `accentKey` and `definition.mode` to the `useEffect` deps array.

**4. `frontend/web/src/routes/settings/general.tsx`** — add accent row below the existing theme toggle:
- Section label: "Accent color"
- Render one swatch button per preset key: filled circle (`w-5 h-5 rounded-full`) in the preset's dark color + label below
- Selected state: `ring-2 ring-offset-2 ring-border-strong`
- On click: `setAccent(key)`
- No popup, no sheet — inline row of swatches, same pattern as the theme radio group

**Target files:**
- `frontend/web/src/theme/themes.ts`
- New: `frontend/web/src/theme/useAccent.ts`
- `frontend/web/src/theme/ThemeProvider.tsx`
- `frontend/web/src/routes/settings/general.tsx`

**Acceptance criteria:**
- [ ] Accent swatches visible in Settings → General, below theme toggle
- [ ] Selecting a swatch updates `--accent`/`--on-accent` immediately (no page reload)
- [ ] Selection persists across reload (localStorage)
- [ ] Both dark and light themes use the correct per-theme color value
- [ ] Default is `green` (no visual regression for existing users)
- [ ] All existing `bg-accent`/`text-accent`/`text-on-accent` usages pick up the new color
- [ ] `--gold` (running state, kept counts) is visually unaffected

---

## Summary table

| ID | Title | Type | Effort | Deps | Status |
|---|---|---|---|---|---|
| A1 | Legibility token lift (+ light theme) | Small | ~1h | — | ready |
| A2 | Focal metric on eval-run detail | Small | ~½d | — | ready |
| A3 | Optimizer phantom Start delete | Small | ~30min | A5 | ready (A5 first) |
| A4 | List ergonomics (column picker + scroll) | Small | ~2–3d | — | ready |
| A5 | Wire --accent/--on-accent into Tailwind | Small | ~1h | — | ready |
| A6 | Extract useCycleEventStream hook | Small | ~½d | — | ready |
| C1 | Optimizer 3-zone layout redesign | Large | ~1 week | A3+A5+A6 | ready to spec |
| C2 | Chart craft: dashboard chart upgrade | Medium | ~1–2d | — | blocked (surface ID) |
| C3 | Accent color picker in Settings | Medium | ~1d | A5 | ready |

**Recommended execution order:**
1. **No-risk, parallel:** A1, A4, A5
2. **After A5:** A3, A2
3. **After A6 (pre-flight):** Begin C1 phase 2
4. **Unblocked separately:** C2 (surface ID first), C3 (after user picks accent)

---

## Agent instructions

**A-series items:** Read the source prototype HTML (open in browser) and/or the handoff README. Identify the exact lines to change. Do not refactor surrounding code. Write tests if the change touches logic; no tests for pure CSS token changes. One PR per item.

**C-series items:** Read the full optimizer handoff README at `design_handoff_optimizer_redesign/README.md` before touching any file. Work in phases as described in C1. Do not combine phases into one PR.

**Design source of truth:** `docs/design/DesignImprovementSweep/` — standalone HTML prototypes are canonical for visual treatment; the two handoff READMEs are canonical for implementation spec.

**Token discipline:** `--gold` stays as the running-state / kept-signal color. `--accent` is the interactive chrome. Never use `--pos`/`--neg` for UI chrome (those are strictly money: gains/losses).

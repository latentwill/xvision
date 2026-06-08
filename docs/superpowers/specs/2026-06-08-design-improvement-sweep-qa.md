# Design Improvement Sweep — QA Spec

**Date:** 2026-06-08  
**Status:** ready-to-work  
**Source material:** `docs/design/DesignImprovementSweep/` — 5 standalone HTML prototypes (Moves 04–06 + Accent Explorer + List Ergonomics) and 2 detailed handoff READMEs (`design_handoff_list_ergonomics/README.md`, `design_handoff_optimizer_redesign/README.md`).

Each prototype is a self-contained browser demo. Open them to see before/after. The READMEs contain pixel-level implementation specs.

---

## Part A — Small items (direct, clear spec, ≤ 1 day each)

These can be worked in parallel by independent agents. Each has a complete spec, specific file targets, and no open architecture questions.

---

### A1 · Legibility token lift (Move 06)

**Source:** `DesignImprovementSweep/XVN Legibility Pass (standalone).html`  
**Effort:** ~1 hour, one file.

**Problem:** `--text-3` on `--bg` is ~3.6:1 (below WCAG AA 4.5:1 threshold). It carries the heaviest load in the UI — column headers, timestamps, IDs, eyebrows. `--border` hairlines are near-invisible at 1.06:1.

**Change:** Three token value changes in `frontend/web/src/styles/tokens.css` under `:root` (and `[data-theme="dark"]` if it shadows these):

| Token | Current | Proposed | WCAG on --bg |
|---|---|---|---|
| `--bg` | `#000000` | `#070809` | n/a |
| `--surface-card` | `#0a0a0a` | `#111318` | n/a |
| `--text-3` | `#5f6670` | `#9aa3b2` | ~3.6:1 → ~7:1 |
| `--text-2` | `#9ca3af` | `#aeb6c2` | — → higher |
| `--border` | `#1a1a1a` | `#2c313b` | hairline → readable |

**Target file:** `frontend/web/src/styles/tokens.css`

**Acceptance criteria:**
- [ ] Dark theme `--text-3` on `--bg` ≥ 4.5:1 (WCAG AA small text)
- [ ] Dark theme `--border` visibly separates rows in dense tables
- [ ] Light theme tokens are unchanged (they have separate values)
- [ ] No layout changes — tokens only

**Note on scope:** This is a global change — all dense surfaces (eval runs, decisions, marketplace, settings) get the lift automatically. That's the point.

---

### A2 · Focal metric on eval-run detail header (Move 04)

**Source:** `DesignImprovementSweep/XVN Focal Metric (standalone).html`  
**Effort:** ~½ day, 2 files.

**Problem:** The eval-run detail header (`eval-runs-detail.tsx`, line 261) leads with the ULID as its `<h1>` and renders total return / Sharpe / drawdown as small, equal-weight figures in a flat row. The reader hunts for the verdict.

**Change:** Layout + type-scale change only. No new colors beyond the locked palette.

- ULID moves to the breadcrumb (already at line ~258, the `{/* Body header */}` section)
- Total return becomes the `<h1>` at display scale: `text-5xl font-bold text-pos tabular-nums`
- Supporting stat rail below: Sharpe · Max drawdown · Win rate · Trades · Cost — `text-sm text-text-2`
- Equity curve inline at the right of the header (already available via `getRunChart`)
- On mobile (`eval-runs-detail-mobile.tsx`): stack vertically, equity curve below the stat rail

**Target files:**
- `frontend/web/src/routes/eval-runs-detail.tsx` (~line 252–300, the header section)
- `frontend/web/src/routes/eval-runs-detail-mobile.tsx` (mobile header)

**Acceptance criteria:**
- [ ] Total return is the largest text element in the header (`text-5xl` or equivalent)
- [ ] ULID appears in the breadcrumb row, not as H1
- [ ] Sharpe, MaxDD, Win rate, Trades, Cost are in a single horizontal stat rail
- [ ] Equity curve is visible in the header on desktop
- [ ] Mobile: single-column stacking, no truncation
- [ ] No new color tokens introduced

---

### A3 · Optimizer phantom Start — kill the scroll-only button (P0 of optimizer redesign)

**Source:** `DesignImprovementSweep/design_handoff_optimizer_redesign/README.md`  
**Effort:** ~30 min, one file.

**Problem:** `OptimizerHome.tsx` line 192–193: the `Start` button inside `ConfigureSection()` calls `document.getElementById("optimizer-run-controls").scrollIntoView()` — it does NOT start a run. The real launch is `LaunchStrip` inside `LiveCycleView.tsx`. The phantom button is confusing.

**Change (P0 only — do not tackle the full redesign yet):**
- Delete the `scrollIntoView` `Start` button from `ConfigureSection` (line ~191–199)
- Optionally: add a clear comment that `LaunchStrip` inside `LiveCycleView` is the real launch path
- Do NOT attempt to merge `ConfigureSection + ScheduleStrip + LaunchStrip` yet (that's the larger P0/P1 work in I5)

**Target file:** `frontend/web/src/features/autooptimizer/screens/OptimizerHome.tsx` (~line 169–210)

**Acceptance criteria:**
- [ ] No `scrollIntoView` code in `ConfigureSection`
- [ ] The page still renders correctly (ConfigureSection can remain as a config display, just without the phantom button)
- [ ] The real launch flow in `LiveCycleView` / `LaunchStrip` is unaffected

---

### A4 · List ergonomics — column picker + scroll affordance

**Source:** `DesignImprovementSweep/design_handoff_list_ergonomics/README.md`  
**Effort:** ~2–3 days, 6 files.

This is the most detailed small-item spec — the README is a complete implementation document. Summary below; agents must read the full README.

**Files:**

| File | Action |
|---|---|
| `src/components/lists/useListState.ts` | Extend `ListColumn` type; add `useListColumns` hook |
| `src/components/lists/ListCard.tsx` | Add scroll affordance (fades, sticky, nudge arrows), wire column visibility |
| `src/components/lists/ListToolbar.tsx` | Add Columns menu button using `SignalMenu` |
| `src/components/lists/ResponsiveListCard.tsx` | Pass column-picker props through |
| `src/routes/eval-runs.tsx` | Add metadata to each column; fold ULID into name cell |
| `src/components/primitives/SignalMenu.tsx` | Reuse as column picker popover (likely no changes needed) |

**`ListColumn` type extensions:**
```ts
interface ListColumn {
  // existing fields ...
  key: string;
  essential?: boolean;       // always visible, locked in picker
  defaultOff?: boolean;      // hidden by default (opt-in)
  priority?: number;         // higher = drop first during auto-hide
  estWidth?: number;         // estimated px width for auto-hide math
}
```

**`useListColumns` hook (new, in `useListState.ts`):**
- `visibleKeys: Set<string>` — from localStorage at `xvn:list:${listId}:columns`
- `toggle(key: string)` — persists immediately; essentials are unclearable
- `reset()` — clears stored key, reverts to defaults
- `isEssential(key: string)` — returns true for locked columns
- Parse failures fall back to defaults silently

**Scroll affordance (`ListCard.tsx`):**
- `ResizeObserver` on the scroll container
- Left/right gradient fade overlays when `scrollLeft > 0` / `scrollLeft < scrollWidth - clientWidth`
- Sticky `<thead>` row
- Nudge arrows: click = `scrollBy({ left: ±240, behavior: 'smooth' })`, hidden at boundaries
- Auto-hide: when container width < sum of `estWidth` of visible columns, drop non-essential columns by priority (highest priority number = first to drop). Auto-hidden columns remain in the picker tagged "auto" — nothing lost.
- Recompute on resize via the existing `ResizeObserver`
- Respect `prefers-reduced-motion` for smooth scroll

**Column picker popover:**
- Triggered by Columns button in `ListToolbar` (shows badge count of hidden columns)
- Uses `SignalMenu` or equivalent popover primitive
- Checkboxes for each non-essential column, toggle on click
- Essential columns shown locked (greyed checkbox)
- Reset link at bottom

**Acceptance criteria:**
- [ ] Columns button appears in eval-runs toolbar
- [ ] Toggling a column persists across page reload
- [ ] Essential columns (Run ID) cannot be hidden
- [ ] Auto-hide engages when table overflows container; tagged "auto" in picker
- [ ] Left/right fade overlays appear when horizontally scrollable
- [ ] Nudge arrows scroll 240px; hide at boundaries
- [ ] Sticky header holds on vertical scroll
- [ ] `prefers-reduced-motion` disables smooth-scroll
- [ ] ULID is folded into the name cell (not a separate column)

---

## Part B — Investigation questions

These require research before implementation can be scoped. Each item identifies what needs to be understood and what decision must come out of the investigation. Write findings as a note appended to this spec or as a separate linked doc.

---

### B1 · Accent system: is `--gold` the right token name for interactive accent?

**Source:** `DesignImprovementSweep/XVN Accent Explorer (standalone).html`

**What the prototype shows:** 5 brand accent presets (Azure / Cyan / Teal / Amber / Magenta / Mono) — each driving nav, links, focus rings, ⌘K, and decision markers. The prototype treats this as a conceptually separate `--accent` token, distinct from `--pos`/`--neg` (gain/loss green/red) and `--warn`/`--danger`.

**The tension:** `tokens.css` comments say "Token NAMES are unchanged — `--gold` is the brand accent." But the Accent Explorer imagines swapping the brand color without touching money signals. If `--gold` is used interchangeably for both "running optimizer state" and "interactive UI accent", they can't be cleanly separated.

**What to investigate:**
1. Run `grep -rn "\-\-gold\b" frontend/web/src/` — how many uses? Sort into semantic buckets: nav/link/focus vs running/kept/optimizer-active vs decorative
2. Check `tailwind.config.ts` for `gold` utility mappings
3. Answer: **Can `--gold` already be treated as pure interactive accent, with `--pos` covering gains?** Or are there places where it means "running" (a machine-state semantic that shouldn't change with brand color)?
4. If yes to clean split: **is a `--accent: var(--gold)` alias additive and safe?** (enables future swapping without touching all callsites)

**Decision needed:** Whether to introduce `--accent` as an alias now, defer, or declare `--gold` is already the correct name and the Explorer is aspirational only.

---

### B2 · Chart craft: what renders the dashboard "Chart snapshot" card?

**Source:** `DesignImprovementSweep/XVN Chart Craft (standalone).html` (Move 05)

**What the prototype shows:** The dashboard's "Chart snapshot" card currently shows a flat single-line equity curve with a simple gradient fill. Move 05 proposes composing the full chart2 vocabulary onto it: gradient fill + drawdown pane + buy/sell/veto/hold decision markers + SMA 20/50 lines + monthly returns heat ramp.

**What to investigate:**
1. `frontend/web/src/routes/home.tsx` uses `getRunChart()` (line 51) — trace to the component that renders it. What chart primitive does it use? (KlineChart? inline SVG? uPlot?)
2. Does the home chart already use `Chart2ThemeDefinition` tokens from `themes.ts`?
3. The chart2 vocabulary (`equityTop`, `drawdown`, `markerBuy/Sell/Veto`, `sma20/50`, `CHART2_HEAT_RAMP`) is defined in `frontend/web/src/theme/themes.ts` — are there chart2 rendering components that accept these tokens, or are they only used in the chart-lab routes?
4. How does the "Chart snapshot" in `home.tsx` differ from the eval-runs-detail equity chart? Are they the same component?

**Decision needed:** Whether the dashboard chart needs a component-level upgrade (new chart2 composition) vs a simpler token wiring, and which component is the right upgrade target.

---

### B3 · Optimizer redesign: SSE wiring stability in LiveCycleView decomposition

**Source:** `DesignImprovementSweep/design_handoff_optimizer_redesign/README.md`

**What the spec proposes:** Decompose `LiveCycleView.tsx` (1163 lines) — keep the SSE/event logic, replace the 3-column `[300px·1fr·260px]` layout with the new Live zone. Delete its duplicate `RecentCyclesSectionFull`.

**What to investigate:**
1. How is the SSE connection structured in `LiveCycleView.tsx`? Is `EventSource` instantiated inside the component or in a custom hook (e.g., `useOptimizerEvents`)?
2. Can the event buffer be extracted to a standalone hook without breaking the current tests (`LiveCycleView.test.tsx`)?
3. Are there other subscribers to `/api/autooptimizer/events` beyond `LiveCycleView`? (Search for `autooptimizer/events` in the codebase.)
4. What is the event buffer type? (Check for `OptimizerEvent` type or similar in `features/autooptimizer/api.ts`)
5. How does `EvalMatrix.tsx` currently get its data? Does it consume SSE events or query a separate endpoint?

**Decision needed:** Whether to extract SSE logic into a custom hook first (pre-flight for the full redesign) or proceed directly to the zone-by-zone decomposition described in the README.

---

### B4 · Optimizer redesign: EvalMatrix current state and heatmap viability

**Source:** `DesignImprovementSweep/design_handoff_optimizer_redesign/README.md` (Zone 3)

**What the spec proposes:** `panels/EvalMatrix.tsx` becomes the live heatmap anchor. Each cell shows an experiment×regime backtest state: `done` (gold fill), `testing` (info-blue shimmer), `queued` (surface-panel), `failed` (danger). The animated shimmer is to be ported from `Autoresearch.html` → `ar-home.jsx` `HeatCell`.

**What to investigate:**
1. Open `frontend/web/src/features/autooptimizer/panels/EvalMatrix.tsx` — what does it currently render? Is it an active component or a stub?
2. Check `EvalMatrix.test.tsx` for coverage of current behavior
3. Where is `Autoresearch.html` / `ar-home.jsx`? Check `docs/design/XVN_optimizer/` — is `ar-home.jsx` there?
4. Does `EvalMatrix` already accept experiments×regimes data, or does it need a new data shape?
5. Check what `useCycleRuns()` returns — does it expose per-regime eval state that maps to the heatmap cells?

**Decision needed:** Whether `EvalMatrix` is ready to be promoted as-is (just repositioned) or needs a significant data/rendering upgrade first.

---

### B5 · Optimizer launch consolidation: what are the 3 surfaces and is ScheduleStrip functional?

**Source:** `DesignImprovementSweep/design_handoff_optimizer_redesign/README.md` (Zone 5)

**What the spec proposes:** Merge `ConfigureSection + ScheduleStrip + LaunchStrip` into one inline launch drawer. This is the P0/P1 core work.

**What to investigate:**
1. `features/autooptimizer/ui/ScheduleStrip.tsx` — does it work end-to-end or is it a UI stub? Is there a schedule endpoint in `api.ts`?
2. `features/autooptimizer/ui/ModePicker.tsx` — is it a standalone component or embedded inside `LaunchStrip`?
3. In `features/autooptimizer/api.ts` — what does `startRunCycle()` accept? Does it take schedule parameters today, or is that missing from the API?
4. `features/autooptimizer/screens/OptimizerHome.tsx` line 169 — how large is `ConfigureSection`? What fields does it render that `LaunchStrip` doesn't already cover?
5. Does `preferences.ts` already persist model overrides for the experiment-writer and reviewer model pickers?

**Decision needed:** Whether the launch drawer consolidation is additive (wrap existing components) or requires deleting and rewriting `ConfigureSection` from scratch. This affects whether it's a 1-day or 3-day task.

---

### B6 · Legibility lift: light theme impact

**Source:** `DesignImprovementSweep/XVN Legibility Pass (standalone).html`

**What the prototype shows:** The legibility pass focuses on dark theme. Light theme exists (`[data-theme="light"]` in `tokens.css`).

**What to investigate:**
1. What are the current light theme values for `--text-3`, `--border`, `--bg`?
2. Run the same WCAG math: `ratio("--text-3", "--bg")` in light mode — does it also fail 4.5:1?
3. Is the light theme actively used in the dashboard (accessible to users) or is it dev-only?

**Decision needed:** Whether the light theme needs a parallel lift (A1 scope doubles) or is out of scope for this wave.

---

### B7 · Scope: what does the Accent Explorer actually require as deliverable?

**Source:** `DesignImprovementSweep/XVN Accent Explorer (standalone).html`

**What the prototype shows:** An interactive tool to pick brand color, showing it in light and dark themes across a full dashboard layout. 5 presets: Azure, Cyan, Teal, Amber, Magenta, Mono.

**The question:** Is this prototype asking for:
- (a) A token change — pick one accent color and apply it (and if so, which one?)
- (b) A runtime switcher — add a color-picker to settings
- (c) A developer reference only — no production deliverable yet

The prototype itself is a design decision tool, not a deployment plan. Without knowing which accent to ship, there's no implementation task.

**What to investigate:**
1. Is there any existing `--accent` token in `tokens.css`? (From the grep, `--gold` is the accent today.)
2. Has a specific accent been chosen, or is the Explorer still open for user selection?
3. Check `B1` first — the accent system and `--gold` aliasing question is the precondition.

**Decision needed:** Owner (user/designer) must select which accent preset to ship (or confirm no change to current green). Block on user input before any implementation.

---

## Summary table

| ID | Title | Type | Effort | Dependencies | Status |
|---|---|---|---|---|---|
| A1 | Legibility token lift | Small | ~1h | — | ready |
| A2 | Focal metric on eval-run detail | Small | ~½d | — | ready |
| A3 | Optimizer phantom Start delete | Small | ~30min | — | ready |
| A4 | List ergonomics (column picker + scroll) | Small | ~2–3d | — | ready |
| B1 | Accent system: `--gold` vs `--accent` | Investigation | ~1h | — | open |
| B2 | Chart craft: dashboard chart snapshot | Investigation | ~1h | — | open |
| B3 | Optimizer SSE wiring stability | Investigation | ~1h | B4 | open |
| B4 | EvalMatrix current state + heatmap | Investigation | ~1h | — | open |
| B5 | Launch drawer: 3 surfaces viability | Investigation | ~1h | A3 | open |
| B6 | Legibility lift: light theme impact | Investigation | ~30min | A1 | open |
| B7 | Accent Explorer: what's the deliverable? | Investigation | ~30min | B1 | open |

**Recommended execution order:**
1. **Parallel, no-risk:** A1 (token lift), A3 (phantom Start delete)
2. **Medium effort, clear spec:** A2 (focal metric), A4 (list ergonomics)
3. **Investigation sprint:** B1–B7 in parallel (each ≤ 1h, mostly file reads)
4. **After B-series:** scope and write implementation specs for:
   - Chart craft upgrade (from B2)
   - Optimizer launch drawer (from B3, B4, B5)
   - Accent token decision (from B1, B7)

---

## Agent instructions

When working a small item (A-series):
1. Read the source prototype HTML (open in browser) and/or the handoff README for full context
2. Identify the exact lines to change — do not refactor surrounding code
3. Write tests if the change touches logic (hooks, state); no tests needed for pure CSS token changes
4. Do not tackle adjacent items; each A item is a standalone PR

When working an investigation item (B-series):
1. Write findings as a short "Investigation findings" note appended to this spec (under the item's section)
2. If the finding unblocks an implementation, write a concrete spec addendum here
3. If a user decision is needed, surface the decision clearly — do not default
4. Investigation items are read-only (grep, file reads, no code changes)

Design source of truth: `docs/design/DesignImprovementSweep/` — the standalone HTML prototypes are canonical for visual treatment; the two handoff READMEs are canonical for implementation spec.

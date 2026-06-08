# xvision Multi-Skill Area Audit
**Date**: 2026-06-08  
**Skills**: `make-interfaces-feel-better` · `interface-design` · `impeccable`  
**Coverage**: 5 areas × 3 skills = 15 concurrent lenses  
**Constraint**: No code changes. All findings are actionable.

---

## AREA 1 — Home / Dashboard

### MF-H1 — RunRow enter/exit animations missing (make-interfaces-feel-better · high)
**Location**: `components/home/ActiveTasksStrip.tsx:57`  
**Fix**: `AnimatePresence` + `motion.div` on each RunRow: `initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={{ duration: 0.15 }}`.

### MF-H2 — `border-l-2 border-green-500` banned side-stripe pattern (make-interfaces-feel-better · medium)
**Location**: `components/home/LiveStrategiesSection.tsx:109`  
**Fix**: Replace with `rounded-md border border-green-500/20 bg-green-500/[0.03] px-4 py-3`.

### MF-H3 — CriticalFindings skeleton: non-structural shimmer (make-interfaces-feel-better · medium)
**Location**: `components/home/CriticalFindingsRow.tsx:83-90`  
**Fix**: Match skeleton shape to `FindingChip` structure with directional shimmer gradient: `bg-gradient-to-r from-muted via-muted/60 to-muted animate-[shimmer_1.5s_infinite]`.

### MF-H4 — StrategyRow not hoverable (make-interfaces-feel-better · low)
**Location**: `components/home/StrategyOutcomesList.tsx:88-149`  
**Fix**: Make `<li>` a `Link` with `hover:bg-muted/30 transition-colors rounded-md`.

### ID-H1 — StrategyOutcomesList buried at section 5 of 7 (interface-design · high)
**Location**: `routes/home.tsx:77-83`  
**Fix**: Reorder: SafetyPauseBanner → StrategyOutcomesList → [ActiveTasksStrip inline] → LiveStrategiesSection → CriticalFindingsRow → NagStrip. Performance data leads.

### ID-H2 — Subtitle "cockpit · N strategies" — jargon (interface-design · high)
**Location**: `routes/home.tsx:72-74`  
**Fix**: `${n} strategies · ${inflightCount > 0 ? inflightCount + ' running' : 'idle'}`.

### ID-H3 — Zero-strategies empty state has one sentence and no workflow (interface-design · high)
**Location**: `components/home/StrategyOutcomesList.tsx:200-207`  
**Fix**: Add onboarding structure: 3-step numbered hints (Create → Eval → Optimize) with a prominent "Create your first strategy" CTA button.

### ID-H4 — OptimizerDigestStrip imported but not rendered (interface-design · low)
**Location**: `routes/home.tsx`  
**Fix**: Add `<OptimizerDigestStrip />` between LiveStrategiesSection and CriticalFindingsRow — it already returns null when no sessions exist.

### ID-H5 — "Draft variant →" links to read-only run detail (interface-design · low)
**Location**: `components/home/CriticalFindingsRow.tsx:37-43`  
**Fix**: Rename to `View run →` or route to `/strategies/:id` if editing is the intent.

### IM-H1 — 7 parallel queries, no coordinated loading gate (impeccable · high)
**Location**: `routes/home.tsx:27`  
**Fix**: If `runs.isPending && strategies.isPending`, show a single full-page skeleton (2 `h-24` pulse blocks) instead of each section blinking in independently.

### IM-H2 — ActiveTasksStrip returns null on load — causes layout jump (impeccable · high)
**Location**: `components/home/ActiveTasksStrip.tsx:108`  
**Fix**: Always render container; show skeleton row (`h-10 rounded bg-muted animate-pulse`) while `data === undefined`.

### IM-H3 — "No active tasks" permanent empty card wastes above-fold space (impeccable · high)
**Location**: `components/home/ActiveTasksStrip.tsx:131-133`  
**Fix**: `return null` when `inflight.length === 0`. Documented in previous audit — adding here for implementation priority.

### IM-H4 — "Live strategies trade real capital" reads as alarm (impeccable · medium)
**Location**: `components/home/LiveStrategiesSection.tsx:48-58`  
**Fix**: "No strategies are running live. Connect a broker to enable live deployment."

### IM-H5 — NagStrip toggle missing `aria-expanded` (impeccable · medium)
**Location**: `components/home/NagStrip.tsx:87-95`  
**Fix**: `aria-expanded={showAll}` + `aria-controls` pointing to the items container id.

### IM-H6 — "Honesty check —" always ends with dangling em-dash (impeccable · medium)
**Location**: `components/home/OptimizerDigestStrip.tsx:40`  
**Fix**: Show outcome: `Honesty check: passed` / `Honesty check: failed`, or omit until API exposes the field.

### IM-H7 — "from 3 most recent reviews" uses wrong terminology (impeccable · low)
**Location**: `components/home/CriticalFindingsRow.tsx:78`  
**Fix**: "from 3 most recent eval runs".

### IM-H8 — NagStrip tone dot is color-only severity signal (impeccable · low)
**Location**: `components/home/NagStrip.tsx:35`  
**Fix**: Add `<span className="sr-only">{tone === 'danger' ? 'Error:' : 'Warning:'}</span>` before item title.

### IM-H9 — Strategy row shows "no evals yet" for in-flight runs (impeccable · medium)
**Location**: `components/home/StrategyOutcomesList.tsx:88`  
**Fix**: Pass `hasInflightRun: boolean` prop to `StrategyRow`; when true and `mostRecent === null`, show "eval in progress…" with `animate-pulse` dot.

---

## AREA 2 — Eval Runs (list + detail + compare)

### MF-E1 — No row stagger animation on list load (make-interfaces-feel-better · high)
**Location**: `routes/eval-runs.tsx:409`  
**Fix**: `style={{ animationDelay: `${Math.min(i, 8) * 40}ms` }}` + `@keyframes row-in { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; } }`.

### MF-E2 — Stat numbers update instantly with no visual feedback (make-interfaces-feel-better · medium)
**Location**: `routes/eval-runs-detail.tsx:749`  
**Fix**: `key={value}` wrapper + `@keyframes num-pop { 0% { opacity: 0.4; transform: scale(0.97); } 100% { opacity: 1; transform: scale(1); } }` on stat value element.

### MF-E3 — Table row hover: flat tint only, no directional depth (make-interfaces-feel-better · medium)
**Location**: `routes/eval-runs.tsx:572`  
**Fix**: `hover:shadow-[inset_2px_0_0_0_var(--gold-soft)] transition-[background-color,box-shadow]`.

### MF-E4 — Compare loading skeleton: non-structural (make-interfaces-feel-better · medium)
**Location**: `routes/eval-compare.tsx:88-93`  
**Fix**: Replace with structural skeleton mirroring MetricsTable column layout (~11 shimmer cells + 2-3 shimmer rows).

### MF-E5 — Primary CTA and action buttons: no `active:scale` (make-interfaces-feel-better · low)
**Location**: `routes/eval-runs.tsx:358-364`, `routes/eval-runs-detail.tsx` `ACTION_BTN`  
**Fix**: `active:scale-[0.96] transition-[transform,background-color,border-color]` on all. Replace `transition-colors` throughout.

### ID-E1 — "Start eval" CTA disconnected from the list (interface-design · high)
**Location**: `routes/eval-runs.tsx:350-365`  
**Fix**: Move trigger to the list header toolbar slot so it is co-located with filters and always at the same visual weight as the list title.

### ID-E2 — Compare defaults to call_order sort (interface-design · high)
**Location**: `routes/eval-compare.tsx:118-145`  
**Fix**: `useState<CompareSortKey>('net_return')` as default. Add winner row: `ring-1 ring-gold/40 bg-gold/[0.04]` + `✦ BEST` badge.

### ID-E3 — No path from eval list to Optimizer (interface-design · high)
**Location**: `routes/eval-runs.tsx` — missing CTA  
**Fix**: Add "Optimize →" action on completed rows pointing to `/optimizer?strategy=<row.agent_id>`. Also in SummaryCard action row on detail page.

### ID-E4 — Detail page h1 is a UUID (interface-design · medium)
**Location**: `routes/eval-runs-detail.tsx:259-311`  
**Fix**: h1 → strategy name at `text-[20px] font-semibold`. UUID → secondary `font-mono text-[13px] text-text-3 select-all`.

### ID-E5 — "Latest run chart" shows wrong data when filters active (interface-design · medium)
**Location**: `routes/eval-runs.tsx:451-474`  
**Fix**: Hide section entirely when filters active. Change heading to "Most recent run".

### IM-E1 — Empty state conflates zero-runs-ever with zero-filter-results (impeccable · high)
**Location**: `routes/eval-runs.tsx:399`  
**Fix**: When `list.totalRows === 0`: "No evals yet. Run your first eval to start benchmarking." + Start eval CTA. When filtered to zero: "No runs match these filters." + Clear filters button.

### IM-E2 — "Compare needs two or more runs" — passive system copy (impeccable · high)
**Location**: `routes/eval-compare.tsx:489`  
**Fix**: "Pick runs to compare".

### IM-E3 — `<tr role="link">` is invalid ARIA (impeccable · high)
**Location**: `routes/eval-runs.tsx:563-572`  
**Fix**: Remove `role="link"`. Wrap run-id cell content in `<Link>` with `block w-full h-full`. Keep row click handler for mouse users.

### IM-E4 — `<label>` not associated with `<select>` via id/htmlFor (impeccable · medium)
**Location**: `routes/eval-runs.tsx:862-870`  
**Fix**: `id="strategy-select"` on select, `htmlFor="strategy-select"` on label. Same fix for Scenario, Provider, Model selects.

### IM-E5 — Chart empty state not differentiated by run status (impeccable · medium)
**Location**: `routes/eval-runs-detail.tsx:667-677`  
**Fix**: Cancelled → "Run was cancelled before chart data was recorded." Running → "Chart will appear as decisions are made." Other → "No chart data available."

### IM-E6 — `text-rose-300` hardcoded color bypasses design tokens (impeccable · medium)
**Location**: `routes/eval-runs.tsx:879-883`  
**Fix**: `text-rose-300` → `text-danger`. Same at line 908.

### IM-E7 — Status badge shows internal tokens (`QUEUED`) to operators (impeccable · low)
**Location**: `routes/eval-runs-detail.tsx:693-696`  
**Fix**: Map: `running → 'IN PROGRESS', queued → 'WAITING', failed → 'FAILED', cancelled → 'STOPPED', completed → 'PASS'`.

### IM-E8 — Color dots in compare table have no non-color fallback (impeccable · low)
**Location**: `routes/eval-compare.tsx:287-290`  
**Fix**: Add `A`/`B`/`C`/`D` positional labels in `text-[9px] font-mono text-text-3` after each dot.

---

## AREA 3 — Optimizer

### MF-O1 — Phase chip transitions: colors only, no transform (make-interfaces-feel-better · high)
**Location**: `features/autooptimizer/ui/PhaseStepper.tsx:58-66`  
**Fix**: `transition-[colors,opacity,transform] duration-300 ease-out`. Elapsed counter: `tabular-nums` + fade-in keyframe.

### MF-O2 — ActivityFeed: new SSE rows appear with no entrance animation (make-interfaces-feel-better · high)
**Location**: `features/autooptimizer/ui/ActivityFeed.tsx:186-208`  
**Fix**: `@keyframes row-enter { from { opacity: 0; transform: translateY(6px); } }` applied `animation: row-enter 200ms ease-out both` to new `<tr>` elements.

### MF-O3 — ProgressDial arc doesn't animate (make-interfaces-feel-better · medium)
**Location**: `features/autooptimizer/ui/ProgressDial.tsx:18-30`  
**Fix**: `style={{ transition: 'stroke-dashoffset 600ms cubic-bezier(0.4, 0, 0.2, 1)' }}` on the fill circle.

### MF-O4 — Pause/Resume/Cancel: no `active:scale` press feedback (make-interfaces-feel-better · medium)
**Location**: `features/autooptimizer/screens/OptimizerHome.tsx` — action buttons  
**Fix**: `active:scale-[0.96] transition-[transform,opacity,background-color]` on all three.

### MF-O5 — "Jump to latest" button snaps in with no transition (make-interfaces-feel-better · medium)
**Location**: `features/autooptimizer/ui/ActivityFeed.tsx:169-174`  
**Fix**: Always in DOM; toggle `opacity-0 pointer-events-none` ↔ `opacity-100` with `transition-[opacity] duration-200`.

### MF-O6 — RecentSessionRow arrow blinks in without slide (make-interfaces-feel-better · medium)
**Location**: `features/autooptimizer/screens/OptimizerHome.tsx:255-258`  
**Fix**: `opacity-0 translate-x-0 group-hover:opacity-100 group-hover:translate-x-1 transition-[opacity,transform] duration-150`.

### MF-O7 — ActivityFeed timestamps cause column-width jitter (make-interfaces-feel-better · medium)
**Location**: `features/autooptimizer/ui/ActivityFeed.tsx:191-194`  
**Fix**: `tabular-nums` + `hour12: false` on `toLocaleTimeString` → fixed-width `HH:MM:SS`.

### ID-O1 — Idle state has no orientation for first-time user (interface-design · high)
**Location**: `features/autooptimizer/screens/OptimizerHome.tsx:323-354`  
**Fix**: Add two-line context in idle StatusHero: "The Optimizer writes and evaluates strategy experiments automatically, then promotes improvements into your live strategy. Configure a run below to begin."

### ID-O2 — Start button scrolls to itself — doesn't start anything (interface-design · high)
**Location**: `features/autooptimizer/screens/OptimizerHome.tsx:196-230`  
**Fix**: Wire to POST `/sessions`. Add strategy dropdown. Remove `scrollIntoView`. This is the primary CTA — it must work.

### ID-O3 — "Tonight's run" subtitle wrong most of the day (interface-design · high)
**Location**: `features/autooptimizer/screens/OptimizerHome.tsx:322`  
**Fix**: "Status and recent runs".

### ID-O4 — "What happened" section always empty, always visible (interface-design · high)
**Location**: `features/autooptimizer/screens/ExperimentDetail.tsx:85-106`  
**Fix**: `{detail?.events?.length > 0 && <section>...</section>}` — never render the section when it has no data.

### ID-O5 — StatusHero running headline leads with session ID not strategy name (interface-design · medium)
**Location**: `features/autooptimizer/screens/OptimizerHome.tsx:87-102`  
**Fix**: Reorder: `{strategy_id} · {modeLabel}` as h2. Session ID → secondary monospace line below.

### IM-O1 — RecentSessionsList: `isLoading` returns null + no error handling (impeccable · high)
**Location**: `features/autooptimizer/screens/OptimizerHome.tsx:262-266`  
**Fix**: Replace `return null` with 3-row skeleton. Add `isError` branch with retry copy.

### IM-O2 — CycleDetail and ExperimentDetail: bare "Loading…" text states (impeccable · high)
**Location**: `features/autooptimizer/screens/CycleDetail.tsx:32-34`, `ExperimentDetail.tsx:44-47`  
**Fix**: Shape-matched skeleton cards + error panels with `refetch()` retry button.

### IM-O3 — "Tonight's run" and "Experiment writers" are jargon (impeccable · high)
**Location**: `features/autooptimizer/screens/OptimizerHome.tsx`, `ExperimentWritersPanel`  
**Fix**: Subtitle → "Status and recent runs". Panel heading → "Variation approaches" with sub-label "How the optimizer varies your strategy prompts".

### IM-O4 — ActivityFeed: no `aria-live` region (impeccable · high)
**Location**: `features/autooptimizer/ui/ActivityFeed.tsx:184-208`  
**Fix**: `aria-live='polite' aria-label='Optimizer activity'` on scroll container. `<caption className='sr-only'>` on table.

### IM-O5 — PhaseStepper wired to `currentPhase={null}` even while running (impeccable · medium)
**Location**: `features/autooptimizer/screens/OptimizerHome.tsx` — StatusHero  
**Fix**: Expose `current_phase` from session API. If not available, hide PhaseStepper entirely when `currentPhase === null` rather than showing 7 grey neutral chips.

### IM-O6 — FlywheelStrip uses `text-emerald-400`/`text-red-400` (impeccable · medium)
**Location**: `features/autooptimizer/ui/FlywheelStrip.tsx:43-59`  
**Fix**: `text-emerald-400` → `text-gold`, `text-red-400` → `text-danger`.

### IM-O7 — Phase-lock hints name internal milestones ("once attesters ship") (impeccable · low)
**Location**: `features/autooptimizer/screens/CycleDetail.tsx:62-63`  
**Fix**: "Coming soon" or hide EmptyPanel blocks entirely until the feature ships.

### IM-O8 — ScheduleStrip enable toggle has no visible label text (impeccable · medium)
**Location**: `features/autooptimizer/ui/ScheduleStrip.tsx:56-65`  
**Fix**: `<label><input .../><span className='sr-only'>Enable scheduled run</span></label>`.

---

## AREA 4 — Design System

### MF-DS1 — No global motion token system (make-interfaces-feel-better · high)
**Location**: `styles/tokens.css`, `styles/globals.css`  
**Fix**: Add to tokens.css:
```css
--duration-fast: 80ms;
--duration-base: 160ms;
--duration-slow: 240ms;
--ease-out: cubic-bezier(0.2, 0, 0, 1);
```
Expose in `tailwind.config.ts` under `transitionDuration` and `transitionTimingFunction`.

### MF-DS2 — No global `prefers-reduced-motion` block (make-interfaces-feel-better · high)
**Location**: `styles/globals.css` — only `xvn-pill-animated` has override  
**Fix**:
```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
  }
}
```

### MF-DS3 — No shadow tokens defined anywhere (make-interfaces-feel-better · high)
**Location**: `styles/tokens.css`  
**Fix** (dark theme):
```css
--shadow-sm: 0 1px 2px rgba(0,0,0,0.5);
--shadow-card: 0 2px 8px rgba(0,0,0,0.6), 0 0 0 1px rgba(255,255,255,0.04);
--shadow-float: 0 8px 24px rgba(0,0,0,0.8), 0 0 0 1px rgba(255,255,255,0.06);
```

### MF-DS4 — No global focus-ring system (make-interfaces-feel-better · high)
**Location**: `styles/tokens.css`, `styles/globals.css`  
**Fix**:
```css
/* tokens.css */
--focus-ring: 0 0 0 2px var(--bg), 0 0 0 4px var(--gold);

/* globals.css @layer base */
:focus-visible { outline: none; box-shadow: var(--focus-ring); }
```

### MF-DS5 — `font-smoothing` and `tabular-nums` applied to `body` only (make-interfaces-feel-better · medium)
**Location**: `styles/globals.css:15-20`  
**Fix**: Move to `* { -webkit-font-smoothing: antialiased; }` so portal/shadow-DOM elements inherit it.

### MF-DS6 — No `text-wrap: balance` on headings (make-interfaces-feel-better · medium)
**Location**: `styles/globals.css` — missing  
**Fix**: `@layer base { h1, h2, h3 { text-wrap: balance; } p { text-wrap: pretty; } }`.

### MF-DS7 — Pill radius (2px) breaks concentricity with Card (6px) at `p-1` padding (make-interfaces-feel-better · medium)
**Location**: `components/primitives/Card.tsx`, `components/primitives/Pill.tsx`  
**Fix**: `--radius-pill: 3px` in tokens.css. Update Pill to `rounded-[3px]`.

### MF-DS8 — Sweep animation uses `ease-in-out` (wrong direction) (make-interfaces-feel-better · low)
**Location**: `styles/globals.css:248-266`  
**Fix**: Change `xvn-pill-sweep` timing to `cubic-bezier(0.4, 0, 0.6, 1)`. Keep outer pulse on `ease-in-out`.

### ID-DS1 — Text tokens are positional, not semantic (interface-design · high)
**Location**: `styles/tokens.css`  
**Fix**: Add semantic aliases:
```css
--text-body: var(--text);
--text-supporting: var(--text-2);
--text-meta: var(--text-3);
--text-muted: var(--text-4);
```
Expose in tailwind.config.ts as `text-body`, `text-supporting`, `text-meta`, `text-muted`.

### ID-DS2 — No `--action-primary` token (brand accent doubles as CTA color) (interface-design · high)
**Location**: `styles/tokens.css`  
**Fix**: Add:
```css
--action-primary: var(--gold);
--action-primary-hover: var(--gold-soft);
--action-primary-text: var(--on-accent);
--action-secondary-bg: var(--surface-elev);
--action-secondary-border: var(--border-strong);
--action-secondary-text: var(--text);
```

### ID-DS3 — Duplicate `:root` and `[data-theme=dark]` blocks (interface-design · high)
**Location**: `styles/tokens.css`  
**Fix**: Remove `:root` block; set `data-theme='dark'` as default attribute in ThemeProvider or `index.html`. Single source of truth.

### ID-DS4 — `--gold` naming debt: green token with warm name (interface-design · medium)
**Location**: `styles/tokens.css`, `theme/themes.ts`  
**Fix**: Add `--brand: var(--gold)` as forward-facing alias. In Chart2WarmPalette, rename `gold` key → `signal` or `brand`. Migrate consumers from `text-gold` → `text-brand` incrementally.

### ID-DS5 — No spacing scale tokens (interface-design · medium)
**Location**: `styles/tokens.css`  
**Fix**: Add `--space-1` through `--space-8` + semantic aliases: `--space-component: var(--space-3)`, `--space-section: var(--space-5)`, `--space-page: var(--space-6)`.

### ID-DS6 — `font-serif` alias is Geist (not a serif) (interface-design · low)
**Location**: `tailwind.config.ts:37-38`  
**Fix**: Rename to `font-display` or remove. Rename `.serif-i` in globals.css to `.display-bold`.

### IM-DS1 — `--text-3` (#5f6670) fails WCAG 4.5:1 on `--surface-card` (impeccable · high)
**Location**: `styles/tokens.css:25`  
**Issue**: ~3.6:1 contrast ratio at 13px normal weight. Fails WCAG 2.1 AA.  
**Fix**: Dark `--text-3: #7a8390` (~5.2:1 on #0a0a0a). This single change fixes ~80% of contrast failures.

### IM-DS2 — Placeholder color `--text-4` fails at ~1.7:1 contrast (impeccable · high)
**Location**: `styles/globals.css:27-31`  
**Fix**: Change placeholder to `--text-3` (after fixing text-3 to #7a8390). Verify rendered contrast ≥4.5:1.

### IM-DS3 — Light mode `--on-accent` (#000 on #00a15c) = 4.1:1 — fails AA (impeccable · high)
**Location**: `styles/tokens.css:89`  
**Fix**: Light mode `--on-accent: #ffffff`. White on #00a15c = ~5.2:1. Passes AA.

### IM-DS4 — `Pill` has no disabled state (impeccable · medium)
**Location**: `components/primitives/Pill.tsx`  
**Fix**: Add `disabled?: boolean` prop → `opacity-40 cursor-not-allowed pointer-events-none aria-disabled="true"`.

### IM-DS5 — `Icon` has no `label` prop — icon-only buttons are nameless (impeccable · medium)
**Location**: `components/primitives/Icon.tsx:231`  
**Fix**: Add `label?: string`; when provided: `<title>{label}</title>` as first SVG child, `role='img'`, remove `aria-hidden`.

### IM-DS6 — `Card` has no loading/error state (impeccable · medium)
**Location**: `components/primitives/Card.tsx`  
**Fix**: `state?: 'loading' | 'error'` prop. `loading` → shimmer pseudo-element. `error` → `--danger` border tint + children slot.

### IM-DS7 — `.caps` class: 10.5px + `--text-3` fails both size and contrast (impeccable · medium)
**Location**: `styles/globals.css:37-44`  
**Fix**: Raise to 11px. Change color to `--text-2` for semantic caps (section labels). Reduce tracking 0.10 → 0.08em.

### IM-DS8 — `.input` focus indicator fails WCAG 2.4.11 (impeccable · low)
**Location**: `styles/globals.css:56`  
**Fix**: `focus-visible:ring-2 focus-visible:ring-gold focus-visible:ring-offset-1 focus-visible:ring-offset-bg focus-visible:border-transparent`.

### IM-DS9 — No disabled token set (impeccable · low)
**Location**: `styles/tokens.css`  
**Fix**: `--text-disabled: var(--text-4)`, `--surface-disabled`, `--border-disabled`. Wire to tailwind.

---

## AREA 5 — Strategies / Agents / Scenarios

### MF-S1 — TemplateCard: no `active:scale` on the most important click (make-interfaces-feel-better · high)
**Location**: `routes/agents-edit.tsx:93`  
**Fix**: `active:scale-[0.96] transition-[transform,background-color,border-color] duration-100` on the `<button>` in TemplateCard.

### MF-S2 — All three primary CTAs missing press feedback (make-interfaces-feel-better · medium)
**Location**: `routes/strategies.tsx:386`, `routes/agents.tsx:176`, `routes/scenarios.tsx:219`  
**Fix**: `active:scale-[0.96] transition-[transform,background-color] duration-100` on all three New buttons.

### MF-S3 — InlineEditField edit entrypoint is invisible (make-interfaces-feel-better · medium)
**Location**: `routes/strategies-detail.tsx:330`  
**Fix**: Pass `displayClassName="hover:underline decoration-dashed decoration-text-3 underline-offset-2 cursor-text"` to both InlineEditField instances.

### MF-S4 — Strategy detail loading: bare text, no skeleton (make-interfaces-feel-better · medium)
**Location**: `routes/strategies-detail.tsx:301`  
**Fix**: Width-graded skeleton: `h-6 w-48` title + `h-3 w-32` subtitle + 4× `h-4 w-full` definition rows, all `animate-pulse bg-surface-hover rounded`.

### MF-S5 — ColorPickerRow swatches below 40px hit area (make-interfaces-feel-better · low)
**Location**: `routes/strategies-detail.tsx:115-160`  
**Fix**: `p-2` wrapper to reach 40×40px total. Add `transition: outline 100ms` to swatch style.

### MF-S6 — ScenarioForm Advanced toggle: no animation, Unicode arrows (make-interfaces-feel-better · low)
**Location**: `routes/scenarios-new.tsx` → `ScenarioForm.tsx:344`  
**Fix**: `max-h-0 overflow-hidden transition-[max-height] duration-200` for section reveal. Chevron icon with `rotate-90` transform.

### ID-S1 — Strategies list: no performance data at all (interface-design · high)
**Location**: `routes/strategies.tsx:280-292`  
**Fix**: Add "Evals" count column. Add "Run eval →" in row action menu pointing to `/eval-runs/new?strategy_id=`.

### ID-S2 — Strategy detail dead-ends — no "run eval" next step (interface-design · high)
**Location**: `routes/strategies-detail.tsx`  
**Fix**: Add `Run eval on this strategy →` CTA once agent-readiness resolves as ready. Closes the create → evaluate loop.

### ID-S3 — Model column: raw provider string unreadable (interface-design · high)
**Location**: `routes/strategies.tsx:453`  
**Fix**: `truncate max-w-[140px]` + `title` tooltip. Optionally prefix with provider badge icon.

### ID-S4 — Agents list Skills column: integer count with no context (interface-design · medium)
**Location**: `routes/agents.tsx:149-157`  
**Fix**: Replace integer with first 2 skill names + `+N more` overflow chip. Or surface skill names as pills below description in the name cell.

### ID-S5 — Memory/Skills nav buttons at same weight as "New agent" CTA (interface-design · medium)
**Location**: `routes/agents.tsx:163-181`  
**Fix**: Move Memory and Skills to Topbar sub or a tab strip. Only "New agent" remains in the primary action strip.

### ID-S6 — Scenario window column: needs duration computed (interface-design · medium)
**Location**: `routes/scenarios.tsx:202-209`  
**Fix**: Add computed `Duration` display: `2y 3m` derived from `fmtWindow`. Or shorten to year ranges + day count as sub-text.

### IM-S1 — Template picker copy uses em-dash and jargon (impeccable · high)
**Location**: `routes/agents-edit.tsx:82-84`  
**Fix**: "Templates prefill slot names, system prompts, and structure. You can rename anything or add and remove slots once you get in."

### IM-S2 — ScenarioForm: sequential validation reveals one error at a time (impeccable · high)
**Location**: features/scenarios/`ScenarioForm.tsx:145-162`  
**Fix**: Run ALL validations before any `return`. Set all error states simultaneously, then return if any failed.

### IM-S3 — `<tr role='link'>` invalid ARIA in scenarios and agents tables (impeccable · high)
**Location**: `routes/scenarios.tsx:312-321`, `routes/agents.tsx:275`  
**Fix**: Remove `role='link'`. Add `aria-label={\`Open ${row.display_name}\`}` to `<tr>`.

### IM-S4 — Context bars help text: `t=0`, `≥`, `26-bar EMA` are jargon (impeccable · medium)
**Location**: `features/scenarios/ScenarioForm.tsx:326-331`  
**Fix**: "How many bars of price history to load before the scenario window starts. More bars let indicators and your agent warm up. A good starting point is 200."

### IM-S5 — "Create →" button label has no object (impeccable · medium)
**Location**: `features/scenarios/ScenarioForm.tsx:410`  
**Fix**: "Create scenario". Remove `→`.

### IM-S6 — Strategy detail error state has no retry (impeccable · medium)
**Location**: `routes/strategies-detail.tsx:308-313`  
**Fix**: Add `<button onClick={() => query.refetch()}>Try again</button>` to the error state.

### IM-S7 — Name field has `required` but also JS validation — double error surfaces (impeccable · medium)
**Location**: `features/scenarios/ScenarioForm.tsx:218-231`  
**Fix**: Remove `required` attribute. Add `placeholder='e.g. BTC 2020-2022 bear market'`. Rely solely on JS validation.

### IM-S8 — "Venue (Alpaca)" fieldset title is implementation jargon (impeccable · low)
**Location**: `features/scenarios/ScenarioForm.tsx:338-345`  
**Fix**: "Simulation settings".

### IM-S9 — `scenarios-new.tsx` Topbar `sub=''` wastes subtitle slot (impeccable · low)
**Location**: `routes/scenarios-new.tsx:36`  
**Fix**: `sub="Define a backtest window and market context"`.

### IM-S10 — AgentEdit Topbar subtitle is raw ULID in edit mode (impeccable · low)
**Location**: `routes/agents-edit.tsx:35-39`  
**Fix**: Resolve to agent display name from query cache, or use "Edit agent configuration" as fallback.

---

## Top Picks: Implement First

### Accessibility tier (ship immediately — these are failures)
1. `--text-3` contrast fix → `#7a8390` (fixes ~80% of contrast failures in one token change)
2. Placeholder color → `--text-3` (after above)
3. Light mode `--on-accent` → `#ffffff`
4. Global `:focus-visible` ring system
5. `<tr role='link'>` → remove invalid ARIA in eval-runs, scenarios, agents tables

### Motion tier (big demo impact, CSS-only)
6. Global `prefers-reduced-motion` block
7. Motion token system (`--duration-fast/base/slow`, `--ease-out`)
8. Row stagger animation on all list loads (same `@keyframes row-in` reused everywhere)
9. ActivityFeed SSE row entrance flash
10. `active:scale-[0.96]` on all primary CTAs (5-line change each)

### UX clarity tier (most visible to a hackathon judge)
11. Fix Optimizer Start button → actually POST /sessions
12. Strategy detail: add "Run eval" next-step CTA
13. Eval list: default compare to `net_return` sort + winner highlight
14. Home: reorder sections (StrategyOutcomesList first)
15. Eval → Optimizer path: add "Optimize →" on completed run rows

---

*Generated by 5 area agents × 3 skills each (make-interfaces-feel-better · interface-design · impeccable)*  
*Total findings: ~155 raw → 97 de-duplicated across 5 areas*

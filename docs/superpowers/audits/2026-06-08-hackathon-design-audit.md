# xvision Hackathon Design Audit
**Date**: 2026-06-08  
**Goal**: Make xvision win a hackathon — exceptional design + clear profitable-strategy UX  
**Method**: 4 parallel skill audits (make-interfaces-feel-better, interface-design, bencium-innovative-ux-designer, impeccable) over the full frontend codebase  
**Constraint**: Research only — no code changes. All findings are actionable for a coding agent.

---

## TIER 0 — Hackathon-Critical (fix before any demo)

These are the issues a judge will notice in the first 60 seconds.

### H1 — Optimizer Start button is a no-op
**File**: `features/autooptimizer/screens/OptimizerHome.tsx:193`  
**Issue**: The "Start" button in ConfigureSection calls `getElementById + scrollIntoView` — it scrolls to the button you just clicked. It does NOT call POST /sessions. The optimizer does not start.  
**Fix**: Wire `handleStart` to POST /sessions via `useMutation`. The `runMode/runCount/runBudget` state is already set by ModePicker. Add a strategy dropdown (use the existing strategies query already fetched on this page). Remove the `scrollIntoView` entirely.

### H2 — Dashboard has no narrative for a first-time viewer
**File**: `routes/home.tsx:74,77`  
**Issue**: Subtitle reads "cockpit · 3 strategies" — jargon. Page opens to blank sections. No visible workflow path (Create strategy → Run eval → Optimize). A judge sees five floating cards and no story.  
**Fix**: 
- Replace subtitle: `{n} strategies · {m} eval runs`
- When strategies.length === 0, show a 3-step inline checklist card above the fold: "1. Create a strategy → 2. Create a scenario → 3. Run a backtest" with linked CTAs
- Remove `ActiveTasksStrip` when inflight count is 0 (return null) — it currently shows "No active tasks" as a permanent above-fold element

### H3 — Optimizer home has 9 equal-weight sections, no hierarchy
**File**: `features/autooptimizer/screens/OptimizerHome.tsx`  
**Issue**: StatusHero → ConfigureSection → ScheduleStrip → ImprovementChartsSection → FlywheelStrip → LiveCycleView → ExperimentWritersPanel → RecentCyclesTable → RecentSessionsList — all identical visual treatment, no primary action clear.  
**Fix**:
- **Idle state**: Show only StatusHero + ConfigureSection (with `bg-accent/[0.06] border border-accent/30` to distinguish as CTA) + ImprovementChartsSection (collapsed to sparkline) + RecentSessionsList
- **Running state**: StatusHero full-width + LiveCycleView prominently
- Collapse ExperimentWritersPanel, ScheduleStrip, FlywheelStrip into an "Advanced" accordion

### H4 — Profitable strategies are visually invisible
**File**: `components/home/StrategyOutcomesList.tsx:26-38, 60-65`  
**Issue**: Win state uses `green-500/5` background (5% opacity = invisible). Requires 10 evals to activate. Return % is 13px mono body text. A +34% return looks identical to a -5% return.  
**Fix**:
- Lower win activation threshold: 3 completed evals (not 10)
- Winning strategies: `border-l-2 border-gold bg-gold/[0.06]`, return value at `text-[20px] font-bold text-gold`
- Losing strategies: `border-l-2 border-danger/40`
- Eval runs list: color the Return column: positive → `text-gold font-semibold`, negative → `text-danger`
- Eval runs detail: Create a `MetricHero` component — Return/Sharpe/MaxDD at `text-[48px] font-bold tabular-nums` as the page hero

### H5 — Raw IDs exposed throughout as primary labels
**Files**: `features/autooptimizer/screens/OptimizerHome.tsx`, `routes/eval-runs.tsx:231`, `features/autooptimizer/screens/CycleDetail.tsx`  
**Issue**: Strategy ULIDs appear as primary text in optimizer sessions list, eval runs table strategy column, cycle detail breadcrumbs. App looks like a debug console.  
**Fix**:
- SessionListItem: resolve `strategy_id` → `display_name` from strategies query (already fetched on OptimizerHome)
- Eval runs table: `displayStrategyName()` already exists — ensure it falls back to `'Unnamed strategy'` in italic, never a raw ULID
- Cycle breadcrumb: truncate ID to 8 chars, prefix: "Cycle 01HX9B4M"
- ScheduleStrip: resolve `strategy_id` → `display_name`
- Enforce `display_name` required at strategy creation time

---

## TIER 1 — High Priority (fix for a polished demo)

### A1 — No enter/exit animations anywhere
**Files**: All list routes: `strategies.tsx`, `eval-runs.tsx`, `agents.tsx`, optimizer screens  
**Issue**: Lists snap in simultaneously on load. Panels appear/disappear instantly. Charts toggle abruptly.  
**Fix**:
```css
/* Add to globals.css */
@keyframes row-enter { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: none; } }
@keyframes fade-in { from { opacity: 0; transform: translateY(6px); } to { opacity: 1; transform: none; } }
```
- List rows: `style={{ animationDelay: `${Math.min(i, 8) * 40}ms` }} className="animate-[row-enter_160ms_ease-out_both]"`
- Panels (StartEvalPanel, PhaseStepper show/hide): `className="animate-[fade-in_200ms_ease-out_both]"`
- OutcomeStackedChart toggle: height-animate via `transition-[max-height,opacity] duration-300`
- StartEvalPanel open/close: `max-h-[800px] opacity-100` vs `max-h-0 opacity-0` with `transition-[max-height,opacity] duration-200`

### A2 — Cards have no depth (shadows invisible in dark mode)
**File**: `components/primitives/Card.tsx`, `features/autooptimizer/screens/OptimizerHome.tsx`  
**Issue**: `shadow-sm` = `0 1px 2px 0 rgba(0,0,0,0.05)` — completely invisible on a #000000 background. All cards sit flat.  
**Fix**:
- Base Card: `shadow-[0_1px_3px_rgba(0,0,0,0.6),0_0_0_1px_rgba(255,255,255,0.04)]`
- StatusHero (elevated primary card): `shadow-[0_2px_8px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,255,255,0.04)]`
- StartEvalPanel: replace `shadow-sm` with `shadow-[0_4px_20px_rgba(0,0,0,0.6),0_0_0_1px_rgba(255,255,255,0.05)]`
- Add card elevation variants: `variant='elevated'` with `border-border-strong` + green ambient glow `box-shadow: 0 0 0 1px #1a1a1a, 0 4px 24px rgba(0,230,118,0.04)`

### A3 — No scale-on-press feedback on buttons
**Files**: `routes/strategies.tsx` (NewStrategyButton), `routes/eval-runs.tsx` (Start eval), all primary CTAs  
**Issue**: Clicking buttons feels weightless — no tactile `active:scale` feedback.  
**Fix**: Add to all primary buttons: `active:scale-[0.96] transition-[transform,background-color] duration-75`

### A4 — Navigation has no hierarchy (10 equal-weight items)
**File**: `components/shell/Sidebar.tsx`  
**Issue**: Core workflow (Strategies, Eval, Optimizer) shares equal weight with utility items (Docs, Settings, Charts).  
**Fix**: Group with thin `<hr className="border-border/50 my-1.5">` dividers and optional muted group labels:
- Group 1 (unlabeled): Dashboard
- Group 2 (optional label "Workflow"): Strategies, Agents, Scenarios, Eval Runs, Optimizer  
- Group 3 (optional label "Explore"): Charts, Marketplace
- Group 4 (bottom): Docs, Settings

Also: rename sidebar "Eval" → "Eval Runs" to match the route title and user mental model.

### A5 — Missing skeleton screens (CLS on load)
**Files**: `features/autooptimizer/screens/OptimizerHome.tsx` (RecentSessionsList returns `null` on load), `components/lists/ListCard.tsx` (single-bar skeleton)  
**Fix**:
- RecentSessionsList: replace `if (isLoading) return null` with 3-row skeleton: `<div className="h-11 rounded-md bg-surface-card border border-border animate-pulse" />`
- ListSkeleton: replace single-bar with column-proportional skeleton (`flex gap-3` with width-approximate divs matching the table columns)
- Route-level fallback: replace `<div>Loading…</div>` with a topbar skeleton + 3-4 content-width `animate-pulse` rectangles

### A6 — Preflight errors broken/inconsistent
**File**: `routes/eval-runs.tsx:1153-1178`  
**Issue**: Mixed arrow glyphs (`->` vs `→`), truncated sentences, form stays interactive when provider not configured (user can fill the form and only fail on Submit).  
**Fix**:
- Standardize all path references to `→` (unicode)
- When `evalPreflightError` returns a setup error: `opacity-50 pointer-events-none` the entire form body, render error prominently at top with the `preflightSetupAction` CTA as a visible button
- Complete truncated error strings

### A7 — Sidebar active state is too faint to perceive
**File**: `components/shell/Sidebar.tsx`  
**Issue**: Active state is `border-l-2 border-gold bg-gold/[0.06]` — 6% opacity on black is invisible. Inactive hover has no background.  
**Fix**:
- Active: `bg-gold/[0.10] border-l-[3px] border-gold text-text` (increase to 10% + 3px border + full label brightness)
- Inactive hover: add `hover:bg-white/[0.03]` (the `surface-hover` token already exists for this)
- Transition: `transition-[background-color,color] duration-100`

### A8 — Financial data lacks visual weight
**Files**: `routes/eval-runs.tsx`, `features/autooptimizer/screens/OptimizerHome.tsx`  
**Issue**: P&L percentages and strategy metrics are body-text sized. Key result numbers should be visually dominant.  
**Fix**:
- Eval runs list Return column: `text-gold font-semibold` for positive, `text-danger font-medium` for negative; add `+` prefix on positive values
- Optimizer StatusHero stat line: `kept_count` → `text-gold font-semibold`; `suspect_count` → `text-warn`; `dropped_count` → `text-text-3`
- All numeric financial cells: add `tabular-nums` class explicitly (belt-and-suspenders, global is set but `font-mono` overrides aren't consistent)
- Consider `↑`/`↓` micro-icons before returns >5%

---

## TIER 2 — Medium Priority (polish and coherence)

### B1 — shadcn token leakage (possibly invisible text)
**Files**: `components/home/` subtree (~61 occurrences)  
**Issue**: `text-muted-foreground`, `text-foreground`, `text-primary`, `bg-muted`, `border-muted-foreground` resolve to undefined in this project (no shadcn globals.css). These components may render with wrong colors or be invisible.  
**Fix**: Replace throughout home components:
- `text-muted-foreground` → `text-2`
- `text-foreground` → `text`  
- `text-primary` → `text-gold` (or `text-accent` depending on context)
- `bg-muted` → `bg-surface-elev`
- `border-muted-foreground` → `border-strong`
Enforce with lint rule: `no-restricted-syntax` on `text-muted-foreground`.

### B2 — 68 `focus:outline-none` without focus-visible rings
**Files**: App-wide  
**Issue**: Keyboard navigation is invisible — WCAG 2.4.7 violation. Select elements in eval-runs form, buttons throughout optimizer.  
**Fix**:
```css
/* globals.css */
:focus-visible { outline: 2px solid var(--gold); outline-offset: 2px; }
```
Then replace `focus:outline-none` → `focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold/60 focus-visible:ring-offset-1 focus-visible:ring-offset-bg` on all interactive elements.

### B3 — Atmospheric depth: pure flat black feels unfinished
**File**: `frontend/web/src/styles/globals.css` or `theme/themes.ts`  
**Issue**: Every surface is featureless matte black. Looks like CSS default dark mode, not a designed terminal.  
**Fix**: Add hairline ambient gradients to content area (inside main, not layout):
```css
/* Sidebar: faint left glow */
.sidebar-surface { background-image: radial-gradient(ellipse at 50% 0%, rgba(0,230,118,0.025) 0%, transparent 60%); }
/* Main content: faint bottom-right glow */  
.main-content { background-image: radial-gradient(ellipse at 80% 100%, rgba(0,230,118,0.015) 0%, transparent 50%); }
```
These are so subtle they read as depth, not pattern. Optional: 2.5% opacity noise texture overlay on `#root`.

### B4 — Typography: no display font, no hero moment
**Files**: `styles/globals.css`, `components/shell/Topbar.tsx`  
**Issue**: Geist everywhere — no typographic personality. Page titles at `font-medium text-[30px]` are generic admin panel headings.  
**Fix**:
- Page titles → `font-bold text-[38px] xl:text-[48px] tracking-[-0.04em]`
- Key result metrics (Return %, Sharpe) in detail pages → `text-[48-56px] font-bold tabular-nums tracking-[-0.03em]`
- These changes alone, at the top of every page, signal "tool built intentionally" rather than "CRA dark mode"

### B5 — Token bypass: hardcoded Tailwind colors outside token system
**Files**: `features/autooptimizer/ui/FlywheelStrip.tsx`, `components/home/LiveStrategiesSection.tsx:108`  
**Issue**: `text-emerald-400` and `text-red-400` in FlywheelStrip conflict with brand green (#00E676). `border-green-500` in LiveStrategiesSection bypasses tokens.  
**Fix**:
- `text-emerald-400` → `text-gold`
- `text-red-400` → `text-danger`
- `border-green-500` → `border-gold/30`
Audit: `grep -r "text-emerald\|text-green\|text-red-400\|border-green" frontend/web/src/ --include="*.tsx"`

### B6 — ProgressDial too small for its significance
**File**: `features/autooptimizer/ui/ProgressDial.tsx`  
**Issue**: 64px default, 6px stroke, percentage text at 13px. The optimizer progress indicator is the same size as body text.  
**Fix**: Default to 88px, stroke 7px, percentage text `text-[18px] font-bold text-gold`. Add glow on arc: `filter: drop-shadow(0 0 6px rgba(0,230,118,0.5))` via SVG filter on the progress circle.

### B7 — `transition-all` in LiveCycleView
**File**: `features/autooptimizer/LiveCycleView.tsx:1074`  
**Issue**: `transition-all` on connection status dot — animates every property including layout dimensions. Expensive and imprecise.  
**Fix**: `transition-[background-color,opacity] duration-200`

### B8 — Strategy color palette has 3 perceptually-similar blues
**File**: `theme/themes.ts — CHART2_STRATEGY_ROTATION`  
**Issue**: Sky (#38BDF8), cyan (#22D3EE), teal (#5EEAD4) are indistinguishable on chart lines at small scale and for color-blind users.  
**Fix**: Replace `#22D3EE` (cyan) with `#FF6B35` (signal orange-red) and `#5EEAD4` (teal) with `#C084FC` (lighter violet). Result: sky, deep-violet, light-violet, amber, orange-red, pink, gold-yellow, bench-gray — 7 perceptually separated hues.

### B9 — Pill component under-sized and border invisible in default tone
**File**: `components/primitives/Pill.tsx`  
**Issue**: `rounded-sm` (2px) on 11px text looks square. `default` tone border (`border-border-soft`) is a hairline on black — invisible.  
**Fix**: `rounded` (4px), `px-2.5 py-1 text-[11.5px]`. Default tone: `border-border` (one step stronger). `solid` tone: add `shadow-[0_0_8px_rgba(0,230,118,0.25)]` glow.

### B10 — Missing `--violet` token causes HashSigil color failure
**File**: `features/autooptimizer/ui/HashSigil.tsx`, `styles/tokens.css`  
**Issue**: `ACCENTS` array uses `var(--violet)` which is not defined in tokens.css — silently falls back to nothing.  
**Fix**: Add `--violet: #a78bfa` to tokens.css (matches `CHART2_SIGNAL_DARK.plum`). Upgrade HashSigil container to `border-[var(--fg-color)]/20 bg-[var(--fg-color)]/5` — gives each sigil a colored border matching its fill.

### B11 — Excessive uppercase labels (149 instances)
**Files**: App-wide  
**Issue**: 149 `uppercase tracking-wide/widest` instances — when everything shouts in caps, nothing stands out.  
**Fix**: Keep only 2-3 section markers per page in uppercase. For stat/field labels: reduce tracking to `tracking-wider` (0.08em) from `tracking-widest`. Replace section eyebrow `uppercase` with `font-semibold text-[13px]` in text color. Audit: `grep -c "uppercase" frontend/web/src/**/*.tsx`.

### B12 — `animate-pulse` has no `prefers-reduced-motion` guard
**Files**: 9+ components using `animate-pulse`  
**Fix**:
```css
/* globals.css */
@media (prefers-reduced-motion: reduce) { .animate-pulse { animation: none; } }
```

### B13 — Absolute text floor violated (8.5px rendered text)
**Files**: `features/autooptimizer/screens/CycleDetail.tsx` (stat labels at 8.5px), `OptimizerHome.tsx`  
**Issue**: 8.5px is sub-pixel on many displays. Illegible.  
**Fix**: Set absolute floor: `text-[10px]` minimum for any rendered text. Audit: `grep -r "text-\[8" frontend/web/src/ --include="*.tsx"`.

### B14 — eval-runs timer re-renders every 1s unnecessarily
**File**: `routes/eval-runs.tsx` (setInterval 1000ms)  
**Issue**: 1-second interval calls `setNowMs(Date.now())` which re-renders the entire 50-row list every second.  
**Fix**: Change to 10000ms (10 seconds). The `fmtDuration` display changes at minute granularity — 1s precision is unused.

### B15 — PhaseStepper uses ✓ plain text, no animation on completion
**File**: `features/autooptimizer/ui/PhaseStepper.tsx`  
**Fix**: Replace `✓` with SVG CheckCircle icon (10px, phosphor or inline). Add completion animation: `@keyframes phase-done { from { opacity: 0.5; transform: scale(0.9); } to { opacity: 1; transform: scale(1); } }` on phase transition. Current phase: add `border-b-2 border-b-gold` tab-active metaphor.

### B16 — Command palette trigger is visually muted
**File**: `components/shell/Topbar.tsx`  
**Issue**: The `⌘K` trigger is the most-used power-user affordance but looks like a disabled input.  
**Fix**: `border-border-strong` (not `border-border`). On hover: `border-gold/30 shadow-[0_0_0_1px_rgba(0,230,118,0.15)]`. Add `⌘K` badge with `bg-surface-panel border border-border rounded-sm`. Width: `xl:min-w-[300px]`.

---

## TIER 3 — Low Priority (nice-to-have polish)

### C1 — Strategy empty state is context-blind
**File**: `routes/strategies.tsx`  
**Issue**: "No strategies match these filters" shown even when total is 0 with no filters. New Strategy button silently creates unnamed strategy.  
**Fix**: Distinguish zero-total from zero-filtered. Enforce `display_name` required in creation flow.

### C2 — `decisionMode` labels use implementation vocabulary
**File**: `routes/strategies.tsx`  
**Fix**: `filter-gated agent` → `AI + filter`, `agent-direct` → `AI only`, `rules-only` → `Rules only`, `missing agent` → `Setup needed`

### C3 — "MAX DD" abbreviation unexplained
**File**: `components/home/StrategyOutcomesList.tsx`  
**Fix**: Expand to "Max drawdown". Add `title` tooltip explaining the metric. `—` values: `data-tooltip="No eval data yet"`.

### C4 — Topbar sub-lines use jargon/incoherent copy
**File**: `components/shell/Topbar.tsx` (each route's sub prop)  
**Issues**: "cockpit · N strategies" (dashboard), "Tonight's run, experiment writers, and recent cycles" (optimizer)  
**Fix**: Dashboard: `{n} strategies · {m} eval runs`. Optimizer: `Mutate and test strategy prompts automatically`.

### C5 — NagStrip tone dots are imperceptible
**File**: `components/home/NagStrip` (or similar)  
**Issue**: 6px tone dots as sole severity indicator — fails color-blind users.  
**Fix**: Replace with inline Icon (warning triangle / x-circle). Add `aria-label` to show/hide buttons. Remove em-dash from detail text.

### C6 — `compare` action only reachable via multi-step table workflow
**File**: `routes/eval-runs.tsx`, `routes/eval-runs-detail.tsx`  
**Fix**: Add "Compare with..." action on run detail page → inline picker for second run, same strategy. Pre-fill as first-class action, not a table multi-select workflow.

### C7 — LiveStrategiesSection renders above strategy outcomes
**File**: `routes/home.tsx`  
**Fix**: Move `LiveStrategiesSection` to below `StrategyOutcomesList`. Hide completely when no live runs. Prime dashboard real estate is for the core value prop.

### C8 — Arrow hover on optimizer session rows doesn't slide (blinks)
**File**: `features/autooptimizer/screens/OptimizerHome.tsx` — RecentSessionRow  
**Issue**: `→` arrow fades in with `opacity-0 group-hover:opacity-100` — no transform.  
**Fix**: `opacity-0 translate-x-[-4px] group-hover:opacity-100 group-hover:translate-x-0 transition-[opacity,transform] duration-150`

### C9 — Scrollbar track has unnecessary border
**File**: `styles/globals.css — .xvn-scroll`  
**Fix**: Remove `border-left: 1px solid var(--border-soft)` from scrollbar track. Reduce width 10px → 8px. The gold gradient thumb is enough character.

### C10 — Win-rate threshold too high for early users
**File**: `components/home/StrategyOutcomesList.tsx`  
**Issue**: Win coloring requires 10+ evals. Most users will have <10 per strategy.  
**Fix**: Lower threshold to 3 completed evals. Add tooltip explaining: "Profitable: return > 0 and Sharpe > 1.0"

### C11 — Onboarding tour targets DOM IDs (fragile)
**File**: `features/onboarding/useFirstRunTour.ts`  
**Fix**: Replace DOM ID targeting with `data-tour-target` attributes. Ensure RestartTourButton is discoverable in Settings.

### C12 — Phase numbers exposed in production EmptyPanel
**File**: `features/autooptimizer/ui/EmptyPanel.tsx`  
**Issue**: "Phase 2/3/4" badges expose internal roadmap state to operators.  
**Fix**: Remove Phase badge. Replace with "Available in a future release" or remove section entirely until shipped.

### C13 — ImprovementChart empty state is generic
**File**: `features/autooptimizer/ui/ImprovementChart.tsx`  
**Fix**: Add minimal SVG rising-line illustration (purely decorative, `stroke='var(--border-strong)'`, 80px), message at `text-[13px] text-text-2`, linked CTA `<Link to='/optimizer'>Start Optimizer →</Link>` in `text-gold`.

### C14 — Text color ramp has generic blue-gray coolness
**File**: `styles/tokens.css — --text-2 through --text-4`  
**Issue**: Secondary grays are Tailwind generic gray-400/500/600 equivalents — cool blue-gray reads as "disabled" rather than "secondary."  
**Fix (optional)**: Shift toward warm gray-green: `--text-2: #a8b4a8`, `--text-3: #6b7a6b`, `--text-4: #404840`. Makes green accent feel coherent with the data palette.

---

## Cross-Cutting Themes (for implementation grouping)

### Theme 1: "Show the profit" (H4, A8, B6)
The app's core value prop — profitable strategies — is visually suppressed. Return %s, Sharpe, win rates need to be the biggest, boldest elements on every results page. One afternoon of work.

### Theme 2: "Make it move" (A1, C8)  
Zero enter/exit animations makes the app feel static and unfinished. Add `row-enter` and `fade-in` keyframes globally, apply with staggered delays. No libraries needed. One afternoon.

### Theme 3: "Fix the broken things first" (H1, H2, A6)
Start button that does nothing. Dashboard with no story. Preflight errors that don't block the form. These are functional failures visible on a 5-minute demo. Critical path.

### Theme 4: "Surface depth" (A2, B3)
Cards are invisible against the background. Pure flat black looks unfinished. Shadow tokens and hairline ambient gradients are pure CSS — no component refactoring.

### Theme 5: "Token hygiene" (B1, B5, B10)
61 shadcn tokens resolving to undefined, hardcoded Tailwind colors bypassing the token system. These may already be causing invisible text. One grep-and-replace pass.

---

## Implementation Priority Order

For a hackathon preparation sprint, implement in this order:

1. **H1** — Fix Optimizer Start button (functional broken item)
2. **H2** — Fix dashboard narrative + hide empty ActiveTasksStrip
3. **H4** — Make profitable strategies visually dominant (color + size)
4. **H3** — Optimizer home hierarchy (collapse non-essential sections)
5. **H5** — Resolve raw IDs → display names throughout
6. **A1** — Add enter animations to lists and panels
7. **A2** — Card shadows
8. **A3** — Button active:scale
9. **A4** — Sidebar navigation grouping
10. **B1** — Fix shadcn token leakage (may be invisible text right now)
11. **B2** — Focus rings (accessibility baseline)
12. **A8** — Financial data visual weight
13. **B4** — Typography: bolder page titles
14. **B3** — Atmospheric depth gradients
15. **A6** — Preflight error blocking

Items **B8–C14** are polish and can be done as time allows.

---

*Generated by 4 parallel skill audits: `make-interfaces-feel-better`, `interface-design`, `bencium-innovative-ux-designer`, `impeccable`*  
*Total unique findings: ~90 de-duplicated into 53 actionable items across 3 tiers*

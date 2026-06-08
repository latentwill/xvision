# Handoff: Optimizer surface redesign (`/optimizer`)

## Overview
This package specifies a redesign of the XVN **Optimizer** surface (the autoresearch / autooptimizer dashboard at route `/optimizer`). The goal is to collapse today's loose stack of ~9 full-width panels into **one legible control surface** organized as three honest zones:

> **Command** (steer it) → **Live** (watch it) → **History** (review it)

The redesign removes redundant and non-functional controls, gives the page a single real primary action, and makes "live work being done" visually legible via a phase spine + an experiments×regimes heatmap.

This is a **refactor of an existing screen in an existing codebase** — not a greenfield build. Most P0 work is deletion and relocation.

## About the design files
The file in this bundle — `Optimizer-Redesign-Proposal.html` — is a **design reference created in HTML**. It is an annotated design-review document (diagnosis + before/after blueprint + prioritized fixes), **not production code to copy**. The blueprint blocks are schematic; treat them as a layout and information-architecture spec, not pixel art.

Your task is to **implement this redesign inside the existing React app** at `frontend/web/`, using its established patterns (React 18 + react-router + TanStack Query + Tailwind mapped onto CSS variables). Do **not** introduce the HTML file's bespoke CSS — reuse the codebase's tokens, primitives, and existing autooptimizer components.

## Fidelity
**Low-to-mid fidelity (structural).** The proposal communicates information architecture, layout zones, control hierarchy, and exact copy — but the visual styling should come from the **existing codebase design system** (`src/styles/tokens.css` + Tailwind), which is already the correct dark "Signal" theme. The numeric color/spacing values in the Design Tokens section below are the real codebase tokens; match those, not the HTML file's inlined hex.

A higher-fidelity, animated mock of the *running* state (heatmap lighting up, launch drawer) already exists in this project as `Autoresearch.html` (+ its `ar-*.jsx` files) — see **References** at the bottom. Port the heatmap visual from there.

---

## Target codebase map

All paths relative to `frontend/web/src/features/autooptimizer/`.

| File | Role today | Action in redesign |
|---|---|---|
| `screens/OptimizerHome.tsx` | Page root: stacks StatusHero, ConfigureSection, ScheduleStrip, charts, FlywheelStrip, LiveCycleView, ExperimentWritersPanel, RecentCyclesTable, RecentSessionsList | **Rebuild as the 3-zone layout.** Becomes the orchestrator. |
| `LiveCycleView.tsx` | 1100-line live view: `LivePageHeader`, `LaunchStrip` (the real launch form), `CycleLeftCard`, `EventLogCard`, `KeptNextCard`, 3-col grid `[300px·1fr·260px]`, plus its own `RecentCyclesSectionFull` | **Decompose.** Keep the SSE/event logic; replace the 3-col layout with the new Live zone. Delete its duplicate recent-cycles table. |
| `ui/PhaseStepper.tsx` | 7-phase horizontal stepper, only shown inside StatusHero when running | **Promote** to a full-width "phase spine" directly under the command bar. |
| `ui/ModePicker.tsx` | once / N / until-budget radio selector | **Move inside** the single launch drawer as a tab/segment. Keep as-is. |
| `ui/ScheduleStrip.tsx` | Scheduled-run config strip | **Fold into** the launch drawer as a "Schedule" tab. Remove from page top level. |
| `ui/ImprovementChart.tsx`, `ui/OutcomeStackedChart.tsx`, `ui/SpendChart.tsx` | uPlot charts | Keep. Show in the **idle** state and/or a collapsible "Trends" area. |
| `ui/FlywheelStrip.tsx` | DSPy flywheel progress (hidden when `dspy_enabled=false`) | Keep, demote to a secondary strip. |
| `panels/RecentCyclesTable.tsx` | Standalone history table | **Keep ONE** ledger. Delete the duplicate inside LiveCycleView. |
| `panels/ExperimentWritersPanel.tsx` | Writer ladder panel | Keep; move below the fold. |
| `panels/EvalMatrix.tsx`, `panels/RegimeCards.tsx`, `panels/GateBuckets.tsx` | experiments × regimes views | Use `EvalMatrix` as the **live heatmap anchor** in the Live zone. |
| `api.ts` | `useOptimizerStatus`, `useOptimizerStats`, `useSessionList`, pause/resume/cancel, `startRunCycle` | No API changes required for P0–P1. Reuse all hooks. |

### Two launch code paths exist today (this is a core problem to fix)
1. `OptimizerHome.tsx` → `ConfigureSection` → **`Start` button only calls `document.getElementById("optimizer-run-controls").scrollIntoView()`** — it does not start a run. (See `OptimizerHome.tsx` ~line 188–193.)
2. `LiveCycleView.tsx` → `LaunchStrip` → `handleLaunch()` → `onStartLoop()` → `startRunCycle()` — the **real** launch, behind a ghost button labelled "Run optimizer" (`border-gold / text-gold`) at the bottom of the left column (`#optimizer-run-controls`).

**Unify these into one drawer with one real submit.** Delete the scroll-only `Start`.

---

## Screens / Views

There is one screen, `/optimizer`, with two primary states.

### Zone 1 — Command bar (always visible)
- **Purpose:** show optimizer state and expose the one primary action.
- **Layout:** single full-width row, `rounded-md border border-border bg-surface-card px-5 py-4`. Left group + right group, `justify-between`, `flex-wrap`.
- **Left group:**
  - State pill — reuse `components/primitives/Pill`. `running` → `tone="gold" animated`; `paused`/`cancelling` → `tone="warn"`; `idle`/`finished` → `tone="default"`; `failed` → `tone="danger"`. (Logic already in `StatePill` in `OptimizerHome.tsx`.)
  - Active-run identity (mono, `text-text-2`): `cyc-… · {strategy_id} · {mode}` when active; else `text-text-3` "No run in progress".
- **Right group (the fix):** exactly **one** primary button, contextual:
  - idle/finished → **`Launch run`** (solid: `bg-accent text-on-accent`, `rounded px-3 py-1.5 text-[13px] font-medium`). Opens the Launch drawer (Zone 5).
  - running → **`Pause`** (secondary outline) + **`Cancel`** (danger outline).
  - paused → **`Resume`** (solid) + **`Cancel`** (danger outline).
  - When active, also a `Watch live →` text link to `/optimizer/run/:id`.
- **Remove:** the standalone `Start` button, the header's `Configure run` anchor `<a href="#optimizer-run-controls">`, and any second "Run optimizer" button. One primary only.

### Zone 2 — Phase spine (visible when running)
- **Purpose:** the heartbeat — where the loop is right now.
- **Component:** promote `ui/PhaseStepper.tsx` to a full-width strip directly under the command bar.
- Phases (already defined): `Briefing → Parent selection → Writing experiment → Evaluating → Gate review → Committing → Finishing`.
- Completed → dimmed `opacity-50` with `✓`; current → `border-gold/50 bg-gold/10 text-gold` + live elapsed seconds ticker; future → neutral `border-border text-text-3`.
- Horizontally scrollable on narrow viewports (already handled).

### Zone 3 — Live work band (visible when running)
- **Purpose:** the single image of work being done.
- **Layout:** 2-up grid on a shared baseline, e.g. `grid grid-cols-1 xl:grid-cols-[1.3fr_1fr] gap-6 items-stretch`. **Replace** the old `xl:grid-cols-[300px_1fr_260px]` three-column.
  - **Left ≈ 60%:** experiments × regimes **heatmap** (use/extend `panels/EvalMatrix.tsx`). Each cell = one backtest: `done` filled (`gold-bg-strong` + `gold-soft` border), `testing` animated (info-blue shimmer), `queued` neutral (`surface-panel`), `failed` flash (`danger`). Caption: `{evalsDone} / {evalsTotal} evals · live`. Port the exact cell styling + `bar-flow` shimmer from `Autoresearch.html` → `ar-home.jsx` `HeatCell`.
  - **Right ≈ 40%:** the live **event feed** (`EventLogCard` from LiveCycleView, keep SSE wiring) with the **live spend ticker** (`LiveCostTicker`) pinned at its top or bottom. Equal height to the heatmap.
- The thin right-hand "Kept / Next" rail (`KeptNextCard`) is **removed** — its data moves to Zone 4.

### Zone 4 — Outcome strip (running) / last-run summary (idle)
- **Purpose:** outcomes as a horizontal KPI row, not a tall rail.
- **Layout:** `grid grid-cols-2 sm:grid-cols-4 gap-2`, each tile `border border-border rounded bg-surface-panel px-3 py-2`.
- **Tiles:** `Kept` (gold), `Suspect` (warn), `Dropped` (text-2), `Top Δ` (gold). Values from `useOptimizerStatus().active_session` (`kept_count`, `suspect_count`, `dropped_count`) and `useOptimizerStats()` for top Δ.
- When idle: show last-run summary + the `ImprovementChart` here instead.

### Zone 5 — Launch drawer (one inline surface)
- **Purpose:** the single consolidated launch/config form. Replaces ConfigureSection + ScheduleStrip + LaunchStrip.
- **Behavior:** expands **inline** from the Zone-1 `Launch run` button (no `scrollIntoView`, no page jump). Collapses to a one-line summary (`parent · budget · window · models`) while a run is active.
- **Contents (port from `LaunchStrip` + `ModePicker` + `ScheduleStrip`):**
  - Run mode segment: `once` / `N experiments` (+ count, ≥1) / `until budget` (+ USD, >0) — `ModePicker`.
  - Parent strategy `<select>` (from `listStrategies`).
  - Per-cycle budget cap (USD, optional), Max cycles, Total budget.
  - Evaluation window (day start/end, baseline start/end) — collapsed "Advanced" by default.
  - Model overrides: experiment-writer model + reviewer model (`ModelPicker`), persisted via existing `preferences.ts` helpers.
  - Optional "Schedule" tab (from `ScheduleStrip`).
  - **One** real submit button → `startRunCycle(...)` via the existing `onStartLoop`/`startLoop` path. Solid primary, full-width inside the drawer. Validation messages inline (already in `handleLaunch`).

### Zone 6 — History (below the fold, one ledger)
- **Purpose:** review past work — exactly one table.
- **Component:** keep `panels/RecentCyclesTable.tsx`. **Delete** `RecentCyclesSectionFull` inside `LiveCycleView` and fold `RecentSessionsList` into this one table via a `Runs ⇄ Cycles` toggle.
- Columns (cycles): `Cycle ID · Experiments · Kept · Cost · Tokens · Best diversity · First seen`. Rows link to `/optimizer/cycle/:id`.
- Below it, keep `ExperimentWritersPanel` and the secondary `FlywheelStrip`.

---

## Interactions & behavior
- **Launch:** `Launch run` opens drawer → fill/confirm → submit calls `startRunCycle()`. On success the command bar flips to running, phase spine + live band appear. On error, inline message in the drawer (reuse `launchError`/`loopError`).
- **Pause / Resume / Cancel:** existing mutations `usePauseSession`, `useResumeSession`, `useCancelSession` — keep, just relocate the buttons to the command bar.
- **Continuous loop:** the auto-relaunch loop (maxCycles / totalBudget) in `LiveCycleView` stays; surface "next cycle queued…" in the live-status line.
- **SSE:** keep the `EventSource` wiring in `LiveCycleView` (`/api/autooptimizer/events`). Heatmap and feed both derive from the same event buffer.
- **Idle vs running:** layout differs by `status.active_session.state`. Idle → lead with Launch + last-run summary + improvement chart, hide Zones 2/3. Running → lead with spine + live band, collapse the drawer.
- **Responsive:** zones stack to one column below `xl`; phase spine scrolls horizontally; KPI strip goes `grid-cols-2`.
- **Reduced motion:** gate the heatmap shimmer + pulse on `@media (prefers-reduced-motion: no-preference)`.

## State management
No new global state. Reuse existing hooks:
- `useOptimizerStatus()` — drives command bar, zone visibility, KPI values.
- `useOptimizerStats()` — improvement/outcome charts + top Δ.
- `useSessionList()` — history "Runs" view.
- `useLineageNodes()`, `useCycleRuns()`, `useCycleCost()` — history "Cycles" view + live cost.
- Local component state: drawer open/closed; history toggle (runs|cycles); launch-form fields (already in `LaunchStrip`).

## Design tokens (use the codebase tokens — `src/styles/tokens.css`)
These are the real app values; the HTML file's inlined hex mirror them. Prefer Tailwind utilities mapped to these vars (`bg-surface-card`, `text-text-3`, `border-border`, `text-gold`, etc.).

| Token | Dark value | Use |
|---|---|---|
| `--bg` | `#000000` | page background |
| `--surface-card` | `#0a0a0a` | panels / zones |
| `--surface-elev` | `#0e0e0e` | nested blocks / hover |
| `--surface-panel` | `#121212` | KPI tiles, inputs |
| `--border` | `#1a1a1a` | default borders |
| `--border-strong` | `#2a2a2a` | emphasized borders, secondary buttons |
| `--border-soft` | `#141414` | hairlines |
| `--text` | `#ffffff` | primary text |
| `--text-2` | `#9ca3af` | secondary text |
| `--text-3` | `#5f6670` | labels / mono captions |
| `--text-4` | `#3a3f47` | separators |
| `--gold` (accent) | `#00e676` | brand accent, primary, "kept", running |
| `--gold-soft` | `#00b85f` | accent borders |
| `--gold-bg` / `--gold-bg-strong` | `rgba(0,230,118,.10)` / `.18` | accent fills, done cells |
| `--warn` | `#ffb020` | paused / suspect |
| `--danger` | `#ff4d4d` | cancel / failed / dropped |
| `--info` | `#5fa8ff` | testing cells |
| `--radius-card` / `--radius-sm` | `6px` / `4px` | radii |

A `light` theme variant exists in `tokens.css` — keep everything token-driven so both themes work. Type: **Geist** (UI) + **Geist Mono** (numbers/labels, with `tabular-nums`); uppercase mono labels use `tracking-[0.22em] text-[9.5px]`.

## Assets
No new image assets. The `GenArt` lineage thumbnails seen in `Autoresearch.html` are generated, not required for P0. Icons come from the codebase's existing icon set.

## Implementation order (priority)
- **P0 — Kill the phantom Start; one real launch button.** Delete the `scrollIntoView` Start; promote the real launch to a single solid primary in the command bar. (~½ day)
- **P0 — Consolidate three launch surfaces into one drawer.** Merge ConfigureSection + ScheduleStrip + LaunchStrip. (~2 days)
- **P0 — De-duplicate history.** Remove the second recent-cycles table; merge runs+cycles into one ledger with a toggle. (~½ day)
- **P1 — Rebuild the live zone horizontally.** Replace `[300px·1fr·260px]` with phase spine + 2-up work band + horizontal KPI strip. (~2–3 days)
- **P1 — Make the heatmap the live anchor.** Port the animated experiments×regimes cells from the mock. (~2 days)
- **P2 — Distinct idle vs running layouts.** (~1–2 days)

## Files in this bundle
- `Optimizer-Redesign-Proposal.html` — the design-review reference (diagnosis, before/after blueprint, prioritized fixes). Self-contained; open in a browser.
- `README.md` — this document.

## References (in the design project, not this bundle)
- `Autoresearch.html` + `ar-home.jsx`, `ar-shared.jsx`, `ar-cycle.jsx` — higher-fidelity mock of the running state and the **experiments × regimes heatmap** (`HeatCell`, `bar-flow` shimmer) to port.
- Real source to refactor: `frontend/web/src/features/autooptimizer/` (`screens/OptimizerHome.tsx`, `LiveCycleView.tsx`, `ui/*`, `panels/*`, `api.ts`).
- Tokens: `frontend/web/src/styles/tokens.css`; Tailwind mapping: `frontend/web/tailwind.config.ts`.

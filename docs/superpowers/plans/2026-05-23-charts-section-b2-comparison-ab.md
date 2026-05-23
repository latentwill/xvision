# Charts Section B2 — Comparison AB Scalable (`/charts/compare`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Scaffold status (2026-05-23):** Topology + files + acceptance gates only. Per-step TDD bodies are written when the contract is claimed and B0 + B1 have merged (B2 builds on B1's `MultiStrategyEquityPane` + `Topbar` + theme tokens).

**Goal:** Replace `/charts/compare`'s B0 placeholder with the Chart 02 design — hero overlay equity (N selected strategies) + interactive strategy roster pill rail + auto-flow strategy card grid (2/4/6/8 columns by selection size) — all sharing one URL-synced selection state.

**Architecture:** New surface `ComparisonABDashboard`. Reuses `MultiStrategyEquityPane` and `Topbar` from B1. Three new primitives: `StrategyRosterPills`, `StrategyCardGrid` (+ `StrategyCard` + `MiniSparkline`), and `LeadCardChrome`. One new hook `useChart2Roster` for URL-synced selection state. No new backend work — reuses B1's `/api/v2/charts/dashboards/overview` payload.

**Tech Stack:** React 18 + TypeScript, uPlot, react-router-dom v6 `useSearchParams`.

**Spec:** `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §3 B2 milestone, §4A.1 (StrategyRosterPills, StrategyCardGrid, StrategyCard, MiniSparkline, LeadCardChrome), §4A.4 (useChart2Roster hook).

**Prereqs:**
- B0 merged (theme rotation tokens, `/charts/compare` placeholder, fixture).
- B1 merged (`MultiStrategyEquityPane` reused as the hero overlay; `Topbar` reused with a different headline; `/api/v2/charts/dashboards/overview` real builder so the bundle has all strategies present in the rotation).
- Design reference: `docs/design/trading-charts/XVN.zip` → `design_handoff_charts/source/charts/chart-comparison-ab.jsx` + README.md §02.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `frontend/web/src/components/chart/v2/primitives/StrategyRosterPills.tsx` | Create | Horizontal pill rail. On-state `${color}14` bg + `${color}55` border. Off-state muted. `×` removes; click toggles. Min 2 (× disabled at 2). Stable order = rotation order |
| `frontend/web/src/components/chart/v2/primitives/StrategyCardGrid.tsx` | Create | CSS-grid wrapper. Column count from N selected: `n≤2→2`, `n≤4→4`, `n≤6→3`, `n≥7→4`. Reflows via `grid-template-columns` change |
| `frontend/web/src/components/chart/v2/primitives/StrategyCard.tsx` | Create | Card: head (color dot + Cormorant name + `.caps` kind line + `LEAD` badge + `×` if removable) → `MiniSparkline` → 2×2 metrics → indicator chip strip (`RSI 56`, `MACD ↑`, `EMA`, `Fib · 0.618`) |
| `frontend/web/src/components/chart/v2/primitives/MiniSparkline.tsx` | Create | Micro `UplotLinePane` variant: no axes, no legend, no cursor; deterministic area-fill via `xvnAreaFill` plugin. Prop toggles price line vs drawdown line |
| `frontend/web/src/components/chart/v2/primitives/LeadCardChrome.tsx` | Create | HOC wrapper. Gradient backdrop `linear-gradient(180deg, rgba(212,165,71,0.04), var(--surface-card) 38%)` + gold-tinted border + 1px gold gradient line across the top. Applied to `selectedIds[0]` only |
| `frontend/web/src/components/chart/v2/primitives/*.test.tsx` | Create (5 files) | One per primitive |
| `frontend/web/src/components/chart/v2/hooks/useChart2Roster.ts` | Create | State: `selectedIds: string[]`. URL-synced via `useSearchParams` (`?ids=fib,ema,brk`). Exposes `add`, `remove`, `toggle`, `setLead`, `count`. Min-2 invariant enforced |
| `frontend/web/src/components/chart/v2/hooks/useChart2Roster.test.ts` | Create | Min-2 invariant, URL round-trip, toggle behavior |
| `frontend/web/src/components/chart/v2/surfaces/ComparisonABDashboard.tsx` | Create | Composition: `ChartFrame` · `Topbar` (headline = "`N` strategies, one frame") · `MultiStrategyEquityPane` (hero overlay; legend chips include return as JetBrains Mono micro-stat) · `StrategyRosterPills` · `StrategyCardGrid` of `StrategyCard`s |
| `frontend/web/src/components/chart/v2/surfaces/ComparisonABDashboard.test.tsx` | Create | Renders against fixture; adding/removing a roster pill reflows grid column count without remount flicker; LeadCardChrome only wraps selectedIds[0] |
| `frontend/web/src/routes/charts/ChartsCompare.tsx` | Modify | Replace B0 placeholder. Fetch `/api/v2/charts/dashboards/overview`; render `<ComparisonABDashboard payload={data} />` |
| `frontend/web/src/routes/chart-lab/dashboards/Compare.tsx` | Create | `/chart-lab/dashboards/compare` — fixture render for visual regression |
| `team/contracts/charts-section-b2.md` | Create | Track contract |
| `team/status/charts-section-b2.md` | Create | Status file |

---

## Task topology

1. **Hook: `useChart2Roster`** — URL-synced selection state. TDD with `MemoryRouter` + `useSearchParams`.
2. **Primitive: `MiniSparkline`** — micro uPlot variant. TDD against fixture series.
3. **Primitive: `StrategyCard`** — composes `MiniSparkline`; props for strategy + lead-flag + onRemove. Snapshot test.
4. **Primitive: `LeadCardChrome`** — HOC; snapshot test the gradient stack.
5. **Primitive: `StrategyCardGrid`** — column-count math test cases (n=1..12).
6. **Primitive: `StrategyRosterPills`** — interaction tests (click toggles, `×` removes, min-2 disables ×).
7. **Surface: `ComparisonABDashboard`** — composition.
8. **Wire `ChartsCompare` route** + URL preservation. Replace B0 placeholder.
9. **`/chart-lab/dashboards/compare`** fixture page.
10. **Verification gate** — visual diff vs `docs/design/trading-charts/xvn theme charts 2.png`; URL deep-linking (`/charts/compare?ids=fib,ema,brk`) restores selection.

---

## Acceptance gates

- Adding or removing a roster pill reflows the grid column count without remount flicker. (Test: assert the same `StrategyCard` instance keys are preserved across reflows.)
- Grid column widths sum within ±2px at all breakpoints — no horizontal scroll, no overflow.
- `LeadCardChrome` only ever wraps `selectedIds[0]`; reordering `selectedIds` by `setLead` swaps which card is lead.
- The `×` button is disabled when `selectedIds.length === 2`.
- `MiniSparkline` renders an area-fill in the strategy's color for the price-line variant and in `var(--danger)` α=0.22 for the drawdown variant.
- URL deep-link `/charts/compare?ids=fib,ema,brk` reads as exactly those three strategies selected (in that order).
- `npm run typecheck && npm test && npm run build` clean.

---

## Out of scope for B2

- Server-side filtering of strategies (B2 reuses the full B1 bundle and filters client-side).
- A persistent "saved comparison" feature (URL is the only persistence mechanism in B2).
- Drag-to-reorder of the card grid.

## Sources

- Spec: `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §3 B2, §4A.1, §4A.4.
- Design reference: `docs/design/trading-charts/XVN.zip` → `design_handoff_charts/README.md` §02, `source/charts/chart-comparison-ab.jsx`.
- B0 plan: `docs/superpowers/plans/2026-05-23-charts-section-b0-foundation.md`.
- B1 plan: `docs/superpowers/plans/2026-05-23-charts-section-b1-overview-dashboard.md`.

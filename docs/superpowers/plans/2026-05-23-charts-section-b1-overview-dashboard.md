# Charts Section B1 — Dark Minimal Strategy Dashboard (`/charts/overview`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Scaffold status (2026-05-23):** This plan defines the task topology, files, primitives, and acceptance gates. The per-step TDD body is intentionally deferred until **B0 has merged** and the foundation primitives + payload endpoint stub are on `origin/main`. Expanding to full TDD form is the first action of the worker who claims this contract — see `team/contracts/charts-section-b1.md` (to be authored once B0 lands).

**Goal:** Replace `/charts/overview`'s B0 placeholder with the Chart 01 design — multi-strategy equity overlay + drawdown card + monthly-returns heatmap + 5-up KPI row + Topbar — composed entirely from chart-v2 primitives against the real `/api/v2/charts/dashboards/overview` payload.

**Architecture:** New surface `DarkMinimalDashboard` under `frontend/web/src/components/chart/v2/surfaces/`. Five new primitives co-located in `chart/v2/primitives/`. Backend side: replace the B0 stub handler with a real builder that pairs each `Strategy` (with its new `color` column or rotation fallback) to its latest backtest run's equity series.

**Tech Stack:** React 18 + TypeScript + Vite, uPlot, TanStack Query. Rust 2021 + sqlx for the real builder.

**Spec:** `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §3 B1 milestone, §4A.1–4A.2 (primitives + surface map), §6.1 (payload).

**Prereqs:**
- B0 merged: `Strategy.color` migration on disk, theme tokens extended, fixtures committed, `/api/v2/charts/dashboards/overview` stub returning a deterministic bundle, `/charts/overview` placeholder route mounted, `xvn.chartv2=1` cookie scheme established.
- Design reference: `docs/design/trading-charts/XVN.zip` → `design_handoff_charts/source/charts/chart-dark-minimal.jsx` + README.md §01.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `frontend/web/src/components/chart/v2/primitives/MultiStrategyEquityPane.tsx` | Create | uPlot pane: N overlaid equity series, lead halo via ported `xvnLastDot`, dashed benchmark, cursor sync key `'hero'`. Per-series stroke: lead 1.7px, others 1.15px |
| `frontend/web/src/components/chart/v2/primitives/MultiStrategyEquityPane.test.tsx` | Create | Renders N series, lead picked correctly, dashed series rendered for `dashed:true` entries |
| `frontend/web/src/components/chart/v2/primitives/DrawdownCard.tsx` | Create | Composes `UplotDrawdownPane` + 4-cell footer (Max DD / Avg DD / Duration / Recovery), `.caps` eyebrow labels |
| `frontend/web/src/components/chart/v2/primitives/MonthlyReturnsHeatmap.tsx` | Create | SVG/DOM grid N × M. Cell color `var(--gold)`/`var(--danger)`; α = `clamp(0.10, abs(value)/0.15, 0.65)`. Bottom legend `-15% ↔ +15%` |
| `frontend/web/src/components/chart/v2/primitives/MonthlyReturnsHeatmap.test.tsx` | Create | α math correct at value=0, 0.05, 0.15, -0.20 (clamped) |
| `frontend/web/src/components/chart/v2/primitives/KpiCard.tsx` | Create | 5-up row primitive. Value Cormorant 30–32px; label `.caps`; foot JetBrains Mono 11px. Variants: red foot for Max Drawdown |
| `frontend/web/src/components/chart/v2/primitives/Topbar.tsx` | Create | Eyebrow + Cormorant headline + italic tagline. Right cluster: env pill, timeframe toggle row (`1D 1W 1M 3M YTD 1Y ALL`), `⤓ Export` ghost button |
| `frontend/web/src/components/chart/v2/adapters/uplot-plugins.ts` | Create | Port of `xvnLastDot`, `xvnAreaFill`, `xvnRegimeBands` from `chart-helpers.js` to typed uPlot plugins. Function signature: `(seriesIdx: number, opts: PluginOpts) => uPlot.Plugin`. NEW additions `xvnGradientFill` (5-stop, for B4) and `xvnSheen` (top-band, for B4) are stubs here that B4 fills in |
| `frontend/web/src/components/chart/v2/adapters/uplot-plugins.test.ts` | Create | Snapshot the `hooks.draw` callbacks' canvas operations against a stub Ctx |
| `frontend/web/src/components/chart/v2/surfaces/DarkMinimalDashboard.tsx` | Create | Composition: `ChartFrame` · `Topbar` · `KpiCard×5` · `MultiStrategyEquityPane` (cursor key `'hero'`) · `DrawdownCard` (cursor key `'hero'`) · `MonthlyReturnsHeatmap` |
| `frontend/web/src/components/chart/v2/surfaces/DarkMinimalDashboard.test.tsx` | Create | Renders against `multi-strategy-equity.json` fixture without console errors; KPI values match payload metrics; heatmap row count = number of strategies |
| `frontend/web/src/routes/charts/ChartsOverview.tsx` | Modify | Replace B0 placeholder. Use TanStack Query to fetch `/api/v2/charts/dashboards/overview`; render `<DarkMinimalDashboard payload={data} />` |
| `frontend/web/src/api/charts.ts` (new section) | Modify | `fetchDashboardOverview(): Promise<MultiStrategyEquityBundle>` |
| `crates/xvision-engine/src/api/charts_dashboards.rs` | Modify | Replace B0 stub. Real builder: load all active strategies, for each pair with its latest backtest run's equity series, resolve `color` (Strategy.color → rotation fallback by stable index), build drawdown server-side, derive monthly returns + metrics |
| `crates/xvision-engine/src/api/charts_dashboards.rs` *(adjacent helpers)* | Create | `fn fallback_color(idx: usize, rotation: &[StrategyRotationEntry]) -> &str` and `fn build_monthly_matrix(equity: &[f64], time: &[i64]) -> Vec<MonthlyReturn>` |
| `crates/xvision-engine/tests/charts_dashboards_overview_real_builder.rs` | Create | Seeded test DB with 3 strategies (one with `color`, two without); assert resolved colors, monthly matrix shape, lead picked correctly |
| `frontend/web/src/routes/chart-lab/dashboards/Overview.tsx` | Create | `/chart-lab/dashboards/overview` — renders `<DarkMinimalDashboard>` against the fixture, no fetch |
| `team/contracts/charts-section-b1.md` | Create | Track contract (authored when B1 is ready to dispatch) |
| `team/status/charts-section-b1.md` | Create | Status file (worker-authored) |

---

## Task topology

1. **Port `uplot-plugins.ts`** — TS port of `xvnLastDot`, `xvnAreaFill`, `xvnRegimeBands`. Includes stubs for `xvnGradientFill` + `xvnSheen` (B4 fills bodies). Tested against a stub Ctx.
2. **Primitive: `MultiStrategyEquityPane`** — uPlot wrapper with cursor sync key prop; per-series stroke width/dash from rotation; halo plugin attached to lead. Test against fixture.
3. **Primitive: `DrawdownCard`** — composes existing `UplotDrawdownPane` + 4-cell footer DOM. Test derived stats (Max DD, Avg DD, Duration, Recovery).
4. **Primitive: `MonthlyReturnsHeatmap`** — pure SVG grid + tooltip on hover. Test α math + legend bar.
5. **Primitive: `KpiCard`** — composable; emits one card per KPI; red foot variant.
6. **Primitive: `Topbar`** — DOM only; props for eyebrow / headline / tagline / right-cluster slot.
7. **Real builder for `/api/v2/charts/dashboards/overview`** — replace B0 stub. Backend tests with seeded DB.
8. **Surface: `DarkMinimalDashboard`** — composition. Test against fixture.
9. **Wire `ChartsOverview` route** — fetch + render. Replace B0 placeholder.
10. **`/chart-lab/dashboards/overview` page** — fixture render for visual regression.
11. **Verification gate** — full backend + frontend test sweep; manual smoke with cookie on/off; visual diff vs `docs/design/trading-charts/xvn theme charts.png`.

Each task expands to TDD steps (write failing test → run → minimal implementation → run → commit) when the worker picks up the contract.

---

## Acceptance gates

- B1 surface renders `multi-strategy-equity.json` fixture with no console errors in dark, folio-dark, light, and black themes.
- Cursor sync works between `MultiStrategyEquityPane` and `DrawdownCard` (single shared `'hero'` key).
- `MonthlyReturnsHeatmap` cell α scales correctly across ±15%; values outside the range clamp without overflowing the cell.
- Backend builder resolves `color` from `Strategy.color` when present; falls back to `strategyRotation` by **stable index** (not random) when NULL.
- Backend builder picks lead = first strategy by insertion order unless an explicit `lead` is configured per-account (followup; default is fine for B1).
- `/chart-lab/dashboards/overview` renders the surface full-bleed against the fixture; the page is the visual regression target for any future tweak to B1 primitives.
- No new `lightweight-charts` imports anywhere added in this milestone.
- `npm run typecheck && npm run test && npm run build` clean.
- `cargo test -p xvision-engine` clean (covers the new real-builder tests).

---

## Out of scope for B1

- Real-time WebSocket updates of the equity bundle (B1 is fresh-at-load).
- Per-strategy color-picker UI (separate follow-up; B1 reads existing `Strategy.color` only).
- The `⤓ Export` button is wired to a no-op handler in B1; the actual screenshot/export path is its own follow-up.
- Timeframe toggle (`1D 1W 1M 3M YTD 1Y ALL`) is visually styled but drives the chart range only on the client (no server round-trip for B1 — uses the full bundle and crops in uPlot).

## Sources

- Spec: `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §3 B1, §4A, §6.1.
- Design reference: `docs/design/trading-charts/XVN.zip` → `design_handoff_charts/README.md` §01, `source/charts/chart-dark-minimal.jsx`, `source/charts/chart-helpers.js` (plugin ports).
- B0 plan: `docs/superpowers/plans/2026-05-23-charts-section-b0-foundation.md`.

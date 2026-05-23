---
track: charts-section-b0
lane: foundation
wave: charts-section-2026-05-23
worktree: .worktrees/charts-section-b0
branch: task/charts-section-b0
base: origin/main
status: ready
depends_on: []
blocks:
  - charts-section-b1
  - charts-section-b2
  - charts-section-b3
  - charts-section-b4
stacking: none
allowed_paths:
  - crates/xvision-engine/migrations/034_strategies_color.sql
  - crates/xvision-engine/migrations/034_strategies_color.down.sql
  - crates/xvision-engine/src/strategies/**
  - crates/xvision-engine/src/api/charts_dashboards.rs
  - crates/xvision-engine/src/api/mod.rs
  - crates/xvision-engine/tests/strategies_color_roundtrip.rs
  - crates/xvision-engine/tests/charts_dashboards_overview.rs
  - crates/xvision-core/src/strategies/**
  - crates/xvision-core/migrations/034_strategies_color.sql
  - crates/xvision-core/migrations/034_strategies_color.down.sql
  - crates/xvision-core/tests/strategies_color_roundtrip.rs
  - crates/xvision-dashboard/src/routes.rs
  - frontend/web/src/api/types.gen/Strategy.ts
  - frontend/web/src/api/types.gen/MultiStrategyEquityBundle.ts
  - frontend/web/src/api/charts.ts
  - frontend/web/src/components/chart/v2/types.ts
  - frontend/web/src/components/chart/v2/theme.ts
  - frontend/web/src/components/chart/v2/theme.test.ts
  - frontend/web/src/components/chart/v2/__fixtures__/multi-strategy-equity.json
  - frontend/web/src/components/chart/v2/__fixtures__/annotations.json
  - frontend/web/src/components/chart/v2/__fixtures__/monthly-returns.json
  - scripts/gen-chart-v2-fixtures.ts
  - frontend/web/index.html
  - frontend/web/src/styles/globals.css
  - frontend/web/src/components/shell/Sidebar.tsx
  - frontend/web/src/components/shell/Sidebar.test.tsx
  - frontend/web/src/components/primitives/Icon.tsx
  - frontend/web/src/routes.tsx
  - frontend/web/src/routes/charts/**
  - frontend/web/src/routes/chart-lab/ChartLabDashboards.tsx
  - frontend/web/src/routes/chart-lab/index.tsx
  - team/MANIFEST.md
  - team/contracts/charts-section-b0.md
  - team/status/charts-section-b0.md
forbidden_paths:
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/eval/**
  - crates/xvision-engine/src/api/chart.rs            # Track A v1 chart endpoint — untouched
  - crates/xvision-engine/migrations/0[0-2]*.sql      # earlier migrations
  - crates/xvision-engine/migrations/033_*            # other in-flight migration
  - frontend/web/src/components/chart/*.tsx           # Track A v1 chart files (RunChart, CompareChart, ...)
  - frontend/web/src/components/chart/v2/surfaces/**  # Track B surfaces are B1-B4 territory, not B0
  - frontend/web/src/components/chart/v2/primitives/[A-Z]*.tsx   # B0 only adds payload types + extends theme; new primitive files are B1-B4
  - frontend/web/src/components/chart/v2/adapters/**  # likewise
  - frontend/web/src/components/chart/v2/hooks/**     # likewise (useChart2Roster is B2)
  - frontend/web/src/components/shell/ChatRail.tsx
  - frontend/web/src/components/shell/* (other than Sidebar.{tsx,test.tsx})
interfaces_used:
  - xvision_engine::strategies::Strategy (add `color: Option<String>` field with #[serde(default)])
  - xvision_engine::strategies::store::StrategyStore (insert + load round-trip the new column)
  - xvision_engine::api::charts_dashboards::MultiStrategyEquityBundle (NEW serializable type per spec §6.1)
  - frontend chart-v2 types.ts: AnnotatedChartPayload, Annotation, LiquidationHeatmapPayload, LiquidationLevel, MultiStrategyEquityBundle, DashboardFixtureKey (additive only)
  - frontend chart-v2 theme.ts: Chart2WarmPalette, Chart2StrategyRotationEntry, Chart2HeatRamp, Chart2Typography, Chart2Radius (additive only)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --check
  - cargo clippy -p xvision-engine --tests -- -D warnings
  - cargo test -p xvision-engine --test strategies_color_roundtrip
  - cargo test -p xvision-engine --test charts_dashboards_overview
  - cargo test -p xvision-engine
  - cd frontend/web && npm run typecheck
  - cd frontend/web && npm test -- "chart/v2/theme|shell/Sidebar|routes/charts"
  - cd frontend/web && npm run build
  - cd frontend/web && npm run gen:chart-v2-fixtures && git diff --exit-code frontend/web/src/components/chart/v2/__fixtures__/  # idempotency
  - bash scripts/board-lint.sh
acceptance:
  - Migration 034 reserved in team/MANIFEST.md (status: reserved → merged when PR lands)
  - `strategies.color TEXT` nullable column landed; up/down round-trips against the test schema
  - `Strategy.color: Option<String>` field with #[serde(default)] persists and reloads (3 round-trip tests pass)
  - ts-rs export emits `color?: string | null` on the frontend `Strategy` type
  - `Chart2ThemeDefinition` gains 5 new token groups (warm, strategyRotation [length 8, dashed:true on last entry], heatRamp [5 stops in descending heat order], typography, radius) on all 4 themes (dark/folio-dark/light/black). Snapshot test enforces shape
  - `multi-strategy-equity.json`, `annotations.json`, `monthly-returns.json` exist, generated deterministically (idempotent re-run produces no diff)
  - Google Fonts loaded once (no duplicate `<link>`); `.caps` utility class in globals.css
  - `Charts` sidebar entry mounted between `Scenarios` and `Eval`, behind `xvn.chartv2=1` cookie. Two Sidebar tests assert visibility-by-cookie and correct neighbor order
  - `/charts/{overview,compare,annotated,hero}` routes mounted with placeholder shells; `/charts` and `/charts/` redirect to `/charts/overview`
  - `/api/v2/charts/dashboards/overview` returns `MultiStrategyEquityBundle` with `kind: "multi_strategy_equity"`, `time.length >= 240`, `strategies.length === 5`, each strategy has a hex `color` and `equity.length >= 240`
  - `/chart-lab/dashboards` tab exists and lists the four B-milestones
  - No new `lightweight-charts` imports anywhere
---

# Scope

B0 of `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md`
Track B (Charts dashboard section). Foundation only — sidebar entry,
`/charts` route topology, `Strategy.color` schema migration, theme-token
superset mirroring the design handoff, three new fixtures, the
`/chart-lab/dashboards` review tab, and a fixture-backed
`/api/v2/charts/dashboards/overview` endpoint. B1–B4 (the four canvases)
build on this foundation in subsequent contracts.

Plan: [`docs/superpowers/plans/2026-05-23-charts-section-b0-foundation.md`](../../docs/superpowers/plans/2026-05-23-charts-section-b0-foundation.md).

# Out of scope

- The four B-milestone canvases (`DarkMinimalDashboard`,
  `ComparisonABDashboard`, `AIAnnotationDashboard`,
  `GradientHeroDashboard`) — those land in B1–B4 contracts.
- Real builder for `/api/v2/charts/dashboards/overview` (B0 ships a
  fixture-matched stub; B1 implements the real builder).
- Track A migration (M0–M4) — touched in a different track's contract.
- Mobile variants of the Charts section.
- Pin-a-color UI for `Strategy.color` (read-only in B0; UI is a
  separate follow-up).
- Annotation producer (called out explicitly in spec §9; B3 consumes
  annotations, doesn't generate them).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin

# Worktree create (if not already present):
git worktree add .worktrees/charts-section-b0 -b task/charts-section-b0 origin/main

# Inside the worktree:
cd .worktrees/charts-section-b0
git status
git log --oneline -3 origin/main..HEAD

# Verify migration 034 is reserved to you:
grep -A1 "^| 034 " team/MANIFEST.md

# Per-worktree cargo target dir (avoids collision with parallel agents):
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-charts-b0"
```

State out loud:

```
I am on branch task/charts-section-b0.
I am based on origin/main at commit <sha>.
My contract is team/contracts/charts-section-b0.md.
Migration 034 is reserved to charts-section-b0 in team/MANIFEST.md.
I will only edit paths matching allowed_paths.
```

# Migration number reservation

**034 — `charts-section-b0`** (`strategies.color` nullable TEXT column).

Registry row already added in `team/MANIFEST.md` by the planning commit.
**Verify** the row is present and points at this track before writing
the SQL file. If it has drifted (parallel rename, conflict resolution),
stop and reconcile with the conductor — do not rename to a free number
without an explicit registry edit.

The owning crate (`xvision-engine` vs `xvision-core`) is determined by
where the `strategies` table is currently defined. Both crates'
migration dirs appear in `allowed_paths`; you write to **one**, not
both, per the project guardrail in CLAUDE.md.

# Notes

(free-form; worker appends PR links, surprises, checkpoint summaries)

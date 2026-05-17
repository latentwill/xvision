---
track: ux-polish-eval-list-and-snapshot
lane: leaf
wave: ux-polish
worktree: .worktrees/ux-polish-eval-list-and-snapshot
branch: task/ux-polish-eval-list-and-snapshot
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/home.tsx
  - frontend/web/src/routes/eval-runs.tsx
  - frontend/web/src/routes/eval-runs.test.tsx
  - frontend/web/src/routes/home.test.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/api/**
  - crates/xvision-engine/migrations/**
interfaces_used:
  - "@/api/strategies — listStrategies, strategyKeys, StrategyListItem"
  - "@/api/scenarios — listScenarios, scenarioKeys, Scenario"
  - "@/api/runs — RunSummary"
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run eval-runs home
  - pnpm --dir frontend/web build
acceptance:
  - Home's `ControlChartCard` heading sub-line shows latest eval's strategy display name + scenario title + started_at date, with "Latest eval" framing
  - Eval list desktop table Strategy column renders strategy display_name (not 8-char agent_id slice)
  - Eval list desktop table Scenario column renders scenario title (not raw scenario_id)
  - Eval list mobile card mirrors the same friendly labels
  - Falls back to short id only when the strategy/scenario lookup misses (deleted upstream)
  - Eval list horizontal scroll area carries a visible affordance (persistent scrollbar styling OR edge fade gradient) that works in both light and dark themes
---

# Scope

Three small UI nits in one PR — see
`team/intake/2026-05-17-ux-polish-eval-list-and-snapshot.md` for the full
brief. Frontend-only, no API changes.

1. Home `ControlChartCard` — sub-line with latest eval title (strategy +
   scenario display names) and start date.
2. Eval list — replace `agent_id.slice(0, 8)` and raw `scenario_id` with
   strategy display name and scenario title in both desktop table and mobile
   card.
3. Eval list — visible horizontal scroll affordance (persistent scrollbar
   styling or edge fade gradient).

# Out of scope

- Backend API additions or rename of `agent_id`/`scenario_id` fields.
- Eval list filtering, pagination, or row layout changes.
- Backfilling display names for orphaned runs — fallback to short id is
  acceptable when the lookup misses.
- Any change outside `home.tsx` / `eval-runs.tsx` and their tests.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/ux-polish-eval-list-and-snapshot \
  -b task/ux-polish-eval-list-and-snapshot origin/main
git -C .worktrees/ux-polish-eval-list-and-snapshot status
```

# Notes

Implementation hints (do not rewrite the contract — use as starting points):

- `listStrategies()` and `listScenarios()` are already imported by
  `eval-runs.tsx`; build two `Map<string, displayName>` keyed by id once
  and look up per row.
- The Home page's chart payload is sourced from one specific run row; pass
  the row (or just the strategy/scenario ids + started_at) into
  `ControlChartCard` instead of only the chart payload.
- Dark-mode borders rule (CLAUDE.md): if you add an edge-fade gradient,
  derive its color from theme tokens (`bg-surface`, etc.), never a hard
  white.

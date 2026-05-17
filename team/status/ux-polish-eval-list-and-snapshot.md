---
track: ux-polish-eval-list-and-snapshot
status: pr-open
owner: claude/2026-05-17
last_update: 2026-05-17T22:25Z
---

# Status

All three nits in one PR:

1. **Home chart sub-line** — `ControlChartCard` now receives the latest
   `RunSummary` plus the strategy/scenario lookups and renders
   `Latest eval · {strategy_name} on {scenario_name} · {date}` under the
   heading. The card's "open eval →" link also deep-links to the
   specific latest run when one exists.
2. **Eval list display names** — `RunsTable` now resolves
   `agent_id → display_name` and `scenario_id → display_name` via the
   already-imported `listStrategies` / `listScenarios` queries. Both
   mobile card and desktop table render friendly labels, falling back
   to a short id when the lookup misses (deleted upstream).
3. **Horizontal scroll affordance** — desktop table is wrapped in a
   `relative` container with a right-edge gradient fade
   (`transparent → var(--surface)`) so it works in both light and dark
   themes without violating the dark-mode borders rule.

Tests added: 2 new in `eval-runs.test.tsx` (display names + lookup
fallback) and 2 new in `home.test.tsx` (sub-line content + open-eval
deep-link).

# Verification

- `pnpm typecheck`: pass
- `pnpm test -- --run eval-runs home`: 40 tests pass (4 files)
- `pnpm build`: success

# Notes

- Contract mentions `pnpm lint` — script does not exist in the frontend
  package.json, skipped.
- Followed CLAUDE.md dark-mode borders rule — no white/`#fff` in the
  gradient; uses `var(--surface)`.

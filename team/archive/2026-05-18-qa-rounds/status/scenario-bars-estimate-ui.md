---
track: scenario-bars-estimate-ui
worktree: .worktrees/scenario-bars-estimate-ui
branch: task/scenario-bars-estimate-ui
base: origin/main
phase: pr-open
last_updated: 2026-05-18T04:10:00Z
owner: claude
---

# What changed

- Root cause: `estimateBars(from, to, granularity)` in
  `frontend/web/src/components/scenario/ScenarioForm.tsx` only derived
  the count from the time window. When the operator typed a Context
  bars value but had not yet picked a from/to date, the estimate
  stayed at `0` because the time-window path returned `0` and there
  was no `warmupBars` term in the sum.
- Fix:
  - `estimateBars` now takes `contextBars: number` as a fourth
    argument and returns `windowBars(...) + max(0, floor(contextBars))`.
  - The call site passes the live `warmupBars` state.
  - `estimateBars` is exported so the unit suite can exercise the
    four cases the contract calls out — time-window only, context-bars
    only, both summed, zero / negative / NaN degrades to time-window.
- Added a UI regression that fires `change` on the Context bars input
  with `"100"` and asserts the estimate label no longer reads
  `"Estimated bars to fetch: 0"`. Matches the operator's repro from
  the round-3 intake.

# Verification

- Passed: `corepack pnpm --dir frontend/web test -- --run scenarios ScenarioForm` (44 tests)
- Passed: `corepack pnpm --dir frontend/web typecheck`
- Passed: `corepack pnpm --dir frontend/web build`

# Notes

- The formula `windowBars + contextBars` matches the backend semantics
  of `warmup_bars` (`Pre-window context bars … `DEFAULT_WARMUP_BARS` =
  200`) — those bars are physically fetched before the scenario window
  runs, so the "to-fetch" estimate must include them.
- No backend changes. The bug was purely a missing dependency in the
  client-side estimate.

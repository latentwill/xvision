---
track: tradingview-2-run-compare-charts
worktree: /root/deploy/xvision/.worktrees/tradingview-2-run-compare-charts
branch: tradingview-2-run-compare-charts
base: tradingview-1-chart-api-indicators
phase: verified
last_updated: 2026-05-14T08:06:11Z
owner: codex
---

# What changed

- Added an optional `dataTable` slot to `ChartContainer`.
- Added a `Data table` toolbar toggle that renders the fallback table below
  the chart shell when enabled.
- Added `RunChart` test coverage for range controls and the data-table
  fallback behavior.

# Checkpoints

- `feat(web): add chart container data table fallback`

# Verification

- `corepack pnpm --dir frontend/web install --frozen-lockfile`
- `corepack pnpm --dir frontend/web test -- RunChart`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`

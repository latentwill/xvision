---
track: tradingview-3-scenario-strategy-charts
worktree: /root/deploy/xvision/.worktrees/tradingview-3-scenario-strategy-charts
branch: tradingview-3-scenario-strategy-charts
base: tradingview-2-run-compare-charts
phase: verified
last_updated: 2026-05-14T08:10:30Z
owner: codex
---

# What changed

- Added an accessible `role="img"` label to populated scenario price charts.
- Wired scenario bars into the shared `ChartContainer` data-table fallback.
- Updated the fully cached badge copy and added ScenarioChart coverage for
  cache state, fetch action, and table fallback behavior.

# Checkpoints

- `feat(web): add scenario chart table fallback`

# Verification

- `corepack pnpm --dir frontend/web install --frozen-lockfile`
- `corepack pnpm --dir frontend/web test -- ScenarioChart`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`

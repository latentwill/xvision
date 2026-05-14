# Claim: tradingview-2-run-compare-charts

Claimed: 2026-05-14T08:06:11Z

Worktree: `.worktrees/tradingview-2-run-compare-charts`

Branch: `tradingview-2-run-compare-charts`

Scope:

- Execute the next available slice of `docs/superpowers/plans/2026-05-14-tradingview-2-run-and-compare-charts.md`.
- Verify the existing run detail and compare Lightweight Charts surface against the plan.
- Add the missing `ChartContainer` data-table fallback contract.

Verification target:

- `corepack pnpm --dir frontend/web test -- RunChart`
- `corepack pnpm --dir frontend/web typecheck`

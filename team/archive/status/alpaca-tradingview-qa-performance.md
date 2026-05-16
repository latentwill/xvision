---
track: alpaca-tradingview-qa-performance
worktree: /root/deploy/xvision/.worktrees/alpaca-tradingview-qa-performance
branch: alpaca-tradingview-qa-performance
base: tradingview-4-live-wizard-preview
phase: verified
last_updated: 2026-05-14T08:20:49Z
owner: codex
---

# What changed

- Documented the scenario Backtest workflow and explicitly separated it from
  Alpaca paper mirror behavior.
- Added a current chart-surface list to `docs/dashboard.md` while preserving
  the archived status of the broader dashboard inventory.
- Updated F30 and F32 follow-up status text to point remaining work at stacked
  PR review/merge feedback and final QA/performance.

# Checkpoints

- `docs: document scenario and chart workflows`

# Verification

- `rg -n "Paper mirror|Backtest means|Current Chart Surfaces|2026-05-14" MANUAL.md docs/dashboard.md FOLLOWUPS.md`
- `git diff --check`

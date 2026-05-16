---
track: tradingview-4-live-wizard-preview
worktree: /root/deploy/xvision/.worktrees/tradingview-4-live-wizard-preview
branch: tradingview-4-live-wizard-preview
base: tradingview-3-scenario-strategy-charts
phase: verified
last_updated: 2026-05-14T08:18:18Z
owner: codex
---

# What changed

- Added LiveChart coverage for streamed `indicator_tail` SSE frames.
- Wired `useRunStream` to merge flat indicator tail updates such as `sma_20`
  and nested indicator aliases such as `bollinger.upper` / `macd_signal` into
  the live chart payload.
- Kept the live stream snapshot, follow, and run-change behavior covered by the
  existing LiveChart tests.

# Checkpoints

- `feat(web): merge live indicator tail events`

# Verification

- `corepack pnpm --dir frontend/web install --frozen-lockfile`
- `corepack pnpm --dir frontend/web test -- LiveChart`
- `corepack pnpm --dir frontend/web typecheck`
- `corepack pnpm --dir frontend/web build`
- `git diff --check`

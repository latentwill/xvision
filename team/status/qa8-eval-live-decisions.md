---
task: qa8-eval-live-decisions
owner: codex
phase: complete
last_updated: 2026-05-13T15:40:15Z
---

# What I'm Doing Right Now

Implemented live eval decision streaming in `.worktrees/qa8-eval-live-decisions`.

The existing eval run SSE channel now carries full `decision` events plus
running/terminal status events. Backtest and paper executors emit decisions
after persistence, and the run detail page merges those rows into the visible
decisions table while the run remains queued/running.

# Blocked On

nothing

# Next Up

- [x] Added focused frontend coverage for streamed decision rows.
- [x] Added `RunChartEvent::Decision` and dashboard SSE event-name mapping.
- [x] Emitted live decisions/status from backtest and paper executors.
- [x] Wired `/eval-runs/:id` to subscribe while active and show a streaming
  indicator.
- [x] `corepack pnpm --dir frontend/web test -- eval-runs-detail eval-runs`
- [x] `corepack pnpm --dir frontend/web typecheck`
- [x] `corepack pnpm --dir frontend/web test`
- [x] `corepack pnpm --dir frontend/web build`
- [x] `git diff --check`

Not run:

- Cargo commands are forbidden on this deploy host by `CLAUDE.md`; Rust
  compile/tests must run in CI/non-deploy.
- `rustfmt --check ...` could not run because `rustfmt` is not installed on
  this host.

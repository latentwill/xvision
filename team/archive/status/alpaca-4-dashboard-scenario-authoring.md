---
track: alpaca-4-dashboard-scenario-authoring
worktree: /root/deploy/xvision/.worktrees/alpaca-4-dashboard-scenario-authoring
branch: alpaca-4-dashboard-scenario-authoring
base: alpaca-3-scenario-registry-runner
phase: verified
last_updated: 2026-05-14T07:50:15Z
owner: codex
---

# What changed

- Added ScenarioForm coverage for unsupported granularity, reversed date
  windows, and advanced fees/slippage/latency payloads.
- Added form-level guards so invalid granularity text and reversed windows are
  blocked before the dashboard sends a create request.
- Preserved the existing expanded Alpaca granularity support used by the
  scenario API, list/detail routes, bar-cache flow, and eval launcher.

# Verification

- `corepack pnpm --dir frontend/web test -- ScenarioForm`
- `corepack pnpm --dir frontend/web test -- ScenarioForm scenarios-detail eval-runs`
- `corepack pnpm --dir frontend/web build`
- `git diff --check`

# Notes

- The route sweep also matched and ran `eval-runs-detail`; all selected
  frontend tests passed.
- The production build refreshed ignored static assets and removed
  `crates/xvision-dashboard/static/.gitkeep`; the `.gitkeep` deletion was
  restored before checkpointing.
- Rust checks were not run on this deploy host because `CLAUDE.md` forbids
  `cargo`, `cargo build`, `cargo check`, and `cargo test` here.

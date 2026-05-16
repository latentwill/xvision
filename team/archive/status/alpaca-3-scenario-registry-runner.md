---
track: alpaca-3-scenario-registry-runner
worktree: /root/deploy/xvision/.worktrees/alpaca-3-scenario-registry-runner
branch: alpaca-3-scenario-registry-runner
base: alpaca-2-bars-cache-cli
phase: implemented
last_updated: 2026-05-14T07:40:44Z
owner: codex
---

# What changed

- Added `Scenario::validate_v1` and `ScenarioValidationError` so DB-loaded,
  seeded, cloned, and API-created scenarios can share the same v1 envelope
  checks.
- Covered valid crypto scenarios plus unsupported assets, multi-asset
  scenarios, non-crypto/non-USD scenarios, and future windows in
  `scenario_shape`.
- Routed `api::scenario::create` through the domain validator before insert,
  while preserving the existing expanded Alpaca granularity support.

# Checkpoints

- `feat(engine): validate scenario v1 domain shape`

# Verification

- `git diff --check`

# Blocked on

- Rust tests were not run on this deploy host because `CLAUDE.md` forbids
  `cargo`, `cargo build`, `cargo check`, and `cargo test` here.
- `cargo` is also not installed on PATH in this shell.

# CI/non-deploy verification target

```bash
cargo test -p xvision-engine --test scenario_shape
cargo test -p xvision-engine --test scenario_api
cargo test -p xvision-engine --test eval_run_scenario
```

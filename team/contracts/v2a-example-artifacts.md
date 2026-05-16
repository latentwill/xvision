---
track: v2a-example-artifacts
lane: leaf
wave: v2a
worktree: .worktrees/v2a-example-artifacts
branch: task/v2a-example-artifacts
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-cli/src/commands/example/**
  - crates/xvision-cli/src/commands/mod.rs            # register `xvn example` only
  - data/examples/**
  - crates/xvision-engine/src/strategies/templates.rs # add example template ids
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
  - crates/xvision-dashboard/**
interfaces_used:
  - StrategyStore::create_from_template
  - ScenarioStore::upsert_local
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-cli example
  - cargo test -p xvision-engine strategies::templates
acceptance:
  - `xvn example seed --reset` populates a known set of example strategies, scenarios, and tutorial artifacts into the active XVN_HOME.
  - The seed is idempotent and labelled with `source=example` so existing operator data is not overwritten.
  - Example artifacts referenced by `v2a-driver-tour` are available.
---

# Scope

V2A item 3 from the action plan: produce a small, resettable set of example
strategies, scenarios, and tutorial artifacts so the Driver.js tour and the
in-app docs have something concrete to point at.

# Out of scope

- Live broker connections — examples run on backtest mode only.
- Anything that mints on-chain identity.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/v2a-example-artifacts -b task/v2a-example-artifacts origin/main
```

# Notes

- The `hackathon/sample-strategies` retained branch contains candidate
  examples — pull from there before designing fresh ones.

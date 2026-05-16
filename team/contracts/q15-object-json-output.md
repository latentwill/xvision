---
track: q15-object-json-output
lane: leaf
wave: q15
worktree: .worktrees/q15-object-json-output
branch: task/q15-object-json-output
base: origin/main
status: in-progress
depends_on:
  - q15-eval-json-export             # standardizes the per-object shape used here
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-cli/src/commands/strategy/get.rs
  - crates/xvision-cli/src/commands/scenario/get.rs
  - crates/xvision-cli/src/commands/agent/get.rs
  - crates/xvision-cli/src/json/object_shapes.rs       # may already exist; extend
  - crates/xvision-dashboard/src/routes/strategies/get.rs
  - crates/xvision-dashboard/src/routes/scenarios/get.rs
  - crates/xvision-dashboard/src/routes/agents/get.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/eval/**
  - frontend/web/**
interfaces_used:
  - StrategyStore::get
  - ScenarioStore::get
  - AgentStore::get
  - JsonObjectShape (defined by q15-eval-json-export)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-cli strategy::get::json
  - cargo test -p xvision-cli scenario::get::json
  - cargo test -p xvision-cli agent::get::json
  - cargo test -p xvision-dashboard strategies::get
  - cargo test -p xvision-dashboard scenarios::get
  - cargo test -p xvision-dashboard agents::get
acceptance:
  - `xvn strategy get <id> --format json` emits the full strategy object (manifest, AgentRefs, tags, risk config).
  - `xvn scenario get <id> --format json` emits the full scenario (asset, range, granularity, warmup_bars, fees, slippage, latency).
  - `xvn agent get <id> --format json` emits the full Agent (id, name, AgentSlots with resolved provider/model/max_tokens).
  - Each `--format json` output matches the shape used inside `EvalRunExport.strategy / scenario / agents`.
  - `GET /api/strategies/:id`, `/api/scenarios/:id`, `/api/agents/:id` return the same shape.
---

# Scope

Fix QA15 item 6 (non-eval half): standardize per-object JSON output so
scripted CLI workflows and the eval export agree on shape. Eval export
(`q15-eval-json-export`) defines the shared shape; this track wires the
three object getters to it.

# Out of scope

- List endpoints (`xvn strategy list --json` etc.). Add separately if a
  follow-up QA item requests it.
- Mutating endpoints (create/update). The shapes here are read-only.
- Eval JSON export itself (`q15-eval-json-export`).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/q15-object-json-output -b task/q15-object-json-output origin/main
```

# Notes

- Wait on `q15-eval-json-export` to land the shared `JsonObjectShape` trait
  before standardizing here, OR stack via `stacking: declared:q15-eval-json-export`.

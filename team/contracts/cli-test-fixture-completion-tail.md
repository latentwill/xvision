---
track: cli-test-fixture-completion-tail
lane: leaf
wave: cli-test-tech-debt-2026-05-22
worktree: .worktrees/cli-test-fixture-completion-tail
branch: task/cli-test-fixture-completion-tail
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-cli/tests/strategy_validate.rs
  - crates/xvision-cli/tests/eval_batch_run.rs
  - crates/xvision-cli/tests/experiment_run.rs
  - crates/xvision-cli/tests/strategy_cli.rs
  - crates/xvision-cli/tests/common/**
forbidden_paths:
  - crates/xvision-cli/src/**
  - crates/xvision-engine/**
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - xvision_cli::commands::strategy (post-template-registry-removal CLI verbs)
  - xvision_engine::strategies::store::FilesystemStore (existing strategy fixture builder)
  - xvision_engine::api::eval (eval-boundary validation that rejects legacy trader_slot-only strategies)
  - The post-2026-05-12 `Strategy { agents: Vec<AgentRef>, ... }` shape that requires at least one Agent bound
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-cli --test strategy_validate
  - cargo test -p xvision-cli --test eval_batch_run
  - cargo test -p xvision-cli --test experiment_run
  - cargo test -p xvision-cli --test strategy_cli
  - cargo test -p xvision-cli
acceptance:
  - `cargo test -p xvision-cli --test strategy_validate` — 0 failures (today: 4 failures from `--template` flag removal)
  - `cargo test -p xvision-cli --test eval_batch_run` — 0 failures (today: 5 failures from legacy `trader_slot`-only strategies being rejected at the eval boundary)
  - `cargo test -p xvision-cli --test experiment_run` — 0 failures (today: same shape as eval_batch_run)
  - `cargo test -p xvision-cli --test strategy_cli` — still passes (regression guard; this suite migrated successfully earlier and is the reference shape)
  - No `--template` references remain in any test arg list; replace with `--from-file <path>` invocations using JSON fixtures built inline (the pattern `strategy_cli.rs::write_strategy_file` already demonstrates)
  - No legacy `trader_slot`-only fixtures remain; each test that exercises an eval run must build a `Strategy { agents: vec![AgentRef { agent_id, role }], ... }` and persist a matching `Agent` (with at least one `AgentSlot`) to the store before launch
---

# Scope

The 2026-05-22 conductor-pass surfaced 3 CLI integration-test suites
failing on main:

| Suite | Failures | Root cause |
|---|---|---|
| `xvision-cli/tests/strategy_validate.rs` | 4 | Used `--template mean_reversion` flag, removed by PR #486 (template-registry-removal). Tests still expect it. |
| `xvision-cli/tests/eval_batch_run.rs` | 5 | Builds `Strategy { ..., agents: Vec::new(), trader_slot: Some(...) }` — legacy shape. Post-PR #443/#467 fixture migration the eval boundary rejects strategies without at least one bound `Agent`. |
| `xvision-cli/tests/experiment_run.rs` | (same shape as `eval_batch_run`) | Same root cause. |

`crates/xvision-cli/tests/strategy_cli.rs` already migrated to the
post-template-registry shape (uses `--from-file` + pre-built JSON
fixtures) and serves as the reference pattern.

This contract migrates the three broken suites to that shape.

# What changes

1. **Replace `--template <name>` invocations** in `strategy_validate.rs`
   with `--from-file <path>` invocations. The `write_strategy_file`
   helper in `strategy_cli.rs:38–43` shows the pattern: build a
   complete `Strategy` JSON on disk, then point the CLI at it.

2. **Replace legacy `trader_slot`-only fixtures** in
   `eval_batch_run.rs::save_test_strategy` and
   `experiment_run.rs::save_test_strategy` with the post-refactor
   `Strategy { agents: vec![AgentRef { agent_id, role }], ... }`
   shape. Each test must:
   - Create an `Agent` via `agents_api::create` (the
     `create_agent` helper in `strategy_validate.rs:93–132`
     demonstrates) with at least one valid `AgentSlot`
   - Bind that agent's `agent_id` into the strategy's
     `agents: Vec<AgentRef>`
   - Leave `trader_slot: None` or remove the field entirely if the
     post-refactor `Strategy` shape no longer requires it

3. **Audit for other legacy references**:
   - `required_models` / `model_requirement` field names (renamed to
     `attested_with` by PR #508)
   - `LLMSlot.prompt` field (removed by PR #515 if/when it lands —
     coordinate via timing)
   - `noop_skip: None` on `AgentSlot` literals (already added by
     #514 if it lands first; otherwise add inline)

# Out of scope

- Production-side CLI logic changes (`crates/xvision-cli/src/**` is
  forbidden — this is test-fixture migration only)
- Any new test cases beyond the 9 currently failing
- Engine-side changes (`crates/xvision-engine/**` forbidden)
- The follow-on contract `cli-test-helper-extraction` (a
  shared helper crate that pulls the common fixture-build logic
  out of all four tests) — defer to a separate track

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/cli-test-fixture-completion-tail -b task/cli-test-fixture-completion-tail origin/main
```

Set `CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"` to avoid polluting
the worktree's target tree.

# Iterative verification loop

```bash
cargo test -p xvision-cli --test strategy_validate 2>&1 | tee /tmp/sv.log
cargo test -p xvision-cli --test eval_batch_run    2>&1 | tee /tmp/ebr.log
cargo test -p xvision-cli --test experiment_run    2>&1 | tee /tmp/er.log
cargo test -p xvision-cli --test strategy_cli      2>&1 | tee /tmp/sc.log

# Until all four exit 0, iterate on the failing test's fixture
# construction. The exact match for the post-refactor Strategy shape
# is in strategy_cli.rs::build_mean_reversion.
```

# Notes

The pre-existing test failures were flagged 2026-05-22 by the
`strategy-slot-prompt-resolution` worker (PR #515) during full-
workspace verification. They predate this contract; the worker
reproduced them on `cf3d471` (the commit BEFORE its own PR) to
confirm they're not regressions from #515.

Should be straightforward — `strategy_cli.rs` is the reference and
already passes.

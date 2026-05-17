---
track: qa-execute-slot-cap
lane: foundation
wave: qa-2026-05-17
worktree: .worktrees/qa-execute-slot-cap
branch: task/qa-execute-slot-cap
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/execute.test.rs
  - crates/xvision-engine/tests/agent_execute_slot_cap.rs
forbidden_paths:
  - crates/xvision-engine/src/agent/pipeline.rs
  - crates/xvision-engine/src/eval/executor/**
  - crates/xvision-engine/migrations/**
  - frontend/**
interfaces_used:
  - "xvision_engine::agent::execute::execute_slot"
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build -p xvision-engine
  - cargo test -p xvision-engine agent::execute
  - cargo test -p xvision-engine --test agent_execute_slot_cap
acceptance:
  - "`execute_slot`'s tool-use loop terminates at a bounded iteration cap (default 8–12), with the cap exposed as a config knob or named constant"
  - "Cap exhaustion returns a typed error variant carrying: slot role, model id, list of tool names called in the loop, accumulated input/output token counts, and last stop reason"
  - Regression test: a fake intern that always emits a `ToolUse` block triggers the cap and the test asserts the error variant + payload fields
  - Existing happy-path tests for `execute_slot` (EndTurn / MaxTokens / single tool round-trip) still pass
  - No callers (engine pipeline, eval executors) regress on type signature — error propagates via existing `Result` paths
---

# Scope

Implements remediation step 1 of `qa/2026-05-17-comprehensive-codebase-review.md`
("Engine slot tool-use loop can run forever"). Adds a hard iteration cap to
`execute_slot` so a pathological intern/trader/regime response cannot wedge
a backtest/paper run or burn unbounded upstream LLM and tool budget.

Cap value: pick a conservative default (8–12) and expose it either as a
named constant or a struct field on whatever options/config struct
`execute_slot` already accepts. Prefer not introducing a new settings
surface in this track — operators can override later if needed.

# Out of scope

- Wall-clock timeouts (the recommended fix focuses on iteration cap;
  defer wall-clock to a future track if needed).
- Changes to `agent/pipeline.rs`, eval executors, or any caller. The error
  must propagate through existing `Result` paths.
- Refactoring how tool-call dispatch or content-block accumulation works
  inside `execute_slot` — only add the bounded counter and exhaustion
  branch.
- New configuration surfaces in the engine settings API. If a runtime knob
  is needed, expose it as a `const` or a field on an existing config type
  passed into `execute_slot`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-execute-slot-cap \
  -b task/qa-execute-slot-cap origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
git -C .worktrees/qa-execute-slot-cap status
```

# Notes

Implementation hints (do not rewrite the contract — use as starting points):

- The dashboard wizard already has `MAX_TOOL_LOOP_ITERATIONS` (referenced
  in the QA finding). The engine cap is a sibling concept, not a shared
  constant — keep them independent.
- The error variant should be `tracing::warn!`-friendly so a wedged run
  shows up in agent-run observability events (Phase B will plumb these
  through `RunEventBus`, but this track does not need to emit observability
  events directly).
- If `execute_slot` currently returns `anyhow::Error`, prefer adding a
  concrete `ExecuteSlotError::ToolLoopCapExceeded { ... }` variant on a
  local error enum and converting at the boundary, so the typed payload is
  preserved for the regression test.

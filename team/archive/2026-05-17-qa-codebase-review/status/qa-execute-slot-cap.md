---
track: qa-execute-slot-cap
status: pr-open
last_update: 2026-05-17
worker: Claude (xvision conductor session — picked up after a prior background worker hit org usage limit before producing commits)
pr: 236
base_branch: task/qa-remove-agent-max-tokens
commits:
  - b77da13 — qa: bound execute_slot tool-use loop with iteration cap + typed error
---

## Outcome

PR #236 open, stacked on PR #223 (`task/qa-remove-agent-max-tokens`).
Rebase to `main` once #223 merges.

## What changed

- `crates/xvision-engine/src/agent/execute.rs`:
  - New `MAX_TOOL_LOOP_ITERATIONS = 12` constant.
  - New `ExecuteSlotError::ToolLoopCapExceeded` variant via
    `thiserror` carrying role + model + iterations + tool_names +
    input_tokens + output_tokens + last_stop_reason.
  - Per-iteration cap check fires BEFORE the dispatch call.
  - `tracing::warn!` emits the payload on exhaustion.
- `crates/xvision-engine/tests/agent_execute_slot_cap.rs`: new
  integration test exercising the runaway loop + asserting all 7
  payload fields.

## Verification

| Command | Result |
|---|---|
| `cargo build -p xvision-engine` | clean (only pre-existing dead-code warnings in `api/eval.rs`, unrelated) |
| `cargo test -p xvision-engine --test agent_execute_slot_cap` | 1/1 pass |
| `cargo test -p xvision-engine --lib agent::execute` | 4/4 pass (existing happy-path tests still green) |
| `cargo test -p xvision-engine --lib agent::` | 14/14 pass |
| `cargo test -p xvision-engine --test pipeline_inline` | 4/4 pass (no caller-side regression) |

## Out-of-scope honored

- No wall-clock timeout — contract scoped to iteration cap only.
- No changes to `agent/pipeline.rs`, eval executors, or any caller —
  error propagates through existing `Result<_, anyhow::Error>` paths.
- No new engine settings surface; cap is exposed as a public const so
  callers can override later if needed.
- Stacked-on-#223 means the `max_tokens` `None`-dropping behavior is
  preserved as the base; this commit is additive to it.

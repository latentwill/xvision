# harness-recovery-state-machine — status

**Contract:** [team/contracts/harness-recovery-state-machine.md](../contracts/harness-recovery-state-machine.md)
**Branch:** `task/harness-recovery-state-machine`
**Worktree:** `.worktrees/harness-recovery-state-machine`
**Base:** `origin/task/harness-span-taxonomy-extension` (F-4)
**Stacked on:** F-4 (`harness-span-taxonomy-extension`, claimed). When
F-4 lands, rebase to `origin/main`.

## Owner

`latentwill` — claimed 2026-05-18 by operator direction ("work F5 and add
the non-broker errors"). The deferred `agent-error-feedback-non-broker-errors`
follow-up is folded into this contract.

## Plan

1. Sync worktree off `origin/task/harness-span-taxonomy-extension` so the
   `RecoveryAttempt` SpanKind variant compiles. If F-4 hasn't published the
   variant yet (it's currently uncommitted in the F-4 worktree), the F-5
   worktree adds a temporary local declaration that mirrors F-4's wire
   identifier — the rebase onto F-4 head deletes that shim before the PR
   opens.
2. New file `crates/xvision-engine/src/eval/executor/recovery.rs`:
   - `FailureClass` enum with seven variants + structured payloads.
   - `RecoveryDispatcher` with the six bounded playbooks + per-class const
     thresholds (`MAX_DECODE_REPAIR_PROMPTS`, `MAX_TOOL_RETRIES`, etc.).
   - `FailureClass::tag()` mapping back to the legacy `&'static str`
     for wire-format compatibility.
3. Rewrite `classify_run_failure` (eval/executor/mod.rs:48) to return
   `FailureClass`. Keep the existing string-match cases; add the three new
   ones (`empty_data`, `context_overflow`, `repeated_tool_failure`).
4. Wire dispatcher invocation into per-cycle error paths in `paper.rs` and
   `backtest.rs` so recoverable classes get a chance to retry/feedback
   before `format_failure_reason` terminates the run.
5. Plumb `MalformedJson` recovery into `agent/llm.rs` (the existing
   `RESPONSE_DECODE_RETRIES = 1` becomes the typed `MAX_DECODE_REPAIR_PROMPTS`)
   and `RepeatedToolFailure` into `agent/execute.rs` (per-cycle
   `HashMap<(tool, input_hash), u8>`).
6. Add `ObsEmitter::emit_recovery_attempt(span_id, parent, class, outcome,
   attempt)` to `agent/observability.rs`.
7. Tests in `crates/xvision-engine/tests/agent_recovery.rs` covering the
   eight acceptance cases.
8. `cargo build --workspace && cargo test -p xvision-engine && cargo
   clippy -p xvision-engine -- -D warnings`.
9. Open PR against `task/harness-span-taxonomy-extension` (stacked).

## Risks / Open Questions

- **F-4 not yet pushed.** The current F-4 worktree has uncommitted changes
  to `crates/xvision-observability/src/types.rs`. Until F-4 commits and
  pushes, the `RecoveryAttempt` variant doesn't exist on
  `origin/task/harness-span-taxonomy-extension`. Plan: open the worktree
  against the F-4 worktree's local HEAD if it's pushed, otherwise add a
  temporary stub variant in this branch's commit and resolve at rebase.
- **`ContextOverflow` cheap-model dispatch** — defaulting to the slot's
  current model with reduced max_tokens is the safe path. A dedicated
  summarize model in `settings.toml` is a follow-up if operators ask.
- **`EmptyData` failure-class wire-tag rename.** Today `MarketSnapshot`
  with `recent_bars.is_empty()` may surface as `unclassified` in the
  failure reason; the new tag is `empty_data`. Downstream consumers
  parsing the `[<class>]` prefix may need to learn the new tag.
  Mitigation: `[unclassified]` remains the catch-all; only snapshots
  with an explicit empty-bars cause graduate to `[empty_data]`. No
  existing assertion in the codebase depends on this exact mapping.

## Checkpoints

_(append as work lands)_

- 2026-05-18 — Contract drafted; board updated; worktree pending.

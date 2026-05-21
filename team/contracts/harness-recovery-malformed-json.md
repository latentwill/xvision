---
track: harness-recovery-malformed-json
lane: integration
wave: harness-observability-tail-2026-05-21
worktree: .worktrees/harness-recovery-malformed-json
branch: task/harness-recovery-malformed-json
base: origin/main
status: deferred
depends_on:
  - harness-recovery-state-machine
blocks: []
stacking: declared:harness-recovery-state-machine
allowed_paths:
  - crates/xvision-engine/src/agent/recovery.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/tests/agent_recovery_malformed_json.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-core/migrations/**
  - crates/xvision-observability/src/types.rs
  - crates/xvision-engine/src/agent/observability.rs
  - frontend/web/**
interfaces_used:
  - agent::recovery::FailureClass (TraderInvalidJson, TraderTruncated)
  - agent::recovery::RecoveryFamily::MalformedJson
  - eval::executor::trader_output::TraderOutput::parse_response (current call site at paper.rs:918)
  - eval::executor::trader_output::TraderOutputError (carries kind + raw_excerpt + detail)
  - agent::pipeline / agent::execute re-call seam for the trader slot
  - ObsEmitter::emit_recovery_attempt / emit_recovery_failed (already exists from F-5 phase 1)
parallel_safe: false
parallel_conflicts:
  - "Single-writer on `crates/xvision-engine/src/eval/executor/paper.rs` with `eval-broker-error-circuit-breaker`, `agent-error-feedback-self-healing`, `executor-trait-extraction`. Coordinate via team/MANIFEST.md before claim."
  - "Single-writer on `crates/xvision-engine/src/agent/recovery.rs` with `harness-recovery-schema-missing-field` and `harness-recovery-context-overflow` (sibling phase-2 contracts). Sequence: MalformedJson → SchemaMissingField → ContextOverflow."
verification:
  - cargo test -p xvision-engine agent_recovery_malformed_json
  - cargo test -p xvision-engine --lib agent::recovery
  - cargo test -p xvision-engine --lib eval::executor
  - cargo clippy -p xvision-engine -- -D warnings
  - bash scripts/board-lint.sh
acceptance:
  - **Source spec:** F-5 phase-2 follow-up filed by the F-5 phase-1 PR. The audit text lives in `team/intake/archive/2026-05-18-harness-observability-audit.md` (F-5 section bullet on MalformedJson). The phase-1 contract `harness-recovery-state-machine.md` Notes section reserves this work explicitly.
  - **Seam:** `eval/executor/paper.rs` at the `TraderOutput::parse_response(...)?` call (currently ~line 918). When `TraderOutputError.kind` is `InvalidJson` or `Truncated`, do not propagate immediately — instead invoke `recovery::Policy::malformed_json_repair_attempt(...)` which re-calls the trader slot ONCE with a feedback message injected.
  - **Feedback message shape:** append a `Message { role: "user", content: ContentBlock::Text { text: ... } }` to the conversation log of the trader slot's `LlmRequest`, carrying:
    - the verbatim parse error from `TraderOutputError.detail`
    - the response_schema descriptor (so the model is reminded what it should have emitted)
    - a one-line instruction: "Your previous response failed to parse. Emit a single JSON object matching the schema; do not include prose, code fences, or tool calls."
    Implementation choice: the message body construction lives in `agent/recovery.rs` (a `pub fn build_malformed_json_repair_message(parse_error: &str, schema: &ResponseSchema) -> String`); the call site in `paper.rs` only owns the dispatch + re-parse.
  - **Bounded retry:** ONE repair attempt. If the second response also fails to parse, propagate the original `TraderOutputError` (do not stack the second error on top — the operator wants the first failure as the surfacing class). `eval_runs.error` ends up with `[invalid_json]` or `[truncated]` exactly as today.
  - **Span emission:** the repair dispatch emits `recovery.attempt` with `class_tag = "invalid_json"` (or `"truncated"`) and `retry_count = 1` via `ObsEmitter::emit_recovery_attempt`. If the retry also fails, emit `recovery.failed` with the final error message. Reuses the existing F-5 emit methods — no observability changes.
  - **No schema or migration changes.** The conversation log lives in-memory inside `execute_slot`; the feedback message is a synthetic turn that does not persist anywhere beyond the existing trace (the model_call spans + payload blobs already capture it).
  - **Backtest seam:** `eval/executor/backtest.rs` calls `TraderOutput::parse_response` too. The repair logic must be shared between paper and backtest — factor it into a helper in `eval/executor/trader_output.rs` (or a new sibling in `eval/executor/recovery.rs`) so both call sites converge.
  - **Out of scope:**
    - Recovery for `TraderMissingField` / `TraderInvalidField` (handled by `harness-recovery-schema-missing-field`).
    - Recovery for `EmptyText` / `ToolUseOnly` / `MissingResponse` — those are EmptyData family per the F-5 taxonomy; the audit's policy is "emit `data_availability_failure` and stop the cycle". A separate contract `harness-recovery-empty-data` is not currently scheduled; the existing behaviour (propagate the error) is acceptable until operator asks otherwise.
    - Changing the trader response shape or the `TraderOutput` struct.
    - The full pipeline self-healing — `agent-error-feedback-self-healing` already does this for broker errors via the tool_result is_error path; F-5 phase-2 mirrors the pattern for parse errors.
  - **Tests required:**
    - `tests/agent_recovery_malformed_json.rs`:
      - 1st-call returns unparseable text, 2nd-call (after repair) returns valid JSON → run completes; exactly one `recovery.attempt` span emitted with `class_tag="invalid_json"`.
      - 1st-call truncated, 2nd-call also truncated → original `[truncated]` error surfaces; one `recovery.attempt` (Ok) + one `recovery.failed`.
      - Repair message body asserts: contains parse error verbatim, contains schema-name hint, contains the no-prose-no-fences instruction.
    - Update `tests/agent_recovery_state_machine.rs` `classify_run_failure_adapter_preserves_wire_tags` to confirm `[invalid_json]` and `[truncated]` still surface for the unrecoverable case (post-second-attempt failure).
  - **Wire-shape stability:** `eval_runs.error` `[invalid_json]` / `[truncated]` prefix is unchanged. Adding the repair retry must not require a UI/review-consumer change.

# Scope

Phase 2 of `harness-observability-tail-2026-05-21`. F-5 phase 1 wired
the typed dispatcher + repeated-tool block; phase 2 picks off the
recovery families one at a time. MalformedJson is first because it's
the most common trader-output failure (invalid_json + truncated) and
the recovery is conceptually simple: dispatch a repair message,
re-parse, propagate the original error if it fails again.

# Out of scope

- Recovery for SchemaMissingField (separate contract).
- Recovery for ContextOverflow (separate contract).
- Recovery for EmptyText / ToolUseOnly / MissingResponse (EmptyData
  family — not currently scheduled).
- Changing the trader response shape, the `TraderOutput` struct, or
  the response_schema definitions.
- Schema migrations or observability wire changes.
- The full pipeline self-healing pattern — that's
  `agent-error-feedback-self-healing` territory and is already shipped
  for broker errors. This contract mirrors the pattern for parse errors.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-recovery-malformed-json status
git -C .worktrees/harness-recovery-malformed-json log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/harness-recovery-malformed-json
#   - base is up to date with origin/main (or rebase planned)
#   - F-5 phase-1 PR #499 is MERGED — this contract depends on it
#   - executor/paper.rs single-writer status: read team/MANIFEST.md
#     before editing; coordinate with active executor / broker tracks
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-recovery-malformed-json -b task/harness-recovery-malformed-json origin/main
```

# Notes

Status `blocked` until PR #499 (`harness-recovery-state-machine`
phase 1) merges to main, since the FailureClass / RecoveryFamily /
emit_recovery_* surface ships in that PR.

Sequencing the three phase-2 contracts as MalformedJson →
SchemaMissingField → ContextOverflow lets each land cleanly without
re-touching `agent/recovery.rs` mid-flight.

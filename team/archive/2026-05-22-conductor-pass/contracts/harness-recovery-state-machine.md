---
track: harness-recovery-state-machine
lane: integration
wave: harness-observability-tail-2026-05-21
worktree: .worktrees/harness-recovery-state-machine
branch: task/harness-recovery-state-machine
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/recovery.rs
  - crates/xvision-engine/src/agent/mod.rs
  - crates/xvision-engine/src/agent/llm.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/observability.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/tests/agent_recovery_state_machine.rs
  - crates/xvision-engine/tests/agent_span_taxonomy.rs
forbidden_paths:
  - crates/xvision-execution/src/broker_surface.rs
  - crates/xvision-execution/src/alpaca.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/trader_output.rs
  - crates/xvision-engine/migrations/**
  - crates/xvision-core/migrations/**
  - crates/xvision-observability/src/types.rs
  - frontend/web/**
interfaces_used:
  - eval::executor::classify_run_failure (extended with a typed dispatch front-end; existing string return preserved for downstream consumers)
  - observability::ObsEmitter (new emit_recovery_attempt / emit_recovery_failed methods on the existing emitter; SpanKind variants already exist from F-4)
  - agent::llm::RESPONSE_DECODE_RETRIES (replaced by typed dispatch)
  - agent::execute::MAX_TOOL_LOOP_ITERATIONS (kept as outer hard cap)
parallel_safe: false
parallel_conflicts:
  - "`crates/xvision-engine/src/eval/executor/mod.rs` is single-writer with `agent-error-feedback-self-healing`, `eval-broker-error-circuit-breaker`, `executor-trait-extraction`. Coordinate via team/MANIFEST.md before claim. The broker classifier those tracks own stays untouched — F-5 wraps it as one branch of the new dispatcher, not a replacement."
  - "`crates/xvision-engine/src/agent/execute.rs` overlaps with `harness-prompt-hash-real-digest` (merged) and `agent-error-feedback-self-healing`. Re-rebase against origin/main before claim."
verification:
  - cargo test -p xvision-engine agent_recovery_state_machine
  - cargo test -p xvision-engine agent_span_taxonomy
  - cargo test -p xvision-engine
  - cargo clippy -p xvision-engine -- -D warnings
  - bash scripts/board-lint.sh
acceptance:
  - **Source spec:** the archived intake `team/intake/archive/2026-05-18-harness-observability-audit.md` (F-5 section, lines 122-148). The other six findings already landed; this is the remaining tail.
  - **New typed enum** `FailureClass { MalformedJson, ToolTimeout, SchemaMissingField, EmptyData, ContextOverflow, RepeatedToolFailure, Unrecoverable }` in a new module `crates/xvision-engine/src/agent/recovery.rs`. Each variant carries the structured fields its recovery policy needs (e.g. `MalformedJson { parse_error: String }`, `SchemaMissingField { missing: Vec<String> }`, `RepeatedToolFailure { tool_name: String, input_hash: String }`).
  - **Dispatcher front-end:** `recovery::classify(&anyhow::Error) -> FailureClass` is the typed front door. It walks the error chain (same `for cause in err.chain()` pattern as `eval::executor::classify_run_failure`) looking for typed downcasts FIRST (TraderOutputError, OpenAiCompatError, BrokerErrorDetail) and falls back to the existing string-matcher for the residual cases. The existing `classify_run_failure(&anyhow::Error) -> &'static str` is preserved as a thin adapter `FailureClass::tag()` so `eval_runs.error` `[<class>]` prefixes do not change shape — downstream review/UI consumers must not break.
  - **Per-class recovery policy** in `recovery::Policy::apply(...)`:
    - `MalformedJson` → repair-prompt the model once with the parse error injected, then fail closed. Replaces the bare `RESPONSE_DECODE_RETRIES=1` constant at `agent/llm.rs:117`.
    - `ToolTimeout` → retry the same tool once with backoff (200ms → 400ms cap). On second failure surface as `ToolCallFailed` to the agent (existing self-healing path from `agent-error-feedback-self-healing`) and emit `recovery.failed`.
    - `SchemaMissingField` → targeted patch prompt that asks for only the missing fields, not the whole response. Once. Then fail.
    - `EmptyData` → emit `data_availability_failure` and stop the cycle (no agent retry).
    - `ContextOverflow` → dispatch a cheap-model summarize call against the conversation history, retry once. Hard cap: summarize budget ≤ 2000 input tokens.
    - `RepeatedToolFailure` → block the exact `(tool_name, input_hash)` pair for the rest of the run. Counter lives in pipeline scope and resets on next cycle. Implementation: a `HashMap<(String, String), u8>` threaded through the existing execute loop; not a new struct on the agent.
    - `Unrecoverable` → no recovery, propagate error.
  - **Bounded loops:** every policy that retries has a per-class max count baked in (no runtime-configurable knob in this contract — defaults locked in code). The outer `MAX_TOOL_LOOP_ITERATIONS=12` hard cap stays as the safety net.
  - **Span emission:** add `ObsEmitter::emit_recovery_attempt(span_id, parent_span_id, class_tag, retry_count)` and `emit_recovery_failed(span_id, parent_span_id, class_tag, final_error)` to `agent/observability.rs`. SpanKind variants already exist from F-4 (`SpanKind::RecoveryAttempt`); reuse them. Each `recovery.attempt` span carries `class_tag` and `retry_count` in `attributes_json` per the F-2 SpanAttributes shape. Test `agent_span_taxonomy.rs` updated to assert the spans now emit (today it asserts the wire identifier is reserved but not emitted — see test line 188 comment).
  - **Existing broker-error path is NOT rewritten.** `agent-error-feedback-self-healing` (PR landed) owns broker `recoverable`/`fatal` split and the per-cycle structured-diagnostic injection. F-5's `RepeatedToolFailure` policy delegates to that path; do not re-implement it. The `eval-broker-error-circuit-breaker`'s `repeated_broker_error` class is a special case of `RepeatedToolFailure` — confirm it still surfaces under the same `[repeated_broker_error]` wire tag.
  - **Out of scope:**
    - New SpanKind variants (F-4 already added the four). F-5 only adds emit methods.
    - Frontend changes — `recovery.attempt` spans already flow through the trace dock end-to-end (see F-7 wiring); no UI work needed in this contract.
    - Migration / new tables — recovery state is in-process only, no persistence beyond the existing spans table.
    - `context.assemble` / `prompt.render` spans (audit nice-to-haves not adopted in F-4).
    - Replacing the existing broker error classifier from `agent-error-feedback-self-healing`.
  - **Tests required:**
    - `tests/agent_recovery_state_machine.rs` — one test per `FailureClass` variant asserting: (a) the right policy fires, (b) `recovery.attempt` spans are emitted with the expected `class_tag` + `retry_count`, (c) the per-class retry count is bounded, (d) success on first retry exits cleanly, (e) failure after exhausted budget emits `recovery.failed` and surfaces the typed error.
    - `tests/agent_span_taxonomy.rs` — flip the F-4 assertion at line 188 from "reserved but not emitted" to "emitted under the documented conditions". Update the comment.
    - End-to-end: at least one test runs an agent loop that hits `MalformedJson` → repair → success, asserts the trace contains exactly one `recovery.attempt` span with `retry_count=1` and no `recovery.failed`.
  - **Wire-shape stability:** `eval_runs.error` `[<class>] <message>` format is unchanged. The class tag strings are the same as today (`invalid_json`, `provider_timeout`, etc.) — `FailureClass::tag()` returns them. Adding a typed front door must not require a UI/review-consumer change to read existing fields.

# Scope

The 2026-05-18 harness observability audit (F-5) called out that recovery
in the agent loop is essentially absent: one JSON-decode retry, one
tool-use loop iteration cap, and `classify_run_failure` doing
regex-on-error-string *after* a run has already terminated. Three of
the seven audit findings (F-3 prompt_version, F-4 span taxonomy, F-7
trace-dock toggle) landed silently across previous waves; one (F-1
prompt hash) merged via PR #277; two (F-2 attrs, F-6 typed params)
merged via #294/#302. F-5 is the remaining tail.

This contract promotes the post-hoc string classifier to a typed
pre-recovery dispatcher with bounded per-class policies. The wire
shape of `eval_runs.error` and the existing self-healing broker path
are preserved; F-5 wraps them, not replaces them.

# Out of scope

- New SpanKind variants (F-4 already reserved `tool.validate_input/output`,
  `recovery.attempt`, `state.transition`). F-5 adds emit methods only.
- Frontend trace-dock changes. The Simple/Advanced toggle (F-7) already
  surfaces recovery.attempt spans correctly in both modes; no UI work.
- DB migration. Recovery state lives in-process; the existing spans
  table is the only persistence touched.
- Broker-error recoverable/fatal classification — owned by
  `agent-error-feedback-self-healing`. F-5 calls into it.
- `context.assemble` / `prompt.render` spans (audit nice-to-haves, not
  adopted in F-4).
- Replacing the regex-string classifier wholesale. It stays as the
  fallback inside the typed dispatcher for the residual cases where
  typed downcast doesn't match.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-recovery-state-machine status
git -C .worktrees/harness-recovery-state-machine log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/harness-recovery-state-machine
#   - base is up to date with origin/main (or rebase planned)
#   - executor/mod.rs single-writer status: read team/MANIFEST.md before
#     editing classify_run_failure; coordinate with active executor +
#     self-healing tracks if either is mid-flight
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-recovery-state-machine -b task/harness-recovery-state-machine origin/main
```

# Notes

Source intake: `team/intake/archive/2026-05-18-harness-observability-audit.md`
(F-5 section). Re-verify `classify_run_failure` line numbers before
implementation — `eval/executor/mod.rs` has churned since the audit.

Adjacent track to coordinate with: `agent-error-feedback-self-healing`
(status doc shows it's PR-open as of 2026-05-18). Its broker-error
recoverable/fatal split should land first if there's any sequencing
overlap; F-5's `RepeatedToolFailure` policy calls into that path.

`emit_recovery_attempt` does not exist today — confirmed via grep
against `crates/xvision-engine/src/agent/observability.rs`. F-4 added
the SpanKind variant but did not add the emit method, intentionally
deferring to F-5.

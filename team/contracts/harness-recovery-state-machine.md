---
track: harness-recovery-state-machine
lane: integration
wave: harness-observability-audit
worktree: .worktrees/harness-recovery-state-machine
branch: task/harness-recovery-state-machine
base: origin/task/harness-span-taxonomy-extension   # stacked on F-4 (PR #297); rebases to origin/main when F-4 merges
status: pr-open
depends_on:
  - harness-span-taxonomy-extension   # F-4 — adds `RecoveryAttempt` SpanKind variant + `state.transition` emission helper
blocks: []
stacking: declared:harness-span-taxonomy-extension
allowed_paths:
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/recovery.rs           # new file — typed FailureClass + dispatcher
  - crates/xvision-engine/src/agent/llm.rs                        # MalformedJson / ContextOverflow seams
  - crates/xvision-engine/src/agent/execute.rs                    # RepeatedToolFailure tracker, ToolTimeout handling
  - crates/xvision-engine/src/agent/observability.rs              # `emit_recovery_attempt` helper
  - crates/xvision-engine/tests/agent_recovery.rs                 # new integration tests
  - team/contracts/harness-recovery-state-machine.md
  - team/status/harness-recovery-state-machine.md
  - team/board.md
allowed_paths_stacking_shim:
  - crates/xvision-observability/src/types.rs           # F-4 owns SpanKind. F-5 adds RecoveryAttempt (and StateTransition for the EmptyData terminal) only as a stacking shim while F-4 is unpushed. Identical wire ids — F-4 rebase resolves trivially.
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/strategies/**             # F-6 owns mechanical_params typing
  - crates/xvision-observability/migrations/**
  - crates/xvision-execution/**                         # broker classification already typed in PR #286
  - frontend/web/**
  - crates/xvision-dashboard/**
interfaces_used:
  - xvision_observability::SpanKind::RecoveryAttempt    # reserved by F-4
  - xvision_observability::SpanKind::StateTransition    # for terminal `EmptyData` data-availability stop
  - xvision_engine::agent::observability::ObsEmitter
  - xvision_engine::agent::llm::{LlmDispatch, LlmResponse, ContentBlock}
  - xvision_engine::agent::execute::{execute_slot, ExecuteSlotError, MAX_TOOL_LOOP_ITERATIONS}
  - xvision_engine::eval::executor::trader_output::{TraderFailureKind, TraderOutputError}
  - xvision_execution::broker_surface::BrokerErrorClass   # read-only — mapped into FailureClass::Recoverable*
parallel_safe: true   # safe alongside F-3 (migrations), F-6 (strategies), F-7 (frontend) — disjoint files
parallel_conflicts:
  - harness-span-taxonomy-extension   # stacked, not conflicting — rebase on F-4 merge
verification:
  - cargo test -p xvision-engine -- recovery agent eval::executor
  - cargo test -p xvision-engine --test agent_recovery
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo build --workspace
acceptance:
  - New typed enum `FailureClass` in `crates/xvision-engine/src/eval/executor/recovery.rs`
    with variants `MalformedJson`, `ToolTimeout`, `SchemaMissingField`, `EmptyData`,
    `ContextOverflow`, `RepeatedToolFailure`, `Unrecoverable`. Each carries a
    structured payload (e.g. `MalformedJson { parse_error: String, raw_excerpt:
    String }`) rather than just the class tag, so the recovery dispatcher has
    something to feed back to the agent.
  - `classify_run_failure` in `eval/executor/mod.rs` is rewritten to return
    `FailureClass`. The legacy `&'static str` tag is preserved as
    `FailureClass::tag()` so the persisted `[<class>]` wire shape downstream
    consumers parse stays byte-identical. **No persisted-string regression.**
    The existing unit tests in `eval/executor/mod.rs` continue to pass via the
    `.tag()` mapping (rewritten to assert the new enum variant maps to the
    legacy tag).
  - Typed dispatcher `RecoveryDispatcher` in `recovery.rs` owns the per-variant
    playbook. Each playbook is a small `async fn` that takes the failure
    payload + a recovery context (run_id, cycle index, dispatch handle, tool
    registry, obs emitter) and returns `RecoveryOutcome::{Continue, Stop,
    Surfaced}`. The six playbooks:
      - `MalformedJson` → repair-prompt the model once with the parse error
        inline in the user turn, then fail closed (`Stop`) on second failure.
        Bounded count: `MAX_DECODE_REPAIR_PROMPTS = 1`.
      - `ToolTimeout` → retry the same tool call once with constant backoff
        (250ms). On second failure, surface as `ContentBlock::ToolResult { is_error: true }`
        to the agent (let it self-heal) and emit `recovery.attempt` with
        outcome=`surfaced`. Bounded count: `MAX_TOOL_RETRIES = 1`.
      - `SchemaMissingField` → targeted patch prompt that only names the
        missing fields (NOT the whole response repair). Bounded count: 1.
      - `EmptyData` → emit `state.transition` (Running → DataAvailabilityFailed)
        and stop the cycle (`Stop`). No retry. The eval-run completes with the
        existing failure path, but the persisted `[<class>]` tag is the new
        `empty_data` (alias for legacy `unclassified` when the root cause is a
        snapshot with zero `recent_bars`).
      - `ContextOverflow` → summarize the conversation history via a cheap-model
        dispatch (env: `XVISION_RECOVERY_SUMMARIZE_MODEL`, defaults to the
        current slot's effective model with a flag), retry the original call
        once. Hard cap on summarize budget (`MAX_SUMMARIZE_INPUT_TOKENS = 4096`).
      - `RepeatedToolFailure` → block the exact `(tool_name, input_hash)` pair
        for the rest of the run. Counter lives in pipeline scope (a
        `HashMap<(String, u64), u8>` in `execute_slot`'s caller). Resets per
        cycle. Threshold: 2 identical failures within a single cycle.
  - Every recovery transition emits a `SpanKind::RecoveryAttempt` span via
    `ObsEmitter::emit_recovery_attempt(span_id, parent, class, outcome)`. The
    `attributes_json` carries `{"failure_class": "<tag>", "outcome":
    "<continue|stop|surfaced>", "attempt": <n>}` merged with the F-2
    `SpanAttributes` bag (`run_id` populated). Recoverable transitions also
    update the parent span's `severity` to `warn` (mirroring the broker.call
    pattern from PR #286) so the trace dock renders them distinctly.
  - **Folds in `agent-error-feedback-non-broker-errors`** (was deferred from
    PR #286): the recoverable/fatal split previously limited to broker errors
    now extends to:
      - **Risk-engine rejections** → already a non-fatal `RiskDecision`
        variant; verify the trace path records a `risk.gate` span outcome
        without terminating the run. No new playbook (risk is already
        recovery-by-design — the agent re-decides next bar).
      - **Model-call failures** → `provider_timeout` → `ToolTimeout` playbook
        (one retry then surface); `provider_decode` / `error decoding response
        body` → `MalformedJson` playbook (was the operator's
        `[unclassified] error decoding response body` repro, see PR #242
        history); context-window overrun (HTTP 400 with `context_length`
        anywhere in the message) → `ContextOverflow`.
      - **Data-fetch gaps** → `MarketSnapshot` with `recent_bars.is_empty()`
        OR `intern.brief()` returning a structured `data_availability_failure`
        → `EmptyData` playbook.
  - **Broker errors are NOT re-classified.** PR #286's
    `BrokerErrorClass::Recoverable*` is the source of truth for the broker
    boundary; F-5 maps `BrokerErrorClass::Recoverable*` into
    `FailureClass::Unrecoverable` (passes through unchanged — the broker layer
    already self-heals) and `BrokerErrorClass::Fatal*` to
    `FailureClass::Unrecoverable` (terminates as today). The dispatcher
    short-circuits to the existing broker path. **No behaviour change for
    broker errors.**
  - Integration tests in `crates/xvision-engine/tests/agent_recovery.rs`:
      - `malformed_json_repair_prompts_once_then_fails_closed` — stub
        `LlmDispatch` returns garbage JSON twice; assert the dispatcher
        repair-prompts once (second LLM call message inspectable) then stops
        with `FailureClass::MalformedJson`. Trace has exactly one
        `recovery.attempt` span with outcome `stop`.
      - `tool_timeout_retries_once_then_surfaces` — stub `ToolRegistry`
        returns timeout twice; assert one retry, then a `ToolResult { is_error: true }`
        block is delivered as the next user turn. Trace has one
        `recovery.attempt` span with outcome `surfaced`.
      - `schema_missing_field_targeted_patch` — first response missing
        `confidence`; second response (after patch prompt) provides it; run
        continues. Patch prompt body contains only `confidence`, not the
        whole schema.
      - `empty_data_stops_cycle_cleanly` — `MarketSnapshot.recent_bars = vec![]`;
        run does not panic, does not retry, emits `state.transition`
        (Running → DataAvailabilityFailed), persists `[empty_data]` reason.
      - `repeated_tool_failure_blocks_pair_for_rest_of_run` — same
        `(tool_name, input_hash)` fails twice in one cycle; third call from
        the same input is blocked at the registry boundary with a
        `ToolResult { is_error: true, content: "tool blocked for remainder of
        run" }`. Block clears at next-cycle boundary.
      - `recovery_attempt_spans_carry_failure_class` — every playbook
        invocation emits a `recovery.attempt` span whose `attributes_json`
        round-trips through `serde_json` with the documented shape.
      - `legacy_failure_tag_unchanged` — round-trip every variant through
        `FailureClass::tag()` and assert the legacy `&'static str` set is a
        strict superset (no regression on persisted wire format).
      - `broker_error_classification_unchanged` — recoverable + fatal broker
        errors map to `FailureClass::Unrecoverable` (pass-through); the
        broker.call self-healing path from PR #286 still operates and is
        not double-dispatched by F-5.
  - Existing tests pass: `cargo test -p xvision-engine` green. The
    `classify_run_failure` unit tests in `eval/executor/mod.rs` are
    rewritten against the typed enum but keep their original `&'static str`
    assertions via the `.tag()` mapping.
  - `cargo clippy -p xvision-engine -- -D warnings` is clean. No new
    `unwrap()` in production code paths; the dispatcher returns typed errors.
  - No schema / migration changes. No frontend changes. No new dependencies.
  - Every recovery loop has a hard maximum. No unbounded retries anywhere.
    The bounded counts live as `pub(crate) const` declarations in
    `recovery.rs` and are exhibited in test assertions.
---

# Scope

Implements F-5 from the 2026-05-18 harness observability audit
(`team/intake/2026-05-18-harness-observability-audit.md` finding F-5).

`classify_run_failure` today is a regex-on-error-string post-hoc
classifier (eval/executor/mod.rs:48). It only fires AFTER the run has
already failed, and only to label the failure for downstream UI. The
agent never gets a chance to react to MalformedJson, ToolTimeout,
SchemaMissingField, EmptyData, or ContextOverflow — they all terminate
the run. The only recovery loop that exists pre-F-5 is
`RESPONSE_DECODE_RETRIES = 1` (agent/llm.rs:117), an untyped one-shot
retry buried inside the dispatcher.

F-5 promotes the classifier to a typed pre-recovery dispatcher: a
single `FailureClass` enum with a bounded playbook per variant. Every
transition emits a `recovery.attempt` span (the wire id F-4 reserved).
Every loop has a hard maximum.

This contract **folds in** the `agent-error-feedback-non-broker-errors`
deferred follow-up from PR #286. The recoverable/fatal split that PR
#286 shipped at the `xvision-execution` boundary is generalized here
to risk-engine, model-call, and data-fetch errors. Broker errors are
untouched (PR #286 is already the source of truth for the broker
boundary — F-5 maps both BrokerErrorClass arms through to
`FailureClass::Unrecoverable` to short-circuit the dispatcher).

Stacked on F-4 (`harness-span-taxonomy-extension`). F-4 introduces the
`RecoveryAttempt` SpanKind variant with serde + db_str mapping but
does NOT emit it anywhere — F-5 is the consumer. If F-4 lands first,
F-5 rebases to `origin/main` trivially (different crates). If F-5
opens before F-4 merges, this branch stays stacked on
`origin/task/harness-span-taxonomy-extension` until F-4 merges.

Reference: 2026-05-18 harness audit intake, finding F-5 ("Recovery is
essentially absent: 1 JSON-decode retry + 12-iter tool-use cap.
`classify_run_failure` is a regex-on-error-string post-hoc
classifier — promote it to a typed pre-recovery dispatcher with the
six playbooks. Every loop hard-capped.").

# Out of scope

- Broker-error recovery. PR #286 owns the broker boundary. F-5 maps
  both BrokerErrorClass arms to `FailureClass::Unrecoverable` (which
  short-circuits — the dispatcher does NOT re-classify or re-route
  broker errors). The same-cycle rerun and real-broker integration
  test for broker errors remain separate deferred follow-ups
  (`agent-error-feedback-same-cycle-rerun`,
  `agent-error-feedback-real-broker-roundtrip-test`).
- The actual validator BODY for `tool.validate_input` /
  `tool.validate_output` spans. F-6 owns the typed schema validator.
  F-5 does not touch the validator — it only adds the recovery
  dispatcher.
- Circuit breakers / exponential backoff / global retry budgets. The
  dispatcher is per-cycle and per-class with a fixed small upper
  bound (1 retry for most playbooks). Operator-tunable retry budgets
  are a follow-up if needed.
- A typed `mechanical_params` enum on `Strategy`. F-6 owns that.
- Trace dock UI changes. F-7 owns the Simple/Advanced toggle that
  hides `recovery.attempt` spans in Simple mode.
- The cheap-model dispatch for `ContextOverflow` summarization is
  best-effort and runs against the slot's current model with a
  reduced max_tokens by default. A configurable summarize-only model
  in `settings.toml` is a follow-up.
- Persisting the per-class recovery count in `eval_runs`. The
  `recovery.attempt` spans are the record of truth. A summary
  column in `eval_runs` (count of recoveries, terminal class) can be
  added in a follow-up if operators ask.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-recovery-state-machine status
git -C .worktrees/harness-recovery-state-machine log --oneline -5 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/harness-recovery-state-machine
#   - base is origin/task/harness-span-taxonomy-extension (F-4)
#   - exactly one commit on top of F-4 head at the start;
#     plus the F-5 work commits as they land
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-recovery-state-machine \
  -b task/harness-recovery-state-machine origin/task/harness-span-taxonomy-extension
```

When F-4 (and F-2) merge, rebase to `origin/main`:

```bash
git -C .worktrees/harness-recovery-state-machine fetch origin
git -C .worktrees/harness-recovery-state-machine rebase origin/main
# Conflicts expected in: agent/observability.rs (separate helpers, mechanical)
```

# Notes

Append checkpoints / PR links below.

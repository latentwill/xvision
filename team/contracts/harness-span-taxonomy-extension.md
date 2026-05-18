---
track: harness-span-taxonomy-extension
lane: integration
wave: harness-observability-audit
worktree: .worktrees/harness-span-taxonomy-extension
branch: task/harness-span-taxonomy-extension
base: origin/main   # rebased onto main 2026-05-18 after F-2 squashed in as PR #294
status: pr-open
depends_on:
  - harness-span-attrs-populate   # F-2 — new spans populate the typed SpanAttributes bag from emission
blocks:
  - harness-recovery-state-machine        # F-5 — emits recovery.attempt
  - trace-dock-simple-advanced-toggle     # F-7 — gated on F-2 + F-4
stacking: declared:harness-span-attrs-populate
allowed_paths:
  - crates/xvision-observability/src/types.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-observability/tests/span_kind_roundtrip.rs
  - crates/xvision-engine/src/agent/observability.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/tests/agent_span_taxonomy.rs
  - team/contracts/harness-span-taxonomy-extension.md
  - team/status/harness-span-taxonomy-extension.md
  - team/board.md
forbidden_paths:
  - crates/xvision-engine/migrations/**          # F-3 owns the prompt_version migration
  - crates/xvision-engine/src/strategies/**      # F-6 owns mechanical_params typing
  - crates/xvision-engine/src/eval/executor/**   # F-5 owns classify_run_failure → typed dispatcher
  - frontend/web/**                              # F-7 owns the trace-dock toggle
  - crates/xvision-observability/migrations/**
  - crates/xvision-dashboard/**
interfaces_used:
  - xvision_observability::SpanKind   # extended with 4 new variants
  - xvision_observability::SpanAttributes   # from F-2; populated by new emission sites
  - xvision_observability::SpanStartedEvent / SpanFinishedEvent
  - xvision_engine::agent::observability::ObsEmitter   # gains emit_state_transition + emit_tool_validate_*
parallel_safe: true   # safe alongside F-3 (different files) and F-6 (different files)
parallel_conflicts:
  - harness-span-attrs-populate   # stacked, not conflicting — rebase on merge
verification:
  - cargo test -p xvision-observability
  - cargo test -p xvision-engine
  - cargo test -p xvision-observability --test span_kind_roundtrip
  - cargo test -p xvision-engine --test agent_span_taxonomy
  - cargo build --workspace
acceptance:
  - `SpanKind` gains four variants: `ToolValidateInput` → `"tool.validate_input"`, `ToolValidateOutput` → `"tool.validate_output"`, `RecoveryAttempt` → `"recovery.attempt"`, `StateTransition` → `"state.transition"`. Each has the matching serde `rename` AND matching arm in `as_db_str()`. The serde wire string and the db_str MUST be identical so the existing column-string SQL comparison invariant (see types.rs:1-6 docstring) holds.
  - New `tests/span_kind_roundtrip.rs` covers all twelve pre-existing variants plus the four new ones: each variant round-trips through `serde_json::to_string` / `from_str` AND returns the same string from `as_db_str()`. The test fails closed if a variant is added later without updating `as_db_str()`.
  - `ObsEmitter::emit_tool_validate_input(span_id, tool_name)` and `ObsEmitter::emit_tool_validate_output(span_id, tool_name)` exist on `crates/xvision-engine/src/agent/observability.rs`. Each opens an instantaneous span (open then close-ok with the same `span_id`) keyed `SpanKind::ToolValidateInput` / `SpanKind::ToolValidateOutput`, parented to the current tool.call span. The `SpanAttributes` bag carries `run_id` + `tool_name` (using the F-2 typed struct). The validator BODY remains a no-op — F-6 owns the real schema check. The spans exist as the instrumentation seam.
  - In the agent execute path (`crates/xvision-engine/src/agent/execute.rs`), every `tool.call` span is bracketed by `tool.validate_input` before the call and `tool.validate_output` after. Order is mandatory: validate_input → tool.call open → tool.call close → validate_output. The brackets MUST emit even when the tool call errors — `validate_output` records the post-condition either way (no-op today; F-6 will validate the actual response).
  - `ObsEmitter::emit_state_transition(from: Option<RunStatus>, to: RunStatus, parent_span_id: Option<&str>)` exists. Emits an instantaneous span of kind `SpanKind::StateTransition`, name `"state.transition"`, with `attributes_json` carrying `{"from": "<old_status_or_null>", "to": "<new_status>"}` merged with the F-2 `SpanAttributes` bag (`run_id` populated). Called once from `emit_run_started` (with `from = None`, `to = RunStatus::Running`) and once from `emit_run_finished` (with `from = Some(RunStatus::Running)`, `to = <terminal_status>`).
  - `RecoveryAttempt` is added as a `SpanKind` variant with serde + db_str BUT is NOT emitted anywhere in this PR. The contract documents this as F-5's seam: when F-5 lands the typed `FailureClass` dispatcher, each transition through it emits a `recovery.attempt` span. F-4 reserves the wire identifier so F-5 doesn't conflict.
  - New `tests/agent_span_taxonomy.rs` in xvision-engine integration-tests: builds a fake `ObsEmitter` backed by an in-memory bus (existing test pattern in observability tests), exercises the engine's tool-call wrapper around a stub tool, and asserts the recorded event stream contains `validate_input → tool.call.started → tool.call.finished → validate_output` in that exact order, with matching `tool_name` on the validate spans. A second test exercises `emit_run_started` + `emit_run_finished` and asserts two `state.transition` spans land with the expected from/to attributes.
  - Existing tests pass: `cargo test -p xvision-observability` and `cargo test -p xvision-engine` both green. No regression in the synthetic event-bus tests (`event_bus_synthetic.rs`, `event_bus_saturation.rs`, `event_bus_drop_oldest.rs`, `export_schema.rs`).
  - No schema changes. No migration. Migration registry is untouched.
  - No frontend changes — the trace dock will surface the new span kinds automatically (they're rendered by kind string). F-7 owns the Simple/Advanced toggle that hides them.
  - No new dependencies. The 4 variants and the emission helpers compile against the existing crate graph.
---

# Scope

Implement F-4 from the 2026-05-18 harness observability audit
(`team/intake/2026-05-18-harness-observability-audit.md`).

Extend `SpanKind` with four new variants that the audit identified as
load-bearing for forensics:

- `tool.validate_input` — brackets the pre-condition of a tool call.
  Today the body is a no-op (no validator runs); the span exists so
  F-6 has a seam to drop a typed schema check into without
  re-instrumenting the runner.
- `tool.validate_output` — brackets the post-condition of a tool
  call. Same shape as `tool.validate_input`.
- `recovery.attempt` — F-5's seam. F-4 reserves the wire identifier
  and serde mapping; F-5 emits the spans from the typed
  `FailureClass` dispatcher.
- `state.transition` — fires on every change in run lifecycle status
  (Queued → Running → terminal). Today the runner emits
  `RunStartedEvent` / `RunFinishedEvent` but no intermediate
  "status changed" record; the trace dock cannot show a timeline of
  transitions. The new span carries `{"from", "to"}` in its
  attributes bag.

This is the "wire the seams" track. F-6 and F-5 fill the
`tool.validate_*` and `recovery.attempt` bodies respectively; F-7
(blocked on this contract + F-2) adds the Simple/Advanced toggle that
hides the new instrumentation noise from operators.

Stacked on F-2 (`harness-span-attrs-populate`, PR #293). The new
emission sites populate F-2's typed `SpanAttributes` bag from
emission — `run_id` always, `tool_name` on validate spans. If F-2
merges first, F-4 rebases to `origin/main` trivially (separate
sections of `types.rs`). If F-4 merges first, F-2's rebase is also
trivial (different lines).

Reference: 2026-05-18 harness audit intake, finding F-4.

# Out of scope

- The actual validation logic in `tool.validate_input` /
  `tool.validate_output`. F-6 owns the typed schema check. F-4 ships
  the spans as no-op brackets so the wire format and ordering are
  pinned before F-6 starts.
- The recovery state machine. F-5 owns `classify_run_failure` →
  typed dispatcher and emits `recovery.attempt` from each transition.
  F-4 ships the SpanKind variant only.
- A new `agent_slots.prompt_version` column. F-3 owns that
  migration. The F-2 `SpanAttributes` struct already has the field;
  F-3 wires it once the column exists.
- Trace dock UI changes. F-7 owns the Simple/Advanced toggle. F-4's
  new spans are renderable today via the existing per-kind dispatch
  in `AgentRunIndentedTimeline` — the question is operator triage
  UX, not span rendering.
- A `RunStatus` change in the eval-runs store (`eval/store.rs`).
  That table tracks a higher-level concept (user-visible eval-run
  state); `state.transition` here is the per-agent-run observability
  view. The eval-runs ledger has its own `update_status` and is
  unchanged.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-span-taxonomy-extension status
git -C .worktrees/harness-span-taxonomy-extension log --oneline -5 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/harness-span-taxonomy-extension
#   - base is origin/task/harness-span-attrs-populate (F-2)
#   - exactly one commit on top of F-2 head (14a2e79) at the start;
#     plus the F-4 work commits as they land
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-span-taxonomy-extension \
  -b task/harness-span-taxonomy-extension origin/task/harness-span-attrs-populate
```

When F-2 (PR #293) merges, rebase to `origin/main`:

```bash
git -C .worktrees/harness-span-taxonomy-extension fetch origin
git -C .worktrees/harness-span-taxonomy-extension rebase origin/main
# Conflicts expected in: types.rs (separate sections, mechanical), lib.rs
# (single re-export line)
```

# Notes

Append checkpoints / PR links below.

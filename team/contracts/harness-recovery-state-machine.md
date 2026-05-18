---
track: harness-recovery-state-machine
lane: integration
wave: harness-observability-audit
worktree: .worktrees/harness-recovery-state-machine
branch: task/harness-recovery-state-machine
base: origin/main
status: pr-open
depends_on:
  - harness-span-attrs-populate          # F-2: SpanAttributes bag the recovery span piggybacks on
  - harness-span-taxonomy-extension      # F-4: SpanKind::RecoveryAttempt + StateTransition variants
blocks: []
stacking: harness-span-taxonomy-extension
allowed_paths:
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/observability.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/src/eval/executor/recovery.rs
  - crates/xvision-engine/tests/agent_observability_hash.rs
  - crates/xvision-engine/tests/agent_span_taxonomy.rs
  - crates/xvision-observability/src/types.rs
  - crates/xvision-observability/tests/span_kind_roundtrip.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-dashboard/**
  - frontend/web/**
interfaces_used:
  - xvision_observability::SpanKind::{RecoveryAttempt, StateTransition, ToolValidateInput, ToolValidateOutput}
  - xvision_observability::SpanAttributes (run_id, retry_count)
  - xvision_engine::eval::executor::classify_run_failure (legacy `&'static str` surface, preserved)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test --lib -p xvision-engine eval::executor
  - cargo test -p xvision-engine --test agent_span_taxonomy
  - cargo build --workspace
acceptance:
  - New module `crates/xvision-engine/src/eval/executor/recovery.rs` with `FailureClass` enum (MalformedJson, ToolTimeout, SchemaMissingField, EmptyData, ContextOverflow, RepeatedToolFailure, Unrecoverable) carrying structured payloads.
  - `recovery::classify(err) -> FailureClass` typed classifier; walks the anyhow source chain via alternate `Display`.
  - `FailureClass::tag()` projection covers every variant; the typed classifier is the source of truth.
  - `RecoveryOutcome` enum (`Continue` / `Stop` / `Surfaced`).
  - Bounded retry constants (`MAX_DECODE_REPAIR_PROMPTS`, `MAX_TOOL_RETRIES`) hard-cap every recovery loop.
  - **Wire-format compatibility:** `classify_run_failure` delegates to `recovery::classify(...).tag()` with explicit fixups so legacy `&'static str` callers (eval store, dashboard, CLI grep) see exactly the pre-F-5 wire format. Fixups cover `MalformedJson` (split back to `invalid_json` / `provider_decode`), `ToolTimeout` (→ `unclassified`), `ContextOverflow` (→ `provider_http_error` or `unclassified` per legacy needle), and `EmptyData` (→ `unclassified`). Regression tests pin each projection.
  - `ObsEmitter::emit_recovery_attempt(...)` emits one instantaneous `SpanKind::RecoveryAttempt` span carrying the typed `SpanAttributes` bag (`run_id`, `retry_count`) merged with a `recovery` sub-object holding the `failure_class` / `outcome` / `attempt`.
  - Folds in the deferred `agent-error-feedback-non-broker-errors` follow-up from PR #286 — the recoverable/fatal split generalizes to risk/model/data-fetch errors. Broker classes short-circuit to `Unrecoverable` so PR #286's broker self-heal arm is not double-dispatched.
  - 54 `eval::executor` lib tests pass; `agent_span_taxonomy` tests pass; workspace builds clean.

---

# Scope

F-5 from `team/intake/2026-05-18-harness-observability-audit.md`.
Promotes `classify_run_failure` from a regex-on-error-string post-hoc
classifier to a typed pre-recovery dispatcher with six bounded
playbooks. Every recovery loop is hard-capped (`MAX_DECODE_REPAIR_PROMPTS`,
`MAX_TOOL_RETRIES`).

Stacks under F-4 (`harness-span-taxonomy-extension`): the
`SpanKind::RecoveryAttempt` + `SpanKind::StateTransition` variants F-4
introduces are the spans this dispatcher emits.

# Out of scope

- Frontend trace dock filtering (F-7).
- Per-tool input/output JSON-schema validators that emit the
  `tool.validate_*` spans (F-4 provides the SpanKind; the validators
  themselves are follow-up work, gated on F-6 typed
  `mechanical_params`).
- Broker error recovery — PR #286 owns the broker boundary; this
  module short-circuits broker classes to `Unrecoverable`.
- Database migration — no schema change. The `[<class>]` wire prefix
  is preserved byte-identical via the `classify_run_failure` fixups.

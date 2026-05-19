---
track: harness-span-attrs-populate
lane: leaf
wave: harness-observability-audit
worktree: .worktrees/harness-span-attrs-populate
branch: task/harness-span-attrs-populate
base: origin/main
status: pr-open
depends_on: []
blocks:
  - trace-dock-simple-advanced-toggle  # F-7 needs the populated bag to triage on
  - harness-prompt-version-field       # F-3 will populate prompt_version field added here
stacking: none
allowed_paths:
  - crates/xvision-observability/src/types.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-engine/src/agent/observability.rs
  - crates/xvision-engine/src/agent/execute.rs
forbidden_paths:
  - crates/xvision-observability/migrations/**
  - crates/xvision-dashboard/**
  - frontend/web/**
interfaces_used:
  - xvision_observability::SpanStartedEvent.attributes_json (existing Option<String> column)
  - xvision_engine::strategies::LLMSlot.role (used as `stage`)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-observability
  - cargo test -p xvision-observability --lib span_attributes_tests
  - cargo build -p xvision-engine
acceptance:
  - New `SpanAttributes` struct in `xvision-observability/src/types.rs`, re-exported from `lib.rs`. Optional fields: `run_id`, `agent_id`, `stage`, `model`, `provider`, `tool_name`, `retry_count`, `prompt_version`. All `#[serde(default, skip_serializing_if = "Option::is_none")]` so the wire payload stays compact.
  - `SpanAttributes::to_attributes_json` returns `None` when every field is `None` (avoids `"{}"` rows); returns `Some(json)` otherwise with absent fields skipped.
  - `SpanAttributes::merge_into_object` writes typed fields into a `serde_json::Map` without overwriting existing keys — used by the broker-call site to coexist with the `broker_call` sub-object added by `qa-trace-broker-spans`.
  - `ObsEmitter::emit_model_call_started` takes a new `stage: Option<&str>` parameter and populates `attributes_json` with `run_id` / `provider` / `model` / `stage`.
  - `ObsEmitter::emit_broker_call_started` continues to carry the `broker_call` sub-object **and** additionally emits the typed `run_id` field at the top level (via `merge_into_object`).
  - Caller in `crates/xvision-engine/src/agent/execute.rs` passes `Some(&input.slot.role)` as the new `stage` argument.
  - `attributes_json` deserialize is forward-compatible (no `deny_unknown_fields` on `SpanAttributes`) so older rows / future fields parse cleanly.
  - Unit tests in `crates/xvision-observability/src/types.rs::span_attributes_tests`: empty-default-is-None, populated-skip-None-fields, round-trip, broker-merge preserves sub-object, merge does not overwrite collisions, deserialize tolerates unknown fields.
  - No SQL migration, no new SpanKind variant, no wire-format change to `SpanStartedEvent` (the field type is already `Option<String>`).
  - No frontend changes — `agent-runs.ts` already parses `attributes_json` into a generic record; F-7 will add the SpanInspector key-value grid in a separate track.
  - Existing tests still pass. `cargo test -p xvision-observability` green.

---

# Scope

F-2 from `team/intake/2026-05-18-harness-observability-audit.md`. The
`SpanStartedEvent.attributes_json` column is wired through the recorder,
SQLite layer, and frontend parser, but every emission site passes
`None`. This contract fills the bag.

Scope is deliberately tight: only the two existing engine emission
sites (`emit_model_call_started`, `emit_broker_call_started`) and a
typed `SpanAttributes` struct in `xvision-observability`. The four
new `SpanKind` variants (F-4) and the `prompt_version` migration (F-3)
are separate tracks; the `tool_name` and `retry_count` fields stay
reserved (Option) until those tracks add the call sites that own them.

The F-7 trace-dock Simple/Advanced toggle is gated on this contract +
F-4. Once both land, the SpanInspector will have something
non-trivial to hide in Simple mode.

# Out of scope

- New `SpanKind` variants (F-4, separate contract).
- `prompt_version` field on `agent_slots` (F-3, requires migration).
- `tool_name` / `retry_count` population (F-5 owns the call sites).
- SpanInspector UI changes (F-7, gated on this + F-4).
- Touching `xvision-observability` SQLite recorder — `attributes_json`
  is already persisted as-is.

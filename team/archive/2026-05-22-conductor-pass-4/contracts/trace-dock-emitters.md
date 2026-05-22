---
track: trace-dock-emitters
lane: integration
wave: trace-dock-emitters-2026-05-22
worktree: .worktrees/trace-dock-emitters
branch: task/trace-dock-emitters
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/pipeline.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/types.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-observability/tests/event_emitters.rs
  - crates/xvision-observability/tests/tool_call_emit.rs
  - frontend/web/src/stores/trace-dock.ts
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agent/recovery.rs
  - crates/xvision-engine/src/agent/memory_recorder.rs
interfaces_used:
  - xvision_observability::sqlite (existing writers for spans/model_calls/supervisor_notes; ADD writers for tool_calls/events/checkpoints — design call per sub-item below)
  - xvision_engine::agent::pipeline (tool-call dispatch sites)
  - xvision_engine::eval::executor (per-decision lifecycle events)
parallel_safe: false
parallel_conflicts:
  - trader-noop-skip
  - indicator-tool-wiring
verification:
  - cargo test -p xvision-observability --test event_emitters
  - cargo test -p xvision-observability --test tool_call_emit
  - cargo test -p xvision-engine
  - pnpm -C frontend/web test -- trace-dock SpanInspector
acceptance:
  - **tool_calls:** every `ToolCall` dispatch in `pipeline.rs` emits a `tool_calls` row with `(name, args_redacted, result_or_error, latency_ms, parent_decision_index)`
  - **events:** missing `INSERT INTO events` writer added to `xvision-observability/sqlite.rs`; engine emits bar-level lifecycle events: `decision_started`, `decision_completed`, `fill_attempted`, `guardrail_fired`, `early_stop_triggered`, `flat_skip_fired`
  - **supervisor_notes:** broadened beyond F-7 — emitters added for preflight warnings, broker-rule violations, cost-cap warnings, conviction-floor skips, F-9 flat-degeneracy skip notes
  - **spans:** per-decision spans emitted (not just run-level start/end) so the dock can show per-decision LLM/tool/fill timing breakdown
  - **checkpoints / approvals / sandbox_results:** explicit design call documented in the PR description — either name call sites and add emitters, OR open a follow-up to drop these tables from migration 018 in a separate contract so the schema stops promising what nothing delivers
  - Trace dock UI surfaces the new event kinds (existing Simple/Advanced toggle handles them)
---

# Scope

Fill in the trace-dock event emitters so the per-decision trace
surface has real content. Today the engine emits to `model_calls`
(heavy) and `supervisor_notes` (sparse — only the F-7 guardrail
rewrite path); `tool_calls` is referenced in zero files in
`crates/xvision-engine/src/agent/pipeline.rs`; the `events` table
has no writer in the observability crate at all.

Source: `FOLLOWUPS.md` F43. Filed 2026-05-21 after the operator
observed the trace dock shows only `model_calls` rows. The 2026-05-19
eval-traces audit noted this as F-11(f) "partially addressed by V2E
#422"; this contract finishes the work.

Five sub-items (all in one contract because they share emit sites
and the unified observability writer surface):

1. `tool_calls` — wire emitters around every ToolCall dispatch
2. `events` — add the SQL writer + lifecycle event emitters
3. `supervisor_notes` — broaden beyond F-7
4. `spans` — per-decision spans, not just run-level
5. `checkpoints / approvals / sandbox_results` — design call (emit or remove)

# Out of scope

- New trace UI features beyond surfacing the new event kinds
  (existing trace-dock Simple/Advanced toggle handles them)
- Recovery-state-machine emit sites (covered by the harness phase-2
  contracts — coordinate via team/queue/ if both active)
- Memory provenance event payload changes (covered by
  `memory-provenance-in-decisions-trace`)
- Migration changes — schema for `spans`, `model_calls`, `tool_calls`,
  `checkpoints`, `supervisor_notes` already exists from migration 018

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/trace-dock-emitters -b task/trace-dock-emitters origin/main
```

# Notes

Coordinate with:
- `trader-noop-skip` (emits `flat_skip_fired` — that contract should
  use this contract's event surface)
- `indicator-tool-wiring` (will exercise the new `tool_calls`
  emission path)
- The 3 harness phase-2 contracts (`harness-recovery-*`) — they emit
  through `ObsEmitter::emit_recovery_attempt` / `emit_recovery_failed`
  already; this contract should not collide with that path

Audit reference: `team/intake/archive/2026-05-18-harness-observability-audit.md`
F-11 sub-items (a)–(e) match this contract's five sub-items.

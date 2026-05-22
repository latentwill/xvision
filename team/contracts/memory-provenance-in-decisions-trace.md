---
track: memory-provenance-in-decisions-trace
lane: foundation
wave: memory-safety-and-observability-2026-05-22
worktree: .worktrees/memory-provenance-in-decisions-trace
branch: task/memory-provenance-in-decisions-trace
base: origin/main
status: ready
depends_on: []
blocks:
  - memory-aware-eval-findings
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/memory_recorder.rs
  - crates/xvision-engine/tests/agent_memory_dispatch.rs
  - crates/xvision-observability/src/types.rs
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/tests/memory_recall_decision_id.rs
  - crates/xvision-dashboard/src/routes/agent_runs.rs
  - frontend/web/src/features/eval-runs/MemoryPanel.tsx
  - frontend/web/src/features/eval-runs/MemoryPanel.test.tsx
forbidden_paths:
  - crates/xvision-memory/**
  - crates/xvision-engine/src/eval/findings/**
interfaces_used:
  - xvision_engine::agent::memory_recorder::MemoryRecorder::recall (extend signature with decision_id)
  - xvision_observability::events emission for `memory_recall` (carry decision_id)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine --test agent_memory_dispatch
  - cargo test -p xvision-observability --test memory_recall_decision_id
  - pnpm -C frontend/web test -- MemoryPanel
acceptance:
  - `memory_recall` events emit `decision_id` alongside `run_id` and `memory_item_ids[]`
  - The `events` table (or the recall-event sink) persists the per-decision tuple `(run_id, decision_id, memory_item_id)`
  - The eval-review run-detail loader can answer "which memories influenced decision N" by joining decisions to recall events
  - A V2D-shaped integration test asserts the emitted event carries `decision_id` and the item ids match the recall set
---

# Scope

Bind `memory_recall` events to the specific `decision_id` they fed
into, not just the run id. Thread `decision_id` through
`MemoryRecorder::recall` and the corresponding observability event so
the eval-review trace can answer "which memories influenced this
decision" instead of only "this run recalled 12 items."

Foundation for `memory-aware-eval-findings` — that finding extractor
needs the per-decision provenance to attribute outcomes to specific
recalled items.

Source intake: `team/intake/2026-05-21-memory-safety-and-observability.md`.

# Out of scope

- No schema changes to the memory store itself (`xvision-memory` crate is forbidden)
- No new finding kinds — that's `memory-aware-eval-findings`
- No retroactive backfill of historical runs — provenance starts at PR-merge time

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/memory-provenance-in-decisions-trace -b task/memory-provenance-in-decisions-trace origin/main
```

# Notes

V2D's `events.jsonl` carries `memory_recall` / `memory_write` /
`memory_write_error` at the run level. The recall emit site is
`crates/xvision-engine/src/agent/execute.rs:232`. If the events table
schema doesn't already have a `decision_id` column, this contract may
need an engine migration — coordinate with the conductor to reserve
migration **032** before adding it. Prefer attaching `decision_id` to
the event payload JSON if the column-add is structurally invasive.

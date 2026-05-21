---
track: memory-aware-eval-findings
lane: leaf
wave: memory-safety-and-observability-2026-05-22
worktree: .worktrees/memory-aware-eval-findings
branch: task/memory-aware-eval-findings
base: origin/main
status: deferred
depends_on:
  - memory-provenance-in-decisions-trace
blocks: []
stacking: declared:memory-provenance-in-decisions-trace
allowed_paths:
  - crates/xvision-engine/src/eval/findings/memory.rs
  - crates/xvision-engine/src/eval/findings/mod.rs
  - crates/xvision-engine/tests/memory_aware_findings.rs
  - frontend/web/src/features/eval-runs/MemoryPanel.tsx
  - frontend/web/src/features/eval-runs/MemoryPanel.test.tsx
forbidden_paths:
  - crates/xvision-memory/**
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/memory_recorder.rs
  - crates/xvision-engine/migrations/**
interfaces_used:
  - xvision_engine::eval::findings::Finding (existing emit seam)
  - xvision_observability::events join on (run_id, decision_id) → memory_item_ids[]
  - Per-decision outcome judgement (existing eval-review extractor surface)
parallel_safe: false
parallel_conflicts:
  - memory-provenance-in-decisions-trace
verification:
  - cargo test -p xvision-engine --test memory_aware_findings
  - pnpm -C frontend/web test -- MemoryPanel
acceptance:
  - A run with a known bad-outcome decision driven by a known stale memory item emits one `warning` finding with the expected kind / body
  - Good outcomes driven by a memory item emit an `info` finding (opt-in by config; default off to avoid noise)
  - Finding body names the memory item id(s) responsible
  - MemoryPanel surface displays the finding inline with the recall list
---

# Scope

When a decision's outcome is judged (good / bad / inconclusive),
emit a `Finding` that names the memory items most likely to have
driven the decision based on the per-decision provenance mapping.
Severity tracks the outcome — bad outcomes driven by stale memory
get `warning`; good outcomes get `info` (opt-in).

Pure observability — no policy decisions about memory recall
behavior, only surfacing of what already happened.

Source intake: `team/intake/2026-05-21-memory-safety-and-observability.md`.

Deferred behind `memory-provenance-in-decisions-trace` because this
extractor needs the per-decision provenance join.

# Out of scope

- Changes to recall behavior — read-only over the provenance data
- Memory-driven sizing or veto rules — findings are advisory, not enforced
- Retroactive backfill — findings emit on new runs only

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/memory-aware-eval-findings -b task/memory-aware-eval-findings origin/main
```

# Notes

Pairs naturally with the eval-honesty wave (#448–#452) — same shape
of finding, same review-time emission seam. Pattern after
`xvision-engine/src/eval/findings/uniformity.rs` or
`extractor.rs` for the extractor module layout.

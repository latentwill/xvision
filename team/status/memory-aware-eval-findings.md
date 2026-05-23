---
track: memory-aware-eval-findings
worker: claude-opus
phase: ready-to-merge
last_update: 2026-05-23
---

## Verification

- `cargo test -p xvision-engine --test memory_aware_findings` — 5 passed
- `cargo test -p xvision-engine --lib findings::memory` — 9 passed (unit)
- `cargo test -p xvision-engine --test eval_findings` — 9 passed (regression)
- `pnpm test MemoryPanel` — 19 passed (8 new + 11 existing review variant)
- `cargo build -p xvision-engine` — clean, no warnings on new code



# Status

Claimed 2026-05-23 after confirming dep `memory-provenance-in-decisions-trace`
(PR #523) merged. The recorder writes `RunEvent::MemoryRecall` into the
`events` table with `kind = 'memory_recall'` and a serialized
`MemoryRecallEvent` payload that carries `(run_id, decision_id, namespace,
items[])`. This extractor projects those rows back out and joins to the
per-decision outcome (`pnl_realized` sign) to emit `Finding`s.

## Plan

1. Add `crates/xvision-engine/src/eval/findings/memory.rs` — pure
   detector with signature
   `detect_memory_aware_findings(decisions, recalls, opts) -> Vec<Finding>`.
   - Outcome derivation: `pnl_realized > 0 → good`, `< 0 → bad`,
     `None or 0 → inconclusive`.
   - Emit `warning` (`kind = "memory_recalled_into_bad_decision"`) for
     bad outcomes with recall events; body names the memory item ids.
   - Emit `info` (`kind = "memory_recalled_into_good_decision"`) for
     good outcomes — opt-in (`emit_good_outcomes: false` default).
2. Re-export the detector + opts from `findings/mod.rs`.
3. Tests in `crates/xvision-engine/tests/memory_aware_findings.rs`.
4. Frontend: `frontend/web/src/features/eval-runs/MemoryPanel.tsx` +
   `MemoryPanel.test.tsx` — inline finding display next to the recall list.

## Notes

- The existing `MemoryPanel` at `review/MemoryPanel.tsx` is the V2D recall
  surface inside the review tab. The contract scopes a NEW component at
  the top-level `eval-runs/MemoryPanel.tsx` that pairs recall rows with
  the new memory-aware findings inline. This component is the finding-aware
  superset; the review-tab one stays untouched.
- No migration: `events` table already accepts the `memory_recall` rows,
  and `eval_findings` already accepts open-enum `kind` strings.
- Retroactive backfill is explicitly out of scope.

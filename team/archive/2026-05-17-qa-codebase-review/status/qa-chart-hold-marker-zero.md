---
track: qa-chart-hold-marker-zero
status: merged
last_update: 2026-05-17
worker: closed via PR #216 (commit e8eb9fc — "Skip zero-priced hold markers")
---

## Outcome

Contract was delivered out-of-band by PR #216 ("[leaf] qa-chart-hold-marker-zero:
skip missing-bar hold markers") merged before this wave's bookkeeping caught up.

The fix on `main` lives at `crates/xvision-engine/src/api/chart.rs:610-614` —
the "hold" branch uses `if let Some(price) = bar_close.get(&t).copied()` so
markers are skipped when the decision timestamp does not resolve to a loaded
bar close. The `unwrap_or(0.0)` artifact that was producing zero-priced hold
markers is eliminated.

Regression test: `crates/xvision-engine/tests/chart_hold_markers.rs` (121 lines)
covers aligned and missing-bar paths.

Verified by the worker session 2026-05-17: `git diff e8eb9fc 2ff2b83 --
crates/xvision-engine/src/api/chart.rs crates/xvision-engine/tests/chart_hold_markers.rs`
is empty between merged commit and stale task branch.

## Closing notes

- The unmerged `task/qa-chart-hold-marker-zero` branch on the local worktree
  is now stale and can be removed. (`git branch -D task/qa-chart-hold-marker-zero`
  + `git worktree remove .worktrees/qa-chart-hold-marker-zero`.)
- Single-writer claim on `crates/xvision-engine/src/api/chart.rs` releases
  with this status flip.

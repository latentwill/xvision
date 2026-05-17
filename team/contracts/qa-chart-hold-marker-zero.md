---
track: qa-chart-hold-marker-zero
lane: leaf
wave: qa-2026-05-17
worktree: .worktrees/qa-chart-hold-marker-zero
branch: task/qa-chart-hold-marker-zero
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/api/chart.rs
  - crates/xvision-engine/tests/chart_hold_markers.rs
forbidden_paths:
  - crates/xvision-engine/src/eval/**
  - crates/xvision-engine/src/agent/**
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-engine/src/api/strategy.rs
  - crates/xvision-engine/migrations/**
  - frontend/**
interfaces_used:
  - "xvision_engine::api::chart::build_markers"
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build -p xvision-engine
  - cargo test -p xvision-engine api::chart
  - cargo test -p xvision-engine --test chart_hold_markers
acceptance:
  - "`build_markers` no longer emits hold markers at price `0.0` when a decision timestamp does not match a loaded bar"
  - "Chosen fallback policy is one of: (a) skip the marker entirely, OR (b) use the nearest prior bar close within the run granularity. PR description documents the choice"
  - "If (b) is chosen, a diagnostic (warn-level log or returned diagnostic field) records that a fallback was applied, so silent fallback is detectable"
  - "Regression test in `tests/chart_hold_markers.rs` constructs decisions with timestamps that don't match any bar and asserts no marker appears at `0.0`"
  - "Existing buy/sell/hold marker tests still pass; chart autoscaling behavior is sane on a dataset with mixed matching and missing bars"
---

# Scope

Implements remediation step 6b of `qa/2026-05-17-comprehensive-codebase-review.md`
("Hold chart markers can render at price zero when bar lookup misses").
Fixes the `bar_close.get(&t).copied().unwrap_or(0.0)` fallback in
`build_markers` so a missed lookup doesn't distort chart autoscaling or
visually imply a market crash.

# Out of scope

- Buy/sell marker rendering — only hold markers exhibited the issue.
- Restructuring how bars are loaded or aligned with decision timestamps.
- Any other chart route — only `api/chart.rs::build_markers` is in scope.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-chart-hold-marker-zero \
  -b task/qa-chart-hold-marker-zero origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
git -C .worktrees/qa-chart-hold-marker-zero status
```

# Notes

Implementation hints (do not rewrite the contract — use as starting points):

- "Skip" is the lowest-risk fallback and easiest to test. "Nearest prior
  bar close within the run granularity" is more user-friendly but requires
  knowing the run granularity at the call site.
- If skipping, ensure the surrounding marker list remains chronologically
  coherent (no off-by-one indexing).
- The diagnostic, if any, should use the engine's normal tracing surface,
  not stdout.

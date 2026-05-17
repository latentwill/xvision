---
track: qa-eval-retry-params-override
lane: leaf
wave: qa-2026-05-17
worktree: .worktrees/qa-eval-retry-params-override
branch: task/qa-eval-retry-params-override
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-engine/tests/eval_retry_idempotency.rs
forbidden_paths:
  - crates/xvision-engine/src/eval/executor/**
  - crates/xvision-engine/src/agent/**
  - crates/xvision-engine/src/strategies/**
  - crates/xvision-engine/migrations/**
  - frontend/**
interfaces_used:
  - "xvision_engine::api::eval::retry"
  - "xvision_engine::api::eval::params_override comparator"
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build -p xvision-engine
  - cargo test -p xvision-engine api::eval
  - cargo test -p xvision-engine --test eval_retry_idempotency
acceptance:
  - "The retry handler in `crates/xvision-engine/src/api/eval.rs` matches the documented idempotency contract `(agent_id, scenario_id, mode, params_override)` — `params_override` equality is part of the in-flight sibling predicate"
  - "If a queued or running sibling has the same `(agent_id, scenario_id, mode)` but a different `params_override`, retry starts a NEW run instead of returning the unrelated sibling"
  - "Regression test in `tests/eval_retry_idempotency.rs` constructs the conflicting-sibling scenario and asserts a new run is started"
  - "If the team decides coalescing across overrides is intentional, the alternative is acceptable: update the comment + API contract docs and add a test that asserts the coalescing behavior. Either path satisfies acceptance; pick one explicitly in the PR description"
  - "No regressions on existing happy-path retry behavior (true-duplicate sibling still coalesces)"
---

# Scope

Implements remediation step 6a of `qa/2026-05-17-comprehensive-codebase-review.md`
("Eval retry idempotency ignores `params_override` despite documented
contract"). Aligns the retry implementation with its own documented
idempotency key, OR updates the docs/comment if the team decides coalescing
across overrides is the intended behavior. Either outcome is acceptable;
the bug is the silent mismatch.

# Out of scope

- Role normalization — owned by `qa-role-normalization` (different
  surface in this file is forbidden to that track; this track owns
  `api/eval.rs` exclusively for the wave).
- Refactoring the retry handler's queue interaction beyond the
  idempotency predicate.
- Migrations — none required.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-eval-retry-params-override \
  -b task/qa-eval-retry-params-override origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
git -C .worktrees/qa-eval-retry-params-override status
```

# Notes

Implementation hints (do not rewrite the contract — use as starting points):

- The QA reviewer left both fix options open. The implementation route is
  cheaper if `params_override` already has a canonical serialization (JSON
  with stable key order) — equality just compares that.
- If `params_override` is `Option<serde_json::Value>`, beware that two
  semantically-equal objects with different key order are unequal under
  `==`. Use a canonicalized form for the comparator.
- This track does not change the documented contract unless the
  alternative path is taken — in which case the PR description must call
  out the chosen semantics.

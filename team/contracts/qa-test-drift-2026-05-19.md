---
track: qa-test-drift-2026-05-19
lane: leaf
wave: qa-operator-2026-05-19
worktree: .worktrees/qa-test-drift-2026-05-19
branch: task/qa-test-drift-2026-05-19
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/src/eval/postprocess.rs
forbidden_paths:
  - crates/xvision-engine/src/eval/store.rs
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - StrategyStore (read-only for the test)
  - RunStore::finalize (reads its semantics, doesn't change them)
  - Run / RunStatus / MetricsSummary
verification:
  - cargo test -p xvision-engine --lib authoring::tests::validate_draft_reports_missing_agent_for_fresh_template
  - cargo test -p xvision-engine --lib eval::postprocess::tests
  - cargo test -p xvision-engine --lib
acceptance:
  - `authoring::tests::validate_draft_reports_missing_agent_for_fresh_template`
    passes. Update the test assertion at line 826 to match the actual
    error message at `authoring.rs:501` (`"attach at least one
    complete agent with provider/model before validation"`). Pick a
    substring that's discriminating but not brittle —
    e.g. `e.contains("attach at least one complete agent")` or
    `e.contains("agent with provider/model")`. Document the choice
    in a brief inline comment.
  - All 6 `eval::postprocess::tests::extract_and_record_*` tests pass
    (3 currently failing + 3 currently passing). Fix the
    `finalized_run()` helper at line 234 so the tests' subsequent
    `store.finalize(...)` calls succeed.
  - Preferred fix path: rename the helper to `queued_run()` (or
    similar), drop the `status = RunStatus::Completed` and
    `completed_at = Some(...)` lines so it returns a `queued`
    `Run`, and let `store.finalize(...)` do the actual transition.
    Internally consistent and matches what the tests are testing
    (post-finalize behavior).
  - Alternative path (less preferred): keep the fixture, drop the
    `store.finalize(...)` calls from the three test methods. Document
    rationale if you pick this — the test name implies a finalize
    transition.
  - No changes to `crates/xvision-engine/src/eval/store.rs::finalize`
    — the production guard at line 243 (`WHERE status IN ('queued',
    'running')`) is correct and stays. The test was wrong.
  - No changes to `authoring::validate_draft` error message — it's
    the more readable phrasing. The test was wrong.
  - `cargo test -p xvision-engine --lib` runs clean OR the worker
    documents in the status note which remaining failures reproduce
    against the unmodified `origin/main` (with their WIP stashed)
    so reviewers can tell which were caused by this PR versus
    pre-existing.
  - No `try/catch` silencing or fallback shims
    (`feedback_alpha_root_cause`). The fix is to align the test with
    correct production behavior, not to weaken assertions.
parallel_safe: true
parallel_conflicts: []
---

# Scope

Four failing tests on `origin/main` discovered during the conductor's
post-round-4 follow-up check. All test-side drift, no production bug.

- `authoring::tests::validate_draft_reports_missing_agent_for_fresh_template`
  — test expects substring `"attached agent"`; actual error message
  is `"attach at least one complete agent with provider/model before
  validation"`. Update the test.
- `eval::postprocess::tests::extract_and_record_*` (3 tests) — fixture
  `finalized_run()` pre-sets `status = Completed`, then the tests
  call `store.finalize(...)` which now requires source status `queued
  | running` (guard added at some prior point in `eval/store.rs:243`).
  Fix the fixture.

Anchor reading:

- `team/intake/2026-05-19-test-drift-and-wiring.md` (full
  diagnosis).
- `crates/xvision-engine/src/authoring.rs:501` (the message) and
  `:826` (the assertion).
- `crates/xvision-engine/src/eval/postprocess.rs:234-251` (the
  fixture) and `:254+` (the three tests).
- `crates/xvision-engine/src/eval/store.rs:237-256` (the finalize
  guard — read-only; do not change).

# Out of scope

- The 4 pre-existing `crates/xvision-dashboard/tests/http.rs`
  scenario create/clone/eval_compare failures the round-4 workers
  flagged. Separate scoped track if they matter.
- Production semantics changes to `finalize` or the authoring error
  message.
- Renaming any production types/functions.
- Migration work.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-test-drift-2026-05-19 status
git -C .worktrees/qa-test-drift-2026-05-19 log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-test-drift-2026-05-19 \
  -b task/qa-test-drift-2026-05-19 origin/main
```

# Notes

Append checkpoints / PR links below.

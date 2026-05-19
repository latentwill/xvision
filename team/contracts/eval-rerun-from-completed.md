---
track: eval-rerun-from-completed
lane: integration
wave: qa-operator-2026-05-19
worktree: .worktrees/eval-rerun-from-completed
branch: task/eval-rerun-from-completed
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/mod.rs
  - crates/xvision-engine/src/eval/retry.rs
  - crates/xvision-engine/tests/eval_retry_from_completed.rs
  - crates/xvision-dashboard/src/routes/eval_runs.rs
  - crates/xvision-dashboard/tests/eval_runs_retry.rs
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail.test.tsx
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/eval/executor/**
  - frontend/web/src/features/eval-runs/review/**
interfaces_used:
  - eval::retry (engine entry point)
  - RunStatus (Completed | Failed | Cancelled | Queued | Running)
  - RetryEligibility predicate
  - Run summary status field on the frontend
verification:
  - cargo test -p xvision-engine --test eval_retry_from_completed
  - cargo test -p xvision-engine
  - cargo test -p xvision-dashboard --test eval_runs_retry
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run eval-runs-detail
acceptance:
  - Engine: `eval::retry` accepts source runs with `RunStatus::Completed`
    in addition to the currently-allowed `Failed | Cancelled` set.
    Existing fingerprint-based idempotency (a previous retry still
    queued or running) is preserved — a double-click on Rerun does
    NOT fan out new runs.
  - Engine: lineage for a rerun-of-completed records the source run id
    and a `RetryReason::ManualRerun` (or equivalent) distinct from the
    failed-run lineage marker, so downstream surfaces can tell a
    deliberate rerun from a failure-recovery retry.
  - Dashboard: `POST /api/eval/runs/:id/retry` returns `202 Accepted`
    with the freshly-persisted `RunDetail` (status = Queued) on a
    completed source run. The route's doc-comment is re-documented to
    list the updated 400 condition set (source must be in
    `failed | cancelled | completed`).
  - Frontend: `canRetry` in
    `frontend/web/src/routes/eval-runs-detail.tsx:368` widens to
    include `"completed"`. The button label adapts: "Retry" when
    source status is `failed | cancelled`, "Rerun" when source status
    is `completed`. A tooltip on the button distinguishes the two
    semantics ("Rerun: produces a fresh trace against the same
    agent/scenario inputs. Useful for re-testing a fix or
    verifying result stability.").
  - Frontend: clicking Rerun on a completed run optimistically updates
    the UI to the queued state and refetches; on error, surfaces the
    classified `DashboardError` as a toast (per #256 convention).
  - Regression test (engine): rerun-of-completed produces a new
    distinct run id with the same `(agent_id, scenario_id, mode,
    params_override)` inputs; status starts at Queued; lineage points
    back to the source.
  - Regression test (engine): double-rerun-while-queued is idempotent
    — second request returns the in-flight queued run id, does not
    enqueue a third row.
  - Regression test (dashboard): the route returns 202 on completed,
    400 on Queued / Running source, with classified error bodies.
  - Regression test (frontend): canRetry-completed renders "Rerun"
    label; canRetry-failed renders "Retry" label; canRetry-running
    renders no button.
  - No `try/catch` silencing (`feedback_alpha_root_cause`).
  - No migration. No changes to `eval/executor/**` — the queue/scheduler
    that picks up Queued rows is unchanged; this track only widens
    what's allowed to be enqueued.
parallel_safe: true
parallel_conflicts: []
---

# Scope

Today the eval retry route gates source status to `failed | cancelled`
(widened from `failed`-only by PR #260 `qa-eval-action-lifecycle`,
merged 2026-05-18). Operator wants to re-run a `completed` run too:
"re-test the same agent against the same scenario, get a fresh trace,
see if the result is stable". This is **not** A/B compare (different
agents/scenarios) and **not** a fingerprint-dedup case (the operator
explicitly wants a new run).

This track widens the engine + dashboard gate and adapts the frontend
button label/tooltip so the operator can distinguish "Retry" (a failed
run got fixed) from "Rerun" (a deliberate re-test of a completed run).

Anchor reading:

- `team/intake/2026-05-19-qa-operator-round-4.md` item 1.
- `crates/xvision-dashboard/src/routes/eval_runs.rs:109+` for the
  current retry route doc-comment and 400 gates.
- `frontend/web/src/routes/eval-runs-detail.tsx:368` for the current
  `canRetry` predicate.
- PR #260 (merged) widened the predicate from `failed`-only to
  `failed | cancelled`. This track is the third step in that
  progression.

# Out of scope

- Queue-level deduplication policy changes.
- Multi-run batching, lineage UI redesign beyond label + tooltip.
- A separate "Rerun all completed in this cohort" surface — single-run
  only for v1.
- Editing scenario/agent parameters as part of the rerun — that's
  A/B compare, which has its own surface (`xvn ab-compare --cycles`).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-rerun-from-completed status
git -C .worktrees/eval-rerun-from-completed log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-rerun-from-completed \
  -b task/eval-rerun-from-completed origin/main
```

# Notes

Append checkpoints / PR links below.

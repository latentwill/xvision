---
track: q15-eval-retry-button
worktree: .worktrees/q15-eval-retry-button
branch: task/q15-eval-retry-button
phase: pr-open
last_updated: 2026-05-16T15:30:00Z
owner: claude-opus
---

# What I'm doing right now

PR open. Adds `POST /api/eval/runs/:id/retry` plus a Retry button on
terminal failed runs.

Engine: new `eval::retry(ctx, source_id)` next to `eval::cancel` —
loads source, rejects non-Failed status with a Validation error, and
short-circuits to an existing in-flight run with the same
`(agent_id, scenario_id, mode)` fingerprint when one exists (the
idempotency clause). Otherwise calls `start_run` with cloned
`EvalRunRequest`.

Dashboard: `retry_run` handler returning `202 Accepted` with
`RunDetail`. Route registered in `server.rs`.

Frontend: `retryRun` client function with the same structured-logging
shape as `cancelRun`. Retry button on the run-detail summary card,
only visible when `summary.status === "failed"`. On success, navigate
to the new run's detail page if the returned id differs from the
current id (i.e., a new run actually started — not an idempotent hit).

# Blocked on

Nothing. Waiting on review.

# Next up

- Conductor merge.
- Conductor archives this contract per CONDUCTOR.md daily checklist.

# Notes

- Contract `allowed_paths` originally referenced
  `crates/xvision-dashboard/src/routes/eval/retry.rs` and
  `routes/eval/mod.rs` (which do not exist — dashboard routes live in
  flat files under `routes/`, not a subdirectory). Updated frontmatter
  to actual paths.
- The engine-side `eval::retry` is the public-API entry point; the
  HTTP handler is a thin wrapper that mirrors `cancel_run`. Tests
  cover all rejection paths (unknown, completed, running, cancelled)
  and the idempotency-on-in-flight-sibling case in both the engine
  unit tests and the dashboard HTTP integration tests.
- Frontend tests cover: button shows on failed, hides on
  completed/queued/running/cancelled, and clicking posts + navigates
  to the new run id.
- 4 dashboard tests in `tests/http.rs` are failing on `origin/main`
  unrelated to this change (`create_scenario_*`,
  `eval_compare_returns_report_for_seeded_runs`); reproduced before
  any of my changes. Not blocking this PR.

---
track: q15-eval-retry-button
lane: leaf
wave: q15
worktree: .worktrees/q15-eval-retry-button
branch: task/q15-eval-retry-button
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/api/eval.rs              # eval::retry function
  - crates/xvision-engine/tests/api_eval.rs            # retry unit tests
  - crates/xvision-dashboard/src/routes/eval_runs.rs   # retry_run handler
  - crates/xvision-dashboard/src/server.rs             # route registration only
  - crates/xvision-dashboard/tests/http.rs             # retry HTTP integration tests
  - frontend/web/src/api/eval.ts                       # retryRun client
  - frontend/web/src/routes/eval-runs-detail.tsx       # mount button
  - frontend/web/src/routes/eval-runs-detail.test.tsx  # button render + click tests
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/eval/executor/**
  - frontend/web/src/features/chat-rail/**
interfaces_used:
  - EvalRunStore::clone_run_inputs       # strategy_id, scenario_id, mode
  - POST /api/eval/runs                  # existing run-create endpoint
parallel_safe: false
parallel_conflicts:
  - q15-eval-json-export                 # both edit eval routes/mod.rs + eval-runs-detail.tsx
  - eval-review-api-cli
  - eval-review-run-detail-ui
verification:
  - cargo test -p xvision-engine --test api_eval retry
  - cargo test -p xvision-dashboard --test http eval_retry
  - corepack pnpm --dir frontend/web test -- eval-runs-detail
acceptance:
  - `POST /api/eval/runs/:id/retry` re-queues a new run with the same strategy/scenario/mode inputs as the source run and returns the new run id.
  - Retry endpoint is idempotent on a per-source-run basis only if the source's most recent retry is still queued/running (no infinite-retry storm).
  - Run-detail page shows a "Retry" button on terminal failed runs.
  - Retry button is hidden for completed-successful runs and queued/running runs.
  - Clicking Retry navigates to the new run's detail page.
---

# Scope

Fix QA15 item 3: failed eval runs need a Retry action that re-queues the
same run inputs. Useful when a failure was transient (provider 5xx,
network blip) or after fixing an upstream config issue.

# Out of scope

- Editing the run inputs as part of retry (Clone-to-edit is a separate
  flow; covered partially by `qa9-strategy-agent-attachment-flow`).
- Auto-retry on failure (operator-initiated only).
- Retry of cancelled runs (only failed; cancellation is intentional).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/q15-eval-retry-button -b task/q15-eval-retry-button origin/main
```

# Notes

- `eval-runs-detail.tsx` is on the conflict-zone list and shared with the
  eval-review UI track and `q15-eval-json-export`. Land in series, not
  parallel.
- 2026-05-16: contract `allowed_paths` were aspirational — there is no
  `routes/eval/` subdirectory in `xvision-dashboard`. Routes live in
  `routes/eval_runs.rs` (single file) registered in `server.rs`. The
  engine-side public API for retry lives in `crates/xvision-engine/src/api/eval.rs`
  next to `cancel` and `start_run`. Paths updated to reflect reality.
- Idempotency implemented as "no two retries of the same
  `(agent_id, scenario_id, mode)` fingerprint in flight at once." A
  source-run-id parent column would be cleaner but would need a migration —
  intentionally deferred. Returning the existing in-flight run keeps the
  client-side `useMutation` flow simple.
- PR: https://github.com/latentwill/xvision/pull/184 (opened, awaiting review).

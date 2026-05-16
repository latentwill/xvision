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
  - crates/xvision-dashboard/src/routes/eval/retry.rs
  - crates/xvision-dashboard/src/routes/eval/mod.rs    # route registration only
  - frontend/web/src/features/eval-runs/retry-button.tsx
  - frontend/web/src/routes/eval-runs-detail.tsx       # mount button only
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
  - cargo test -p xvision-dashboard eval::retry
  - corepack pnpm --dir frontend/web test -- eval-runs-detail retry
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
- PR: https://github.com/latentwill/xvision/pull/184 (opened, awaiting review).

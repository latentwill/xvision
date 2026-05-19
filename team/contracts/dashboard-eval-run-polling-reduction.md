---
track: dashboard-eval-run-polling-reduction
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/dashboard-eval-run-polling-reduction
branch: task/dashboard-eval-run-polling-reduction
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/sse/**                         # if SSE infra exists, extend it
  - crates/xvision-dashboard/src/routes/eval_runs.rs            # or wherever eval get_run is routed
  - crates/xvision-dashboard/src/state.rs                       # short-TTL cache for hot endpoints
  - frontend/web/src/features/eval-runs/**                      # poll/backoff client-side
  - crates/xvision-dashboard/tests/**
forbidden_paths:
  - crates/xvision-engine/**
  - crates/xvision-engine/migrations/**
interfaces_used:
  - xvision-dashboard::sse::* (existing — qa-trace-broker-spans / qa-retention-prompt-storage-bug touched this surface)
  - xvision-dashboard::routes::eval_runs::get_run
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-dashboard -- -D warnings
  - cargo test -p xvision-dashboard
  - pnpm --filter web exec tsc -p tsconfig.app.json --noEmit
  - pnpm --filter web exec vitest run src/features/eval-runs
acceptance:
  - The dashboard's frontend eval-run-detail polling switches from per-tick to **adaptive backoff with status-aware cadence**:
    * `running`: 2s tick.
    * `queued`: 5s tick.
    * Terminal (`completed`, `failed`, `cancelled`): stop polling after one final read.
    * Cap: never poll more often than 1s; back off to 30s after 5 minutes of no state change.
  - Backend cache: the route handler for `eval/get_run` (the audit found 890× calls vs 64 `start` calls in api_audit) introduces a small TTL'd cache keyed on `run_id` with a 500ms TTL. The cache is **bypassed** for terminal states (they don't change). Reduces DB round-trips for the polling tabs operators leave open.
  - **Optional**: if an SSE event-bus is already exposed for eval state-transition events (qa-trace-broker-spans wave), wire the eval-run-detail view to a stream so polling becomes a fallback rather than the primary mechanism. If no such stream exists yet, ship just the adaptive-poll + backend cache and document the SSE upgrade as a follow-up.
  - Tests:
    * Frontend: vitest snapshot of poll-interval state-machine transitions across (queued → running → completed) and the cap at 30s.
    * Backend: a `get_run` hit within 500ms of a prior identical request is served from cache (assert `recent` flag or use a test middleware to count DB round-trips).
    * Backend: terminal status bypasses cache (a `completed` run is re-fetched fresh — important for retry visibility).
  - No new migrations; no engine changes.
  - **Audit acceptance**: the 890:64 ratio of `eval.get_run` to `eval.start` in `api_audit` drops by ~10× under typical operator-watching workloads. Hot reload of an in-flight run still feels responsive (2s tick during running).
---

# Scope

Intake F-11 sub-bullet (api_audit polling reduction) of
`team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

Audit: `api_audit` showed `eval.get_run` called 890× in the observed
window vs 64 `eval.start` — the UI is polling per-tick on every open
eval-run detail tab. This contract adds adaptive backoff + a small
backend cache so casual operator-watching costs much less.

# Out of scope

- Full SSE migration for eval-run detail (documented as a follow-up if
  the SSE bus doesn't already cover this event class).
- Other endpoints' polling (just `eval.get_run` here).
- Engine-side changes.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/dashboard-eval-run-polling-reduction status
git -C .worktrees/dashboard-eval-run-polling-reduction log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/dashboard-eval-run-polling-reduction -b task/dashboard-eval-run-polling-reduction origin/main
```

# Notes

Keep the frontend logic centralised in the eval-runs feature module so
other polling sites can adopt the same hook (`useAdaptivePoll(runId,
status)`) later without copy-paste.

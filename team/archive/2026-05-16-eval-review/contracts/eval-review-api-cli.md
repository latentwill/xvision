---
track: eval-review-api-cli
lane: leaf
wave: eval-review
worktree: .worktrees/eval-review-api-cli
branch: task/eval-review-api-cli
base: origin/main
status: merged
pr: 188
depends_on:
  - eval-review-agent-engine
blocks:
  - eval-review-run-detail-ui
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/routes/eval/review.rs
  - crates/xvision-dashboard/src/routes/eval/mod.rs   # route registration only
  - crates/xvision-cli/src/commands/eval/review.rs
  - crates/xvision-cli/src/commands/eval/mod.rs       # subcommand registration only
forbidden_paths:
  - crates/xvision-engine/src/eval/review/**          # engine surface frozen at this point
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - EvalReviewService::generate
  - EvalReviewService::list_for_run
  - EvalReviewService::get
  - JsonOutputContract                                 # established in qa8-cli-json-contracts
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-dashboard eval::review
  - cargo test -p xvision-cli eval::review
acceptance:
  - `POST /api/eval/runs/:id/review` accepts `{ agent_profile_id, force? }` and returns the persisted review.
  - `GET /api/eval/runs/:id/reviews` returns reviews ordered newest-first.
  - `GET /api/eval/reviews/:id` returns a single review with normalized findings.
  - `xvn eval review <run_id> --agent <profile> [--force] [--output review.json]` runs through the dashboard binary or local engine, prints human-readable summary by default, emits stable JSON with `--output` or `--format json`.
  - Errors surface as typed exit codes; missing run / missing agent profile → distinct codes.
---

# Scope

Expose the engine review service through the dashboard API and the `xvn`
CLI. Route shape matches existing eval routes (`/api/eval/runs/:id/...`).
CLI JSON contract follows the existing `--format json` convention.

# Out of scope

- Engine-side review payload assembly or LLM dispatch (`eval-review-agent-engine`).
- Run-detail UI panel (`eval-review-run-detail-ui`).
- Streaming SSE for in-flight reviews — deferred to a later track.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/eval-review-api-cli -b task/eval-review-api-cli origin/main
```

# Notes

- Reuse the JSON serialization helpers from the qa8 CLI work; do not invent
  a new contract shape.
- The CLI must work both when xvn is in remote-CLI mode and when running
  locally against the embedded store.

- PR: https://github.com/latentwill/xvision/pull/188 (merged 2026-05-16).

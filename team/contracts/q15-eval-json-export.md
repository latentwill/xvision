---
track: q15-eval-json-export
lane: foundation
wave: q15
worktree: .worktrees/q15-eval-json-export
branch: task/q15-eval-json-export
base: origin/main
status: in-progress
depends_on: []
blocks:
  - q15-object-json-output             # consumer of per-object shapes standardized here
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/export.rs
  - crates/xvision-engine/src/eval/store.rs            # read-only helpers only
  - crates/xvision-dashboard/src/routes/eval/export.rs
  - crates/xvision-dashboard/src/routes/eval/mod.rs    # route registration only
  - crates/xvision-cli/src/commands/eval/export.rs
  - crates/xvision-cli/src/commands/eval/mod.rs        # subcommand registration only
  - frontend/web/src/features/eval-runs/export/**
  - frontend/web/src/routes/eval-runs-detail.tsx       # add Download JSON button
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/eval/executor/**         # do not change execution
  - frontend/web/src/features/chat-rail/**
interfaces_used:
  - EvalRunStore::load_run
  - EvalRunStore::load_decisions
  - EvalRunStore::load_equity_samples
  - EvalRunStore::load_events
  - EvalRunStore::load_errors
  - EvalReviewService::list_for_run
parallel_safe: false
parallel_conflicts:
  - eval-review-api-cli               # both edit eval routes/CLI registration
  - eval-review-run-detail-ui         # both edit eval-runs-detail.tsx
verification:
  - cargo test -p xvision-engine eval::export::roundtrip
  - cargo test -p xvision-dashboard eval::export
  - cargo test -p xvision-cli eval::export
  - corepack pnpm --dir frontend/web test -- eval-runs-detail
acceptance:
  - `EvalRunExport` struct matches the spec §3 shape, `schema_version: "1"`.
  - `GET /api/eval/runs/:id/export` returns the export as `application/json`.
  - `xvn eval export <run_id>` writes the same bytes to stdout; `--output run.json` writes to file.
  - Run-detail UI shows a "Download JSON" button on terminal runs.
  - Round-trip canary test: export → parse → all top-level keys present, `decisions[].ix` is contiguous.
  - QA15 reproducer run exports cleanly and the JSON contains the truncation diagnostics under `provider_diagnostics`.
---

# Scope

Defines and ships the full `EvalRunExport` JSON contract for completed eval
runs per spec §3. Anchors the shared per-object shapes used by
`q15-object-json-output`.

# Out of scope

- Object-level JSON for `strategy` / `scenario` / `agent` (`q15-object-json-output`).
- Streaming export for in-flight runs.
- Binary attachments.
- Any change to execution behavior — read-only over the store.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/q15-eval-json-export -b task/q15-eval-json-export origin/main
```

# Notes

- Coordinate with `eval-review-api-cli` and `eval-review-run-detail-ui` —
  they share the eval route registration files and eval-runs-detail.tsx.
- Pin `schema_version: "1"` from day one. Future breaking changes bump it.

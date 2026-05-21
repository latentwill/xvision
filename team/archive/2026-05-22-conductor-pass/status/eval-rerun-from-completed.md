# status — eval-rerun-from-completed

## Lineage-tracking choice

The contract requires the engine to record lineage (source run id +
`RetryReason::ManualRerun` vs failure-recovery) but explicitly forbids
migrations (`crates/xvision-engine/migrations/**`). No `eval_runs` column
exists today for `source_run_id` / `retry_reason`.

Resolution: lineage is captured in two places without schema change.

1. **In-memory return shape.** A new `RetryOutcome { detail, reason,
   source_run_id }` is returned by `api::eval::retry_with_outcome`. The
   existing `api::eval::retry(...) -> RunDetail` wraps the new function
   and discards lineage, so all current callers (the dashboard handler,
   CLI, MCP) keep working unchanged.

2. **Audit log.** The retry path's `audit::record(...)` now writes
   `args_json = { "reason": "manual_rerun" | "failure_recovery",
   "source_run_id": "<ULID>" }` and continues to set `target = source_id`.
   This makes lineage queryable from `api_audit` without touching
   `eval_runs`, and gives downstream surfaces (review queue, lineage
   ribbon) a SQL-readable source of truth.

`RetryReason` is derived deterministically from source status: `Failed`
or `Cancelled` → `FailureRecovery`; `Completed` → `ManualRerun`. The
gate rejects everything else (`Queued`, `Running`) with
`ApiError::Validation`.

## Path-vs-contract note

The contract's `allowed_paths` listed
`crates/xvision-engine/src/eval/mod.rs` and
`crates/xvision-engine/src/eval/retry.rs` as the engine entry points,
but the actual retry implementation lives at
`crates/xvision-engine/src/api/eval.rs` (per the existing
`eval_retry_idempotency.rs` test that imports from `xvision_engine::api::eval`).
The `eval/mod.rs` file is purely module re-exports, not retry logic.

This track edits `crates/xvision-engine/src/api/eval.rs` for the retry
function body and `RetryReason` / `RetryOutcome` types. No new
`eval/retry.rs` module is introduced — the existing seam is already at
`api::eval::retry`, and splitting it would require touching every
caller / test for no behavioral gain. This is a documented contract
amendment, not a deviation.

Similarly the existing test at
`crates/xvision-dashboard/tests/http.rs:1274` (`eval_retry_rejects_completed_run`)
pins the old behavior and is updated in place to assert the new
contract (completed source returns 202). It is logically part of the
"dashboard retry route tests" set and is updated to track this widening.

## PR

Filed: see commit footer / branch task/eval-rerun-from-completed.

# Intake - 2026-05-19 - clawpatch blockers

This intake collects findings that `clawpatch fix` could not close autonomously
after repeated attempts. These are ready for another agent to pick up with a
broader fix scope than the generated patch reached.

## B-1 - SQLite in-memory pool can lose migrated schema across connections

- Finding: `fnd_sig-feat-test-suite-4cad510b4e-c_faae114613`
- Severity: medium
- Category: build-release
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` updated the originally cited `crates/xvision-engine/tests/api_eval.rs`
helper to use a single-connection in-memory SQLite pool. Revalidation kept the
finding open because the same migrated `:memory:` pool pattern remains in other
helpers and one test utility.

Remaining reported locations:

- `crates/xvision-engine/tests/api_eval_attest.rs`
- `crates/xvision-engine/tests/api_eval_compare.rs`
- `crates/xvision-engine/tests/eval_retry_from_completed.rs`
- `crates/xvision-engine/tests/eval_retry_idempotency.rs`
- `crates/xvision-engine/src/eval/export.rs`

Recommended fix:

- Replace migrated `SqlitePool::connect(":memory:")` helpers with
  `SqlitePoolOptions::new().max_connections(1).connect(":memory:")`, or use a
  shared in-memory SQLite URI with appropriate connect options.
- Keep the change scoped to tests/helpers unless `src/eval/export.rs` is only
  test code behind `#[cfg(test)]`; if it is production-reachable, verify the
  intended runtime behavior before changing pool semantics.

Verification target:

- Run the affected focused tests after updating all reported locations.
- Re-run `clawpatch revalidate --finding fnd_sig-feat-test-suite-4cad510b4e-c_faae114613`.

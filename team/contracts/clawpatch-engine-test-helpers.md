---
track: clawpatch-engine-test-helpers
lane: leaf
wave: clawpatch-blockers-2026-05-21
worktree: .worktrees/clawpatch-engine-test-helpers
branch: task/clawpatch-engine-test-helpers
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/tests/api_eval.rs                        # B-1/B-2 — still has naked SqlitePool::connect(":memory:")
  - crates/xvision-engine/tests/api_eval_attest.rs                 # B-1/B-2 — already fixed; re-verify
  - crates/xvision-engine/tests/api_eval_compare.rs                # B-1/B-2 — already fixed; re-verify
  - crates/xvision-engine/tests/eval_retry_from_completed.rs       # B-1/B-2 — already fixed; re-verify
  - crates/xvision-engine/tests/eval_retry_idempotency.rs          # B-1/B-2 — already fixed; re-verify
  - crates/xvision-engine/src/eval/export.rs                       # B-1 — already fixed; re-verify and confirm it's test-reachable only
  - crates/xvision-engine/src/eval/store.rs                        # B-3 — if `pool_with_migration` lives here, update to single-connection or shared-cache pattern
  - crates/xvision-observability/tests/janitor.rs                  # B-4 — staggered mtimes on max_bytes_evicts_oldest_until_under_cap
forbidden_paths:
  - crates/xvision-engine/migrations/**                            # no schema changes
  - crates/xvision-engine/src/eval/**                              # except export.rs and the pool helper in store.rs — no broader logic edits
  - crates/xvision-observability/src/**                            # tests-only
  - frontend/web/**                                                # frontend findings are in clawpatch-frontend-components
interfaces_used:
  - sqlx::SqlitePool                                               # the actual fix surface
  - sqlx::sqlite::SqlitePoolOptions                                # the correct constructor
  - std::fs::File::set_modified                                    # for B-4 mtime determinism
verification:
  - cargo test -p xvision-engine api_eval --no-fail-fast
  - cargo test -p xvision-engine api_eval_attest
  - cargo test -p xvision-engine api_eval_compare
  - cargo test -p xvision-engine eval_retry_from_completed
  - cargo test -p xvision-engine eval_retry_idempotency
  - cargo test -p xvision-observability max_bytes
  - cargo test --workspace
acceptance:
  - **B-1/B-2 closed.** Every test helper that builds an in-memory SQLite pool uses `SqlitePoolOptions::new().max_connections(1).connect(":memory:")` (or a shared-cache URI). No naked `SqlitePool::connect(":memory:")` calls remain in `crates/xvision-engine/tests/*.rs` or `crates/xvision-engine/src/eval/export.rs`. Recon (2026-05-21) confirms only `api_eval.rs:10` still has the bug.
  - **B-3 closed.** The shared `pool_with_migration` helper (or whatever the test-side pool factory is named in `crates/xvision-engine/src/eval/store.rs` or its test-bin module) builds a single-connection pool. Worker confirms there's no path where a different connection from the pool sees an unmigrated schema. If the helper is per-test inline, this acceptance is satisfied by B-1/B-2; document the finding.
  - **B-4 closed.** `max_bytes_evicts_oldest_until_under_cap` in `crates/xvision-observability/tests/janitor.rs` explicitly assigns staggered mtimes via `file.set_modified(...)` before calling `truncate_to_max_bytes` so the eviction-order assertion is deterministic. Test passes on the existing tie-break-uses-sha test invariants. Recon: `set_modified` is already used at `:295`; confirm the actual test correctly orders the three blobs (a older than b and c).
  - **Verify on origin** that the findings clawpatch revalidate-checks still close — worker runs `clawpatch revalidate --finding <id>` for each of B-1/B-2/B-3/B-4 (4 ids in the intake) and confirms they flip to "closed" (or surfaces the new gap).
  - **No production behavior change.** Changes scoped to tests + the export helper. `cargo test --workspace` green.
  - **No `#[ignore]` or commented-out tests.** If a test cannot be repaired, file a follow-up contract via contract-update PR; don't silence.

---

# Scope

Track #1 of `team/intake/2026-05-19-clawpatch-blockers.md` (engine/observability subset).
Covers four findings (B-1, B-2, B-3, B-4) that all share the same
root cause: in-memory SQLite pools whose multiple connections see
divergent schema (or, in B-4's case, non-deterministic mtime ordering).
Clawpatch's autonomous loop closed some occurrences but couldn't
finish; this track does the last-mile sweep.

Recon (2026-05-21):

- B-1/B-2: Only `api_eval.rs:10` still has `SqlitePool::connect(":memory:")`.
  Five other reported locations are already on `.max_connections(1)`.
- B-3: Need to locate `pool_with_migration` — likely lives in
  `crates/xvision-engine/src/eval/store.rs` or a test-side helper
  module. Worker confirms during sync-before-work.
- B-4: `set_modified` is already imported in `janitor.rs:295`; the
  remaining gap is whether the specific test orders the three blobs
  correctly. Likely a one-line tweak.

# Out of scope

- Schema migrations.
- Broader executor or store refactors.
- The CLI assertion (B-5) — handled by `clawpatch-cli-test-assert`.
- Frontend findings (B-6 through B-11) — handled by
  `clawpatch-frontend-components`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/clawpatch-engine-test-helpers status
git -C .worktrees/clawpatch-engine-test-helpers log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/clawpatch-engine-test-helpers -b task/clawpatch-engine-test-helpers origin/main
```

# Notes

Per CLAUDE.md "alpha: fix root cause" rule — don't catch-and-suppress
the failure pattern. The right fix is enforcing single-connection
pool semantics consistently, not a try/retry shim.

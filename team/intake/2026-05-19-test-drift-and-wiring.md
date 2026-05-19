# Intake — 2026-05-19 — Test-drift cleanup + RunSummary wiring

Discovered during the conductor's post-round-4 follow-up verification.
Two unrelated tracks; both small and unblocked.

## Source

Conductor verification 2026-05-19 after round-4 PRs (#317, #319, #320,
#322, #325) merged. Five of seven round-4 worker agents independently
flagged "pre-existing failures on origin/main" in their PR bodies.
Investigation found:

- The `agent_slots.prompt_version` migration drift the workers
  reported is already resolved on current main — all 7
  `agents::store::tests` pass clean.
- Four genuine test failures remain, all caused by test-side drift,
  not production bugs.
- The `RunSummary.tsx` component shipped by PR #320 lives on main but
  isn't yet wired into `routes/eval-runs-detail.tsx` because that file
  was outside #320's `allowed_paths`.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| 1 | P2 | `authoring::tests::validate_draft_reports_missing_agent_for_fresh_template` fails on main. Test expects the substring `"attached agent"` in the error message, but the actual message at `crates/xvision-engine/src/authoring.rs:501` is `"strategy is not eval-ready: attach at least one complete agent with provider/model before validation"`. Pure string drift — production code is correct. | `qa-test-drift-2026-05-19` |
| 2 | P2 | 3× `eval::postprocess::tests::extract_and_record_*` fail with `finalize: run '...' is already completed`. Root cause: the `finalized_run()` test helper at `crates/xvision-engine/src/eval/postprocess.rs:234` constructs a `Run` with `status = Completed`, then the tests call `store.finalize(...)` which (per `crates/xvision-engine/src/eval/store.rs:243`) only accepts source status `queued | running`. The fixture name is misleading and the test logic is internally contradictory. | `qa-test-drift-2026-05-19` (same track) |
| 3 | P2 | `RunSummary.tsx` (shipped by PR #320) is not yet rendered. `routes/eval-runs-detail.tsx` only imports the `RunSummary` TypeScript **type** from `@/api/types.gen`, not the new component from `@/features/eval-runs/RunSummary`. The repeated-broker-error classified banner therefore never displays even when the failure reason is present. | `wire-run-summary-into-eval-detail` |

Two tracks, both small (<200 lines each), both unblocked.

## Track summaries

### `qa-test-drift-2026-05-19` (P2, leaf)

Four failing tests on `origin/main`, all test-side drift. Production
code is correct in both cases.

**Authoring test** — `crates/xvision-engine/src/authoring.rs:826`:

```rust
assert!(
    v.errors.iter().any(|e| e.contains("attached agent")),
    "expected missing attached agent error, got {:?}",
    v.errors,
);
```

But the message emitted at line 501 is `"attach at least one complete
agent with provider/model before validation"` — no "attached agent"
substring. The message wording is the more readable of the two; update
the test to match (e.g. `e.contains("attach at least one complete agent")`
or `e.contains("agent with provider/model")`).

**Postprocess tests** — `crates/xvision-engine/src/eval/postprocess.rs:234-251`:

```rust
fn finalized_run() -> Run {
    let mut r = Run::new_queued(...);
    r.status = RunStatus::Completed;  // ← pre-set to completed
    r.completed_at = Some(Utc::now());
    ...
}

#[tokio::test]
async fn extract_and_record_persists_findings_and_indexes_them() {
    ...
    let run = finalized_run();
    store.create(&run).await.unwrap();  // ← inserts row with status='completed'
    store
        .finalize(&run.id, run.metrics.as_ref().unwrap())  // ← fails
        .await
        .unwrap();
    ...
}
```

Finalize at `eval/store.rs:237-256` is correctly stricter:

```rust
"UPDATE eval_runs SET status = 'completed', ... \
 WHERE id = ? AND status IN ('queued', 'running')"
```

`rows_affected() == 0` → `bail!("finalize: run '{id}' is already {}", ...)`.

Two fix paths:
1. **Preferred:** rename `finalized_run()` → `queued_run()` (or
   similar), drop the `status = Completed` + `completed_at` lines,
   and let `store.finalize(...)` do the transition. Internally
   consistent and matches what the test is testing
   (post-finalize behavior).
2. Alternative: keep the helper, drop the `store.finalize(...)` call
   from the three tests since the run is already finalized by the
   fixture. Less clean — the test name implies a finalize transition
   happened.

Path 1 is cleaner. Worker decides.

Scope:
- `crates/xvision-engine/src/authoring.rs` — update test assertion
  string (test code is in the same file under `#[cfg(test)] mod`).
- `crates/xvision-engine/src/eval/postprocess.rs` — fix the
  `finalized_run()` helper + the three test methods that depend on it.

Acceptance:
- `cargo test -p xvision-engine --lib authoring::tests::validate_draft_reports_missing_agent_for_fresh_template` → passes.
- `cargo test -p xvision-engine --lib eval::postprocess::tests` → all 6 tests pass (3 currently failing + 3 currently passing).
- `cargo test -p xvision-engine` → full suite clean (modulo any
  separate unrelated failures the worker confirms reproduce against
  origin/main with their WIP stashed).

Out of scope:
- The 4 pre-existing `xvision-dashboard` `http.rs` failures the round-4
  workers flagged (scenario create/clone/eval_compare). Those need a
  separate scoped track if they matter.
- Production code changes to `finalize` semantics or the
  authoring error message — both are correct as-is.

### `wire-run-summary-into-eval-detail` (P2, leaf)

One-file frontend follow-up to PR #320. The `RunSummary` component at
`frontend/web/src/features/eval-runs/RunSummary.tsx` (shipped by #320)
parses the `[repeated_broker_error]` prefix produced by
`format_failure_reason` and renders a human-readable one-liner above
the raw error text. It's currently dead code.

`routes/eval-runs-detail.tsx` has an inline failure-rendering block
(the pre-#320 pattern) that needs to be replaced with a single
`<RunSummary error={...} />` invocation. The inline block lives in the
failed/cancelled rendering path; identify the exact location by
following the existing `RunSummary` (type) usages around line 363+.

Scope:
- `frontend/web/src/routes/eval-runs-detail.tsx` — import the
  component, swap the inline error block for it.

Acceptance:
- `pnpm --dir frontend/web typecheck` → clean.
- `pnpm --dir frontend/web test -- --run eval-runs-detail` → all
  existing tests pass; the existing inline-error test (if any) is
  updated or removed to test against the new component.
- `pnpm --dir frontend/web build` → clean.
- A failed run with the `[repeated_broker_error]` prefix renders the
  classified banner; a failed run without the prefix renders the
  legacy red code-block (no regression).
- Mobile route `eval-runs-detail-mobile.tsx` is also wired if the
  same pattern is present there (touch only if needed and only the
  failure-rendering block).

Out of scope:
- Changes to `RunSummary.tsx` itself.
- Other failure-classification UI work.

## Verbatim findings

> [conductor verification 2026-05-19, post-round-4 follow-up check]
> authoring::tests::validate_draft_reports_missing_agent_for_fresh_template
>   panicked at crates/xvision-engine/src/authoring.rs:826:9:
>   expected missing attached agent error, got
>   ["strategy is not eval-ready: attach at least one complete agent
>   with provider/model before validation"]
>
> eval::postprocess::tests::extract_and_record_persists_findings_and_indexes_them
>   panicked at crates/xvision-engine/src/eval/postprocess.rs:262:14:
>   called `Result::unwrap()` on an `Err` value:
>   finalize: run '01KRZEMNVYN0NG54224CRT7G78' is already completed

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

## B-2 - In-memory SQLite pool can create isolated databases per connection

- Finding: `fnd_sig-feat-test-suite-806b2ebb52-5_501dd10586`
- Severity: medium
- Category: build-release
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` updated the originally cited
`crates/xvision-engine/tests/api_eval_attest.rs` helper to use
`SqlitePoolOptions::new().max_connections(1).connect(":memory:")`.
Revalidation kept the finding open because the same migrated `:memory:` pool
pattern remains in other eval test harness files.

Remaining reported locations:

- `crates/xvision-engine/tests/api_eval.rs`
- `crates/xvision-engine/tests/api_eval_compare.rs`
- `crates/xvision-engine/tests/eval_retry_from_completed.rs`
- `crates/xvision-engine/tests/eval_retry_idempotency.rs`

Recommended fix:

- Replace migrated `SqlitePool::connect(":memory:")` helpers with
  `SqlitePoolOptions::new().max_connections(1).connect(":memory:")`, or use a
  shared-cache/file-backed temporary SQLite database for integration tests.
- Keep the existing tests' assertions unchanged unless a helper API change is
  needed to centralize the single-connection pool setup.

Verification target:

- Run the affected focused eval tests after updating all reported locations.
- Re-run `clawpatch revalidate --finding fnd_sig-feat-test-suite-806b2ebb52-5_501dd10586`.

## B-3 - In-memory SQLite pool can route store calls to an unmigrated database

- Finding: `fnd_sig-feat-test-suite-972b03ea5d-3_f709e3cc62`
- Severity: medium
- Category: build-release
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` did not close this finding after repeated attempts. Revalidation continued to report the issue as open, so this needs a broader manual pass by another agent.

Recommendation from clawpatch:

- Build the test pool with a single connection, for example SqlitePoolOptions::new().max_connections(1).connect(":memory:").await, or use a unique temporary SQLite file/shared-cache URI so every pooled connection sees the same migrated schema.

Minimum fix scope from clawpatch:

- Change pool_with_migration to create a single-connection in-memory pool or a shared/file-backed test database.

Verification target:

- Re-run `clawpatch revalidate --finding fnd_sig-feat-test-suite-972b03ea5d-3_f709e3cc62` after the broader fix.

## B-4 - Janitor oldest-blob test still fails validation after generated mtime fix

- Finding: `fnd_sig-feat-test-suite-a7b7e8d445-6_387eaf6db5`
- Severity: low
- Category: build-release
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` attempted to make `max_bytes_evicts_oldest_until_under_cap`
assign deterministic mtimes in `crates/xvision-observability/tests/janitor.rs`,
but the generated patch failed clawpatch's validation after applying. The failed
generated hunk was removed so the autonomous loop could continue from a clean
tree.

Recommendation from clawpatch:

- Set deterministic mtimes before calling `truncate_to_max_bytes`, with `a`
  older than `b` and `c`, or relax this test to only assert the cap and deletion
  count while leaving tie-break specifics to
  `max_bytes_tie_break_uses_sha_hex_when_mtimes_equal`.

Minimum fix scope from clawpatch:

- Make `max_bytes_evicts_oldest_until_under_cap` explicitly assign staggered
  mtimes to its three blobs.

Verification target:

- Re-run `clawpatch revalidate --finding fnd_sig-feat-test-suite-a7b7e8d445-6_387eaf6db5`
  after a broader fix.

## B-5 - CLI eval export stdout assertion patch fails validation

- Finding: `fnd_sig-feat-test-suite-bb1a90129a-9_8b36947666`
- Severity: low
- Category: test-gap
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` attempted to add a `cli_out.stdout.is_empty()` assertion to
`crates/xvision-cli/tests/eval_export_cli.rs`, but the generated patch failed
clawpatch's validation after applying. The failed generated hunk was removed so
the autonomous loop could continue from a clean tree.

Recommendation from clawpatch:

- Add an assertion such as
  `assert!(cli_out.stdout.is_empty(), "stdout: {}", String::from_utf8_lossy(&cli_out.stdout));`
  before validating stderr.

Minimum fix scope from clawpatch:

- Add one assertion in `crates/xvision-cli/tests/eval_export_cli.rs`.

Verification target:

- Re-run `clawpatch revalidate --finding fnd_sig-feat-test-suite-bb1a90129a-9_8b36947666`
  after a broader fix.

## B-6 - HealthPill generated component tests fail validation

- Finding: `fnd_sig-feat-ui-flow-368150e279-c2d1_425d678994`
- Severity: low
- Category: test-gap
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` attempted to add
`frontend/web/src/components/shell/HealthPill.test.tsx`, but the generated
test file failed clawpatch's validation after applying. The untracked generated
test file was removed so the autonomous loop could continue from a clean tree.

Recommendation from clawpatch:

- Add focused HealthPill tests that render the component under
  `QueryClientProvider` with mocked `getHealth` responses for pending/loading,
  rejected/offline, ok, degraded, and down states, including the title summary
  built from probes.

Minimum fix scope from clawpatch:

- Add HealthPill-specific component tests; no production code change is
  required for this finding.

Verification target:

- Re-run `clawpatch revalidate --finding fnd_sig-feat-ui-flow-368150e279-c2d1_425d678994`
  after a broader fix.

## B-7 - CacheStatusBadge generated component tests fail validation

- Finding: `fnd_sig-feat-ui-flow-7f1ddf7f4e-f0c9_fc3d85213c`
- Severity: low
- Category: test-gap
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` attempted to add
`frontend/web/src/components/scenario/CacheStatusBadge.test.tsx`, but the
generated test file failed clawpatch's validation after applying. The untracked
generated test file was removed so the autonomous loop could continue from a
clean tree.

Recommendation from clawpatch:

- Add focused React Testing Library tests for `CacheStatusBadge` covering the
  three status variants, button rendering/click behavior, disabled state, and
  fetchStatus-only rendering when `onFetch` is absent.

Minimum fix scope from clawpatch:

- Add a component-level test file under
  `frontend/web/src/components/scenario` and include it in the existing
  `pnpm --dir frontend/web test` suite.

Verification target:

- Re-run `clawpatch revalidate --finding fnd_sig-feat-ui-flow-7f1ddf7f4e-f0c9_fc3d85213c`
  after a broader fix.

## B-8 - AgentForm duplicate-slot generated fix fails validation

- Finding: `fnd_sig-feat-ui-flow-98a40b66c8-6d2a_48f373bf7f`
- Severity: low
- Category: api-contract
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` attempted to change `AgentForm.duplicateSlot` so copied slots
set `max_tokens: null`, and added an `agents.test.tsx` regression assertion.
The generated patch failed clawpatch's validation after applying. The failed
generated code and test hunks were removed so the autonomous loop could continue
from a clean tree.

Recommendation from clawpatch:

- When duplicating a slot, explicitly set `max_tokens: null` on the new slot
  while copying the rest of the editable fields.

Minimum fix scope from clawpatch:

- `AgentForm.duplicateSlot` payload construction.

Verification target:

- Re-run `clawpatch revalidate --finding fnd_sig-feat-ui-flow-98a40b66c8-6d2a_48f373bf7f`
  after a broader fix.

## B-9 - WizardPreviewChart memoization generated fix fails validation

- Finding: `fnd_sig-feat-ui-flow-f276b9b4f5-53e4_89387de97b`
- Severity: low
- Category: performance
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` attempted to memoize the synthesized
`ScenarioChartPayload` in `WizardPreviewChart.tsx` and added
`WizardPreviewChart.test.tsx`, but the generated patch failed clawpatch's
validation after applying. The failed generated code and test file were removed
so the autonomous loop could continue from a clean tree.

Recommendation from clawpatch:

- Memoize the synthesized `ScenarioChartPayload` with `useMemo` keyed by
  `query.data`, debounced `asset/from/to/granularity`, and cache status inputs,
  and use a stable placeholder `created_at` value for preview payloads.

Minimum fix scope from clawpatch:

- `frontend/web/src/components/chart/WizardPreviewChart.tsx`.

Verification target:

- Re-run `clawpatch revalidate --finding fnd_sig-feat-ui-flow-f276b9b4f5-53e4_89387de97b`
  after a broader fix.

## B-10 - SlotForm provider-change generated fix fails validation

- Finding: `fnd_sig-feat-ui-flow-0e07bcd326-2bbe_8ce24d101a`
- Severity: medium
- Category: bug
- Status: open in codebase, deferred from the autonomous clawpatch loop

`clawpatch fix` attempted to clear `slot.model` when the provider select changes
to a provider that does not offer the current model, and added focused SlotForm
coverage in `agents.test.tsx`. The generated patch failed clawpatch's
validation after applying. The failed generated code and test hunks were removed
so the autonomous loop could continue from a clean tree.

Recommendation from clawpatch:

- When the provider changes, clear `model` unless the current model is valid for
  the newly selected provider, or route provider changes through the same
  provider/model update path used by `ModelPicker`.

Minimum fix scope from clawpatch:

- Update `SlotForm` provider select handling and add a focused SlotForm
  interaction test.

Verification target:

- Re-run `clawpatch revalidate --finding fnd_sig-feat-ui-flow-0e07bcd326-2bbe_8ce24d101a`
  after a broader fix.

# Status: clawpatch-v2e-deslop-followup

**Track:** clawpatch-v2e-deslop-followup
**Branch:** task/clawpatch-v2e-deslop-followup
**Status:** complete — medium findings fixed; high-leverage low cleanup folded in
**Updated:** 2026-05-21

## Scope

Follow-up to the V2E post-merge `clawpatch review --mode deslopify --jobs 2`
run (`20260521T034713-935c1e`). The pass reported no behavior/security/API
blockers, but flagged four medium maintainability findings where tests or UI
code duplicated implementation grammar/formulas. The low-severity set was
mostly duplicated eval test setup, so this PR also centralizes the largest
shared setup paths without refactoring migration-delta tests.

## What changed

- Removed test-local `VolumeShare` formula copies from:
  - `crates/xvision-engine/tests/cost_model_volume_share.rs`
  - `crates/xvision-engine/tests/cost_model_slippage_sign.rs`
- Removed tautological fee arithmetic tests from
  `crates/xvision-engine/tests/cost_model_fee_accuracy.rs`; kept serde/default
  wire-shape coverage.
- Removed the synthetic rolling-window copy from
  `crates/xvision-engine/tests/eval_prompt_cache_and_rolling_window.rs`; kept
  request-body cache-control coverage.
- Simplified `ScenarioForm` granularity handling to the fixed UI select palette
  instead of mirroring backend `BarGranularity` aliases in the form component.
- Added `crates/xvision-engine/tests/common/mod.rs` for reusable eval API test
  contexts:
  - fully migrated `ApiContext::open` context for schema/registry-aware tests
  - legacy eval-run context for fallback-path tests that intentionally omit the
    scenario FK migration
  - seeded scenario id lookup for tests that create `eval_runs` under the fully
    migrated schema
- Rewired `api_eval`, `api_eval_compare`, `api_eval_min_notional`,
  `api_eval_run`, and `eval_store` off duplicated local migration setup.

## Clawpatch revalidation

- `fnd_sig-feat-test-suite-0256828dd5-b_b632272e99` — fixed
- `fnd_sig-feat-test-suite-721e4a409f-6_bf804bb7e0` — fixed
- `fnd_sig-feat-test-suite-bf181a6341-3_2ba960b5d2` — fixed
- `fnd_sig-feat-ui-flow-4785eb0be5-db37_bfa8be3e60` — fixed

`clawpatch report --status open --severity medium` now returns zero findings.

Low findings addressed by the shared eval test helper:

- `fnd_sig-feat-test-suite-4cad510b4e-2_aac417094d`
- `fnd_sig-feat-test-suite-dc01cf438a-e_753303fff4`
- `fnd_sig-feat-test-suite-c01cdee5e1-f_81466a09c6`
- `fnd_sig-feat-test-suite-972b03ea5d-c_5abed1d969`

Residual low finding left open:

- `fnd_sig-feat-test-suite-ef14f8b222-a_a976b9f6da` — the duplicated local
  helpers in `api_eval_run.rs` are removed, but clawpatch correctly still sees
  the new shared legacy helper as a hand-maintained migration subset. That
  helper is intentionally scoped to tests that exercise legacy scenario
  fallback without the scenario FK migration; moving those to `ApiContext::open`
  changes the contract and should be handled as a separate behavior cleanup.

## Verification

- `cargo fmt --all --check` — pass
- `cargo test -p xvision-engine --test cost_model_volume_share --test cost_model_fee_accuracy --test cost_model_slippage_sign --test eval_prompt_cache_and_rolling_window` — pass
- `cargo test -p xvision-engine --test api_eval --test api_eval_compare --test api_eval_min_notional --test api_eval_run --test eval_store` — pass
- `pnpm --dir frontend/web test -- ScenarioForm` — pass

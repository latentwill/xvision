# qa10-stop-eval-run-control

Status: completed locally on 2026-05-15.

Implemented:

- Upgraded the run-detail active-run affordance from a small `Cancel` link to
  an explicit `Stop eval` button.
- Made repeated cancel requests idempotent once a run is already cancelled.
- Added store-level transition guards so terminal eval runs cannot be revived
  by later `running`, `failed`, or `completed` updates.
- Added a queued-to-running transition helper so queued runs cancelled before
  the background task starts are not revived.
- Changed async eval failure persistence to use active-only failure updates.
- Added executor terminal-state checks before model output processing and
  before paper-order submission.
- Ensured paper executor emits and closes a cancelled terminal stream the same
  way backtest already did.

Verification:

- `cargo fmt --all`
- `cargo test -p xvision-engine --test eval_store -- --nocapture`
- `cargo test -p xvision-engine --test api_eval cancel_is_idempotent_after_run_is_cancelled -- --nocapture`
- `pnpm --dir frontend/web test -- eval-runs-detail`
- `git diff --check -- crates/xvision-engine/src/eval/store.rs crates/xvision-engine/src/api/eval.rs crates/xvision-engine/src/eval/executor/backtest.rs crates/xvision-engine/src/eval/executor/paper.rs frontend/web/src/routes/eval-runs-detail.tsx crates/xvision-engine/tests/eval_store.rs crates/xvision-engine/tests/api_eval.rs frontend/web/src/routes/eval-runs-detail.test.tsx`

Notes:

- `corepack` is not installed in this environment, so frontend verification used
  `pnpm` directly.
- Cargo reported pre-existing warnings for unused test helpers in
  `crates/xvision-engine/src/api/eval.rs`.

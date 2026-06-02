# Autooptimizer Run Completion — 100x task plan

> Date: 2026-06-01
> Status: implementation plan plus 2026-06-01 progress log for completing a runnable optimizer cycle from CLI and UI
> Context: created after a scoped `100x run` planning attempt stalled in analyze stage at `.100x/runs/20260601_180428/`.

## Source requirements

- User goal: complete the optimizer feature so a full optimizer run can be completed.
- User goal: the Optimizer UI must not be blank across items and must let the operator run cycles.
- User goal: both CLI and UI launch paths must work.
- Existing spine: `docs/superpowers/plans/2026-05-27-autooptimizer-master-implementation-spine.md`.
- Core spec: `docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md`.
- AR-2/AR-3 plans: `docs/superpowers/plans/2026-05-09-autooptimizer-2-cycle-judge-evals.md` and `docs/superpowers/plans/2026-05-09-autooptimizer-3-dashboard.md`.

## Current blockers from code inspection

1. Frontend launch still posts to the stale legacy path `/api/autoresearch/evening-cycle`.
2. No dashboard route exists at `POST /api/autooptimizer/evening-cycle`.
3. The existing dashboard-safe CLI job API can launch work, but the remote CLI allowlist does not allow `optimizer evening-cycle`.
4. The UI subscribes only to default SSE `message` events, while `/api/autooptimizer/events` emits named events.
5. The SSE payload is an envelope `{kind, display_label, data}` but the React code treats the top-level JSON as a `CycleProgressEvent`.
6. CLI `optimizer evening-cycle` currently needs `--mock` for smoke-safe execution; the UI needs a mock-safe launch path until the real backtest adapter is wired.

## Implementation tasks

1. `100x run "Allow dashboard-safe optimizer evening-cycle CLI jobs. Edit crates/xvision-dashboard/src/cli_jobs/allowlist.rs only. Add a strict template for optimizer evening-cycle with value flags --session-id --config --db --strategy --budget and switch flag --mock; reject other optimizer subcommands remotely; add tests for allowed mock launch and rejected optimizer run. Verify scripts/cargo test -p xvision-dashboard cli_jobs_allowlist."`

2. `100x run "Fix Optimizer UI launch. Edit frontend/web/src/features/autooptimizer/LiveCycleView.tsx and frontend/web/src/api/cli.ts only if needed. Replace the stale autoresearch POST with POST /api/cli/jobs launching argv [optimizer, evening-cycle, --session-id, <generated>, --mock] plus optional --strategy and --budget. Subscribe to the created job's SSE stream and render stdout JSON cycle events. Verify pnpm run typecheck."`

3. `100x run "Fix Optimizer SSE client parsing. Edit frontend/web/src/features/autooptimizer/LiveCycleView.tsx and api.ts. Listen for named autooptimizer SSE events, unwrap {kind, display_label, data}, normalize type/kind/event_type and missing timestamps, and preserve fallback labels. Verify pnpm run typecheck."`

4. `100x run "Verify optimizer launch end to end. Run scripts/cargo test -p xvision-cli autooptimizer_e2e, scripts/cargo test -p xvision-dashboard cli_jobs_allowlist, pnpm run typecheck in frontend/web, and a CLI smoke command for optimizer evening-cycle --mock with a generated session id. Record any remaining blocker with command output."`

## Acceptance evidence

- `xvn optimizer --help` lists `evening-cycle`.
- `xvn optimizer evening-cycle --session-id <id> --mock` exits successfully and prints cycle JSON plus `cycle_id=... merkle_root=...`.
- `POST /api/cli/jobs` accepts the exact optimizer evening-cycle argv used by the UI and rejects unrelated optimizer subcommands.
- `/autooptimizer` renders controls, empty states, and live/job event rows instead of a blank surface.
- `frontend/web` typecheck passes.
- Targeted Rust tests for CLI and dashboard pass through `scripts/cargo`.

## 2026-06-01 progress

- Completed the strict dashboard CLI allowlist path for `optimizer evening-cycle --mock`.
- Replaced the stale UI launch path with a CLI job launch and job-SSE follower.
- Fixed live event parsing for named optimizer SSE events and `{kind, display_label, data}` envelopes.
- Fixed fresh `XVN_HOME` CLI startup by creating the lineage DB parent directory.
- Fixed seeded mock runs by adding the missing optimizer schema bootstrap tables and indexes used by evening-cycle.
- Added a focused React regression test for live event rendering and UI-launched CLI job stdout parsing.
- Added an eval-cache-backed non-mock paper tester adapter for CLI evening cycles.
- Added a UI `Mock` toggle so operators can launch either smoke cycles or real non-mock cycles through the same CLI job path.
- Added runtime provider resolution for non-mock optimizer cycles, including `default_llm` fallback and keyless `local-candle` support.
- Added an autooptimizer-local dispatch that returns valid mutator, judge, and trader JSON contracts through one dispatch handle.
- Persisted generated optimizer scenarios and agent-run baselines before eval backtests so real paper-test runs satisfy scenario and supervisor-note FK constraints.
- Attempted a second focused `100x run` for non-mock paper testing; it stalled in analyze stage at `.100x/runs/20260601_183020/` and was stopped after no progress.

## Verified

- `pnpm run typecheck`
- `pnpm exec vitest run src/features/autooptimizer/LiveCycleView.test.tsx`
- `pnpm run build`
- `scripts/cargo test -p xvision-engine --test autooptimizer_eval_adapter -- --nocapture`
- `scripts/cargo test -p xvision-cli autooptimizer -- --nocapture`
- `scripts/cargo test -p xvision-dashboard optimizer_evening_cycle`
- `scripts/cargo test -p xvision-dashboard --lib optimizer_evening_cycle -- --nocapture`
- `scripts/cargo test -p xvision-dashboard optimizer_other_subcommands_are_rejected`
- `scripts/cargo test -p xvision-engine --lib autooptimizer::local_dispatch -- --nocapture`
- Fresh DB no-parent smoke: `XVN_HOME=<tmp> scripts/cargo run -p xvision-cli -- optimizer evening-cycle --session-id smoke-ui --mock`
- Seeded strategy smoke: `XVN_HOME=<tmp> scripts/cargo run -p xvision-cli -- example seed` followed by `XVN_HOME=<tmp> scripts/cargo run -p xvision-cli -- optimizer evening-cycle --session-id smoke-seeded --mock --strategy example-trend-follower`
- Non-mock auth-gate proof: with `ANTHROPIC_API_KEY` unset, `XVN_HOME=<tmp> scripts/cargo run -p xvision-cli -- optimizer evening-cycle --session-id nonmock-proof --strategy example-trend-follower` now exits at `ANTHROPIC_API_KEY not set`, proving the previous explicit `--mock is required` blocker is gone.
- Full keyless non-mock proof: with `$XVN_HOME/config/default.toml` set to `default_llm.provider = "local-candle"` and a short autooptimizer config, `XVN_HOME=<tmp> scripts/cargo run -p xvision-cli -- optimizer evening-cycle --session-id local-short-proof --config <tmp>/autooptimizer.toml --strategy example-trend-follower` exited 0 and emitted `cycle_started`, `parent_selected`, `mutation_proposed`, `mutation_gated`, `honesty_check_run`, `cycle_sealed`, and final `cycle_id=... merkle_root=...`.

## Residual external-provider note

- A full external Anthropic/OpenAI-compatible cycle still depends on operator credentials and provider availability. The completed `local-candle` proof exercises the non-mock optimizer path, real cached-bar backtests, lineage persistence, judge/honesty flow, and cycle sealing without using the stub paper tester.

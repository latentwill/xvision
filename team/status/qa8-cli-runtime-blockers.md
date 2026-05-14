# qa8-cli-runtime-blockers

Status: implemented-verified

Claimed: 2026-05-13T14:58:23Z

Worktree: `.worktrees/qa8-cli-runtime-blockers`

Implemented:

- Added a regression test for an existing DB whose eval tables already use
  `agent_id` and no longer have `strategy_bundle_hash`, covering the reported
  migration drift failure mode.
- Centralized CLI `XVN_HOME` resolution in `commands::home` and wired the
  runtime-facing CLI commands that previously duplicated home fallback logic.
- Hardened MCP `XVN_HOME` resolution so an empty environment value does not
  redirect the runtime to an empty-path home.
- Expanded `BarGranularity` from the previous small fixed set to
  Alpaca-supported parsed timeframes, including 1-59m, 1-23h, 1d, 1w, and
  1/2/3/4/6/12mo, while still accepting legacy names such as `Hour4`.
- Wired the broader granularity support through scenario API validation,
  scenario preview/chart bar counting, `xvn scenario`, `xvn bars`, and
  cache-backed `xvn ab-compare`.
- Changed the scenario UI granularity control to a datalist-backed freeform
  input with common options, so agents/operators can enter supported
  timeframes beyond a hardcoded radio set.
- Confirmed runtime source no longer queries `strategy_bundle_hash` outside
  migration compatibility tests and migration files.

Verification:

- `corepack pnpm --dir frontend/web test -- ScenarioForm scenarios-new WizardPreviewChart`
- `corepack pnpm --dir frontend/web typecheck`
- `corepack pnpm --dir frontend/web test`
- `git diff --check`
- `rg -n "strategy_bundle_hash" crates frontend --glob '!**/migrations/**'`

Blocked local verification:

- Local Cargo commands are forbidden on this deploy host by `CLAUDE.md`; Rust
  tests were not run locally. CI/non-deploy follow-up should run:
  - `cargo test -p xvision-engine api_context scenario_api`
  - `cargo test -p xvision-cli scenario bars`
  - `cargo test -p xvision-data alpaca_fetcher`
- `rustfmt` is not installed on this host, so Rust formatting was checked only
  by review plus `git diff --check`.

Last updated: 2026-05-13T15:13:18Z

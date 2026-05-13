# qa8-cli-json-contracts

Status: implemented-verified-static

Claimed: 2026-05-13T15:17:35Z

Worktree: `.worktrees/qa8-cli-json-contracts`

Branch: `qa8-cli-json-contracts`

Base: `qa8-cli-noninteractive-core-flows` commit `7248d0b`

Implemented:

- Added `--json` to `xvn strategy create` / `xvn strategy new`, returning a
  stable object with `id` and `strategy`.
- Added `--json` to `xvn strategy ls`, returning an array of strategy ids.
- Added `--json` to `xvn scenario create`, returning the created Scenario.
- Confirmed `xvn eval run`, `xvn eval list`, and `xvn eval get/show` already
  expose JSON output.
- Added CLI output-shape regressions for strategy create/list JSON and scenario
  create JSON.
- Updated MANUAL examples to show the JSON-capable commands.

Verification:

- `git diff --check`
- `rg -n -e "--json|serde_json::to_string_pretty|serde_json::json|visible_alias" crates/xvision-cli/src/commands/strategy.rs crates/xvision-cli/src/commands/scenario.rs crates/xvision-cli/src/commands/eval.rs crates/xvision-cli/tests/strategy_cli.rs crates/xvision-cli/tests/scenario_cli.rs crates/xvision-cli/tests/exit_codes_eval.rs MANUAL.md`

Blocked local verification:

- Cargo commands are forbidden on this deploy host by `CLAUDE.md`; Rust tests
  were not run locally. CI/non-deploy follow-up should run:
  - `cargo test -p xvision-cli strategy_cli scenario_cli exit_codes_eval`

Last updated: 2026-05-13T15:19:04Z

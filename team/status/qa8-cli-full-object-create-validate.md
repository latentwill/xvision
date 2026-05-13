# qa8-cli-full-object-create-validate

Status: implemented-verified-static

Claimed: 2026-05-13T15:19:48Z

Worktree: `.worktrees/qa8-cli-full-object-create-validate`

Branch: `qa8-cli-full-object-create-validate`

Base: `qa8-cli-json-contracts` commit `5e34178`

Implemented:

- Added `xvn strategy create --from-file <strategy.json|strategy.toml>` for
  full Strategy object creation, with optional `--json` output.
- Exposed `api::scenario::validate_request` and added
  `xvn scenario validate --from-file <request.toml>` as a dry-run validation
  path for full scenario create payloads.
- Added `xvn eval validate --strategy <id> --scenario <id> [--mode ...]` to
  check mode parsing plus strategy/scenario existence without launching a run.
- Updated MANUAL examples for the new file-create and dry-run eval validation
  surfaces.
- Added CLI regressions for strategy full-object create, scenario validate,
  and eval validate error behavior.

Verification:

- `git diff --check`
- `rg -n -e "from_file|Validate\\(|validate_request|eval validate|strategy create --from-file|scenario validate" crates/xvision-cli/src/commands crates/xvision-engine/src/api/scenario.rs crates/xvision-cli/tests MANUAL.md`

Blocked local verification:

- Cargo commands are forbidden on this deploy host by `CLAUDE.md`; Rust tests
  were not run locally. CI/non-deploy follow-up should run:
  - `cargo test -p xvision-cli strategy_cli scenario_cli exit_codes_eval`
  - `cargo test -p xvision-engine scenario_api`

Last updated: 2026-05-13T15:22:34Z

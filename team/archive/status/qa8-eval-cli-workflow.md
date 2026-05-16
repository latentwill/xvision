# qa8-eval-cli-workflow

Status: implemented-verified-static

Claimed: 2026-05-13T15:23:29Z

Worktree: `.worktrees/qa8-eval-cli-workflow`

Branch: `qa8-eval-cli-workflow`

Base: `qa8-cli-full-object-create-validate` commit `77bb715`

Implemented:

- Added `xvn eval results <run_id>` as a first-class results verb backed by
  the existing run detail renderer.
- Added `xvn eval watch <run_id>` with polling until terminal status, plus
  `--once` and `--json` modes for automation.
- The watch text output includes status, mode, scenario, final metrics when
  present, and failure reason when present.
- Added CLI regressions for unknown-run behavior on `eval results` and
  `eval watch`.
- Updated MANUAL eval workflow examples.

Verification:

- `git diff --check`
- `rg -n -e "Watch|Results|eval watch|eval results|is_terminal|print_run_status_line" crates/xvision-cli/src/commands/eval.rs crates/xvision-cli/tests/exit_codes_eval.rs MANUAL.md`

Blocked local verification:

- Cargo commands are forbidden on this deploy host by `CLAUDE.md`; Rust tests
  were not run locally. CI/non-deploy follow-up should run:
  - `cargo test -p xvision-cli exit_codes_eval`

Last updated: 2026-05-13T15:24:41Z

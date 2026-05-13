# qa8-cli-doctor-help-examples

Status: implemented-verified-static

Claimed: 2026-05-13T15:25:14Z

Worktree: `.worktrees/qa8-cli-doctor-help-examples`

Branch: `qa8-cli-doctor-help-examples`

Base: `qa8-eval-cli-workflow` commit `53b3aca`

Implemented:

- Added `xvn doctor [--json]` to report effective `XVN_HOME`, DB path,
  config path, provider/broker secret paths, strategies dir, template names,
  config/secrets existence, and remote target.
- Added a CLI regression for `xvn doctor --json`.
- Updated top-level help coverage to expect `doctor`.
- Added README/MANUAL examples for `xvn doctor --json`.

Verification:

- `git diff --check`
- `rg -n -e "Doctor|doctor|DoctorReport|list_template_names" crates/xvision-cli/src crates/xvision-cli/tests README.md MANUAL.md`

Blocked local verification:

- Cargo commands are forbidden on this deploy host by `CLAUDE.md`; Rust tests
  were not run locally. CI/non-deploy follow-up should run:
  - `cargo test -p xvision-cli doctor_cli help_cli`

Last updated: 2026-05-13T15:27:04Z

# qa8-cli-noninteractive-core-flows

Status: implemented-verified-static

Claimed: 2026-05-13T15:14:24Z

Worktree: `.worktrees/qa8-cli-noninteractive-core-flows`

Branch: `qa8-cli-noninteractive-core-flows`

Base: `qa8-cli-runtime-blockers` commit `faa7e97`

Implemented:

- Added `xvn strategy create` as a visible alias for the existing
  flag-driven `strategy new` implementation.
- Added `xvn eval get` as a visible alias for the existing `eval show`
  implementation.
- Added CLI integration regressions for `strategy create` and `eval get`.
- Updated README/MANUAL examples to prefer `strategy create` and `eval get`.
- Fixed inherited shared-home change in `strategy new/create` by using
  `std::env::var("XVN_CREATOR")` after the module `std::env` import was
  removed.
- Audited the scoped CLI sources for prompt libraries/stdin reads; no
  interactive prompt code was present in the covered core flows.

Verification:

- `git diff --check`
- `rg -n "xvn strategy new|strategy new|xvn eval show|eval show <run_id>|eval show|dialoguer|inquire|read_line|stdin" README.md MANUAL.md crates/xvision-cli/src crates/xvision-cli/tests || true`

Blocked local verification:

- Cargo commands are forbidden on this deploy host by `CLAUDE.md`; Rust tests
  were not run locally. CI/non-deploy follow-up should run:
  - `cargo test -p xvision-cli help_cli strategy_cli exit_codes_eval`

Last updated: 2026-05-13T15:17:06Z

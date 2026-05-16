# Claim: qa8-cli-json-contracts

Claimed: 2026-05-13T15:17:35Z

Worktree: `.worktrees/qa8-cli-json-contracts`

Branch: `qa8-cli-json-contracts`

Base: `qa8-cli-noninteractive-core-flows` commit `7248d0b`

Scope:

- Add or standardize stable machine-readable JSON output for list/get/create/run
  commands that agents need to chain.
- Keep changes narrowly on CLI output contracts and tests.

Verification target:

- Add focused CLI golden/shape regressions where possible.
- Do not run Cargo on this deploy host; record Rust test commands as
  CI/non-deploy follow-up.

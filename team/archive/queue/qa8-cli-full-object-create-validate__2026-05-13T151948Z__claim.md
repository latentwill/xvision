# Claim: qa8-cli-full-object-create-validate

Claimed: 2026-05-13T15:19:48Z

Worktree: `.worktrees/qa8-cli-full-object-create-validate`

Branch: `qa8-cli-full-object-create-validate`

Base: `qa8-cli-json-contracts` commit `5e34178`

Scope:

- Add full-object / file-driven creation and dry-run validation paths for
  strategy, scenario, and eval where missing.
- Keep the first pass narrow and compatible with existing API shapes.

Verification target:

- Add CLI regression tests where possible.
- Do not run Cargo on this deploy host; record Rust test commands as
  CI/non-deploy follow-up.

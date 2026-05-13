# Claim: qa8-cli-noninteractive-core-flows

Claimed: 2026-05-13T15:14:24Z

Worktree: `.worktrees/qa8-cli-noninteractive-core-flows`

Branch: `qa8-cli-noninteractive-core-flows`

Base: `qa8-cli-runtime-blockers` commit `faa7e97`

Scope:

- Make core CLI flows non-interactive for strategy create, scenario create,
  eval run, eval list, and eval get/show.
- Prefer explicit flags and file inputs over prompts so agents can complete
  workflows without the UI.

Verification target:

- Add focused CLI regression tests where possible.
- Run frontend checks only if frontend files change.
- Do not run Cargo on this deploy host; record Rust test commands as
  CI/non-deploy follow-up.

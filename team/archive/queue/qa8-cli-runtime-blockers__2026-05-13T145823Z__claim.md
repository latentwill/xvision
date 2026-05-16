# Claim: qa8-cli-runtime-blockers

Claimed: 2026-05-13T14:58:23Z

Worktree: `.worktrees/qa8-cli-runtime-blockers`

Branch: `qa8-cli-runtime-blockers`

Scope:

- Fix the remote CLI/runtime schema blocker reported as
  `no such column: strategy_bundle_hash`.
- Audit and repair `XVN_HOME` resolution so CLI subcommands consistently use
  the same home/config/DB target and do not silently fall back to a different
  baked-in store.
- Add scenario API/CLI support for missing granularities, including the
  reported 6h rejection, so all intended time frames are accepted.

Verification target:

- Add focused regression tests where possible.
- Run non-Cargo frontend/script/static checks locally.
- Do not run Cargo on this deploy host; leave Rust test commands as CI
  follow-up if the fix touches Rust.

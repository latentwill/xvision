# Claim: v2a-driver-tour-infrastructure

Worktree: `.worktrees/v2a-driver-tour-infrastructure`

Branch: `v2a-driver-tour-infrastructure`

Owner: codex

## Scope

Implement the first V2A intake item from
`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`: Driver.js guided
tour infrastructure with first-visit persistence and a restart-tour entry point.

Initial slice:

- Add a small frontend tour registry/runtime that can launch a route tour.
- Persist completed/dismissed route-tour state in browser storage for the
  first slice, matching F36's single-machine recommendation.
- Add route tour anchors and at least one concrete tour on a dense dashboard
  surface.
- Expose restart-tour from an existing always-available command/control surface
  without blocking keyboard workflows.

## Verification plan

- TDD: write focused frontend tests first for persistence/restart behavior.
- `corepack pnpm --dir frontend/web test -- <focused tour tests>`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`

Rust tests and Cargo commands are CI/non-deploy only on this host per
`CLAUDE.md`.

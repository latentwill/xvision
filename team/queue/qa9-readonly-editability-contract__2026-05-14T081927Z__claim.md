# qa9-readonly-editability-contract claim

Claimed: 2026-05-14T08:19:27Z
Worktree: `.worktrees/qa9-readonly-editability-contract`
Branch: `qa9-readonly-editability-contract`

Scope:

- Clarify that Inspector manifest and mechanical params are read-only surfaces.
- Make the setup page explicit that saved draft state changes only after a
  completed tool call.
- Add focused frontend coverage for the copy contract.

Verification plan:

- `corepack pnpm --dir frontend/web test -- authoring-risk setup`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`

# qa9-readonly-editability-contract

Status: implemented and frontend-verified

Claimed: 2026-05-14T08:19:27Z
Worktree: `.worktrees/qa9-readonly-editability-contract`
Branch: `qa9-readonly-editability-contract`

Implemented:

- Updated the Strategy Inspector manifest hint to state that direct edits are
  locked there, and that wizard changes only appear after a save tool succeeds.
- Updated the mechanical params hint to identify the panel as read-only saved
  JSON.
- Added setup guidance that completed tool calls are the boundary for saved
  draft changes and that the Inspector should be checked before eval.
- Added focused regression coverage in `authoring-risk` and `setup` tests.

Verification:

- `corepack pnpm --dir frontend/web test -- authoring-risk setup`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`

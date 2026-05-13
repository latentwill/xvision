---
track: qa8-strategy-table-density
worktree: /root/deploy/xvision/.worktrees/qa8-strategy-table-density
branch: qa8-strategy-table-density
phase: implemented-verified
last_updated: 2026-05-13T14:51:00Z
---

# What I'm Doing Right Now

Implemented the QA8 Strategies table density fix. The Strategies list no longer
shows backend IDs in desktop table columns, mobile cards, or list-level
Inspector link labels; IDs remain available on the Inspector route. Long tags
truncate inside bounded pills with the full tag retained as the tooltip.

# Blocked On

nothing

# Next Up

- [x] Create isolated worktree and branch.
- [x] Record claim/status.
- [x] Read existing Strategies route and tests.
- [x] Add failing regression test for hiding backend IDs from the list.
- [x] Implement the table/card density update.
- [x] Run focused frontend tests and typecheck.

# Verification

- Red: `corepack pnpm --dir frontend/web test -- strategies` failed on the
  new assertion that `Backend ID` must not render in the list.
- Green: `corepack pnpm --dir frontend/web test -- strategies` passed 2
  route test files / 3 tests.
- `corepack pnpm --dir frontend/web typecheck` passed.
- `corepack pnpm --dir frontend/web test` passed: 16 files, 39 tests.
- `git diff --check` passed.
- Rust/Cargo verification intentionally not run on this deploy host per
  `CLAUDE.md`.

---
track: qa8-shared-chat-rail-context
worktree: /root/deploy/xvision/.worktrees/qa8-shared-chat-rail-context
branch: qa8-shared-chat-rail-context
phase: implemented-verified
last_updated: 2026-05-13T15:00:21Z
---

# What I'm Doing Right Now

Implemented and verified shared workspace chat rail context across route
changes/detail pages.

# Blocked On

nothing

# Next Up

- [x] Create isolated worktree and branch.
- [x] Record claim/status.
- [x] Inspect ChatRail mount and scope derivation.
- [x] Add failing regression for shared workspace chat scope.
- [x] Collapse route-specific rail scoping.
- [x] Run focused frontend tests and typecheck.

# Verification

- Red: `corepack pnpm --dir frontend/web test -- ChatRail` failed because `/authoring/01TEST` still resolved a route-specific strategy scope.
- Green: `corepack pnpm --dir frontend/web test -- ChatRail` passed 3 tests.
- `corepack pnpm --dir frontend/web typecheck` passed.
- `corepack pnpm --dir frontend/web test` passed 16 files / 40 tests.
- `git diff --check` passed.
- Cargo was not run on this deploy host per `CLAUDE.md`.

---
track: app-health-sweep-2026-05-14
worktree: /root/deploy/xvision/.worktrees/app-health-sweep-2026-05-14
branch: app-health-sweep-2026-05-14
phase: complete
last_updated: 2026-05-14T12:31:15Z
owner: codex
---

# Status

Created the health-sweep goal from current `origin/main`.

## Constraints

- Do not run Cargo, Docker builds, or deploy commands on this deploy host.
- Keep fixes narrow and independently reviewable.
- Avoid unmerged board-track overlap unless the touched code is already on
  `main` and the fix is local.

## Loop Log

- Created the health-sweep goal from current `origin/main`.
- Installed `frontend/web` dependencies with the frozen lockfile.
- Ran frontend typecheck: clean.
- Ran frontend tests: found `ChatThread` calling `scrollTo` in environments
  that do not implement it, breaking `ChatRail` tests.
- Fixed `ChatThread` auto-scroll to fall back to `scrollTop` when `scrollTo`
  is unavailable.
- Ran focused `ChatRail` tests: clean.
- Rebasing onto latest `origin/main` showed the `ChatThread` fallback had
  already landed there via PR #148, so this branch retained the upstream
  implementation and kept the remaining performance fix.
- Ran frontend tests again: found the route code-splitting regression guard
  failing because shell variants imported `ChatRail` directly.
- Moved `ChatRail` behind the `Layout` lazy boundary and passed the suspended
  component into mobile/tablet/desktop shells.
- Ran focused route code-splitting test: clean.
- Ran full frontend tests and production build: clean.
- Restored the tracked dashboard static `.gitkeep` removed by Vite build
  output cleanup.

## Verification

- `corepack pnpm --dir frontend/web typecheck` - passed.
- `corepack pnpm --dir frontend/web test -- ChatRail` - passed.
- `corepack pnpm --dir frontend/web test -- routes-code-splitting` - passed.
- `corepack pnpm --dir frontend/web test` - passed, 22 files / 79 tests.
- `corepack pnpm --dir frontend/web build` - passed.

## Result

- Bug fix verified: chat thread auto-scroll now works in DOMs without
  `scrollTo`; upstream PR #148 contains that code after rebase.
- Performance fix: `ChatRail` now builds as its own async chunk instead of
  being pulled through the responsive shell imports.
- No additional safe fixes are visible from the allowed frontend checks in this
  slice without expanding into active board tracks.

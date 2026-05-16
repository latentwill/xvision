# color-themes-light-dark claim

Claimed: 2026-05-14T08:32:35Z
Worktree: `.worktrees/color-themes-light-dark`
Branch: `color-themes-light-dark`

Scope:

- Verify and publish the implemented color-only dashboard theme track.
- Cover theme model/provider behavior, settings General tab, sidebar toggle,
  and chart palette integration.

Verification plan:

- `corepack pnpm --dir frontend/web install --frozen-lockfile`
- `corepack pnpm --dir frontend/web test -- theme settings-layout Sidebar RunChart ScenarioChart`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check codex/q8-board-docs...HEAD`

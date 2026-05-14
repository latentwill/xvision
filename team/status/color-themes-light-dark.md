# color-themes-light-dark

Status: implemented and frontend-verified

Claimed: 2026-05-14T08:32:35Z
Worktree: `.worktrees/color-themes-light-dark`
Branch: `color-themes-light-dark`

Implemented:

- Added light, folio dark, black, and auto theme model/persistence.
- Added `ThemeProvider`, General settings appearance controls, and sidebar
  sun/moon quick toggles.
- Connected chart palettes to resolved app themes.
- Audited theme-sensitive hard-coded color classes touched by this track.

Verification:

- `corepack pnpm --dir frontend/web install --frozen-lockfile`
- `corepack pnpm --dir frontend/web test -- theme settings-layout Sidebar RunChart ScenarioChart`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check codex/q8-board-docs...HEAD`

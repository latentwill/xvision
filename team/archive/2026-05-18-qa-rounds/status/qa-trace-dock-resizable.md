---
track: qa-trace-dock-resizable
worktree: .worktrees/qa-trace-dock-resizable
branch: task/qa-trace-dock-resizable
base: origin/main
phase: pr-open
last_updated: 2026-05-18T03:40:00Z
owner: claude
---

# What changed

- Added a `heightPx: number` slice to `useTraceDock` plus a `setHeightPx`
  action that clamps to `[96, 0.9*vh]`, persists to localStorage under
  `xvision.trace-dock.height`, and hydrates from localStorage on store
  module load. The named `DockHeight` enum (`collapsed | peek | working |
  full`) is retained for back-compat with `StripDockSlot.setHeight("working")`,
  but the rendered dock height is driven by `heightPx`.
- New `DockResizeHandle` component: a top-edge separator that
  - drags vertically via pointer events (dock grows as the pointer moves up),
  - is focusable (`tabIndex={0}`, `role="separator"`, `aria-valuenow`),
  - accepts `ArrowUp` / `ArrowDown` to nudge ±24px,
  - accepts `Home` / `End` to jump to min / max.
- `TraceDock` no longer renders the `peek` / `working` / `full` preset
  buttons. The lone fullscreen affordance is the pop-out arrows; the
  minimize and download controls are unchanged. The `peek`-conditional
  body layout branches are removed (the resize handle makes preset
  heights vestigial).
- Tests:
  - `trace-dock.test.ts` adds a `heightPx slice` suite covering
    clamp bounds, localStorage write-through.
  - `TraceDock.test.tsx` adds: no `Full` / `peek` / `working` preset
    buttons render; pop-out arrows + resize handle remain; dock style
    binds to `heightPx`.
  - `DockResizeHandle.test.tsx` covers: aria attributes; pointer drag
    updates store; drag clamps to min; arrow nudge; Home/End jump;
    localStorage write; persistence across module re-evaluation.

# Verification

- Passed: `corepack pnpm --dir frontend/web test -- TraceDock` (7 tests)
- Passed: `corepack pnpm --dir frontend/web test -- --run trace-dock DockResizeHandle` (23 tests)
- Passed: `corepack pnpm --dir frontend/web typecheck`
- Passed: `corepack pnpm --dir frontend/web build`

# Notes

- No `border-white` / `border-gray-*` / `#fff` introduced (CLAUDE.md rule).
- `prefers-reduced-motion`: the handle does not animate the dock; the
  dock's inline `height` style snaps to the new pixel value on each
  store write. No transition is applied to the dock or to the handle.
- Back-compat: `StripDockSlot.tsx` continues to call
  `useTraceDock.getState().setHeight("working")` to open the dock; the
  rendered height comes from `heightPx`, which is now the persisted
  user choice (defaults to `480px`).

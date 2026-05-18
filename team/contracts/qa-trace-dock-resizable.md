---
track: qa-trace-dock-resizable
lane: leaf
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-trace-dock-resizable
branch: task/qa-trace-dock-resizable
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none   # trace-dock-ux-polish shipped via #251
allowed_paths:
  - frontend/web/src/features/agent-runs/TraceDock.tsx
  - frontend/web/src/features/agent-runs/TraceDock.test.tsx
  - frontend/web/src/features/agent-runs/DockResizeHandle.tsx
  - frontend/web/src/features/agent-runs/DockResizeHandle.test.tsx
  - frontend/web/src/stores/trace-dock.ts
  - frontend/web/src/stores/trace-dock.test.ts
forbidden_paths:
  - crates/**
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/FlameGraph.tsx
  - frontend/web/src/features/agent-runs/RunStatusStrip.tsx
  - frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx
interfaces_used:
  - useTraceDock store (extends height slice)
  - TraceDock header (replaces fullscreen-arrows / Full button)
parallel_safe: true
parallel_conflicts:
  - "trace-fullscreen-redesign (claimed): edits the pop-out route surface. This contract removes the in-dock 'Full' button; coordinate so exactly one fullscreen affordance remains (the pop-out arrows)."
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run trace-dock DockResizeHandle
  - pnpm --dir frontend/web build
acceptance:
  - The trace dock no longer renders both a "Full" button and a
    fullscreen-arrows button. Exactly one fullscreen affordance
    remains: the pop-out to the `/agent-runs/:runId` route (the
    arrows). The "Full" duplicate is removed.
  - The dock gains a drag handle on its top edge. The operator can
    drag it up / down to set the dock's height. The minimum height
    keeps the run-status strip + a single trace row visible (~ 96px).
    The maximum height is `90vh`.
  - The chosen height persists across page reloads via `localStorage`
    (key namespaced under `xvision.trace-dock.height`). The
    `useTraceDock` store exposes the height as a slice.
  - Keyboard accessibility: the drag handle is focusable, accepts
    `ArrowUp` / `ArrowDown` to nudge the height by 24px and
    `Home` / `End` to jump to min / max. Respects
    `prefers-reduced-motion`.
  - The minimize affordance (existing collapse-to-strip) is
    preserved alongside the new resize handle — they are distinct
    interactions.
  - Tests cover: (1) the dock renders no "Full" button; (2) drag
    interaction on the handle updates the store height; (3) the
    height persists across mount/unmount via localStorage; (4)
    keyboard nudge works.
  - No `border-white` / `border-gray-100` / `border-gray-200` / `#fff`
    on dark mode (CLAUDE.md rule).
---

# Scope

Operator reported (2026-05-18): the trace dock has a "Full" button
and a fullscreen-arrows button that do roughly the same thing, and
neither addresses what the operator actually wants — to make the dock
taller for richer browsing without committing to a full pop-out
route. Replace with a resizable dock and consolidate the fullscreen
affordance.

# Out of scope

- The pop-out `/agent-runs/:runId` route surface itself
  (`trace-fullscreen-redesign`).
- FlameGraph / SpanInspector / RunStatusStrip layout — out of scope.
  This contract only edits the dock chrome, the resize handle, and
  the store slice for height. (`trace-dock-ux-polish` shipped via #251
  and is no longer an active contract; the dock layout it left is the
  baseline this contract builds on.)
- Horizontal resize. Operator asked for vertical only.
- Backend / API changes.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-trace-dock-resizable status
git -C .worktrees/qa-trace-dock-resizable log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-trace-dock-resizable \
  -b task/qa-trace-dock-resizable origin/main
```

No stacking — base on `origin/main` (`trace-dock-ux-polish` shipped
via #251 before this contract opened).

# Notes

Append checkpoints / PR links below. Worker must confirm the
fullscreen-arrows affordance is preserved (the operator wants it AS
the pop-out trigger) before removing the "Full" button.

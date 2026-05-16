---
track: q15-scenario-granularity-dropdown
lane: leaf
wave: q15
worktree: .worktrees/q15-scenario-granularity-dropdown
branch: task/q15-scenario-granularity-dropdown
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/features/scenarios/authoring/**
  - frontend/web/src/features/scenarios/granularity-select.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/features/eval-runs/**
  - frontend/web/src/features/chat-rail/**
  - frontend/web/src/themes/**
interfaces_used:
  - Radix Select / shadcn SelectPrimitive
parallel_safe: true
parallel_conflicts: []
verification:
  - corepack pnpm --dir frontend/web test -- scenarios-create granularity-select
  - corepack pnpm --dir frontend/web typecheck
acceptance:
  - Granularity dropdown in scenario create/edit form opens on click and on keyboard activation.
  - Selected granularity value persists into form state and submits to the API.
  - Regression test covers open / keyboard / select / submit flow.
---

# Scope

Fix QA15 item 1: the granularity (timeframe) dropdown does not pop down in
the scenario create/edit form. Likely a Radix Select portal/z-index or
controlled-state regression after recent scenario authoring work.

# Out of scope

- Reshaping the scenario form layout.
- Adding new granularities (`qa4-scenarios-4h-bars-ui` already added 4H).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/q15-scenario-granularity-dropdown -b task/q15-scenario-granularity-dropdown origin/main
```

# Notes

- Diagnose first; if the dropdown is rendering off-screen or behind another
  layer, fix the portal/z-index rather than rewriting the component.

---
track: strategies-folder-into-view-toggle
lane: leaf
wave: qa-chat-rail-2026-05-21
worktree: .worktrees/strategies-folder-into-view-toggle
branch: task/strategies-folder-into-view-toggle
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes.tsx
  - frontend/web/src/routes/strategies.tsx
  - frontend/web/src/routes/strategies.test.tsx
  - frontend/web/src/routes/strategies-folder.tsx
  - frontend/web/src/routes/strategies-folder.test.tsx
  - frontend/web/src/components/strategies/**
forbidden_paths:
  - crates/**
  - frontend/web/src/routes/memory*
  - frontend/web/src/features/memory/**
  - frontend/web/src/features/agents/**
  - frontend/web/src/routes/agents*
  - frontend/web/src/api/types.gen/**
interfaces_used:
  - react-router (route registration)
  - existing StrategiesFolderRoute component export
parallel_safe: false
parallel_conflicts:
  - "memory-into-agents-section: also edits frontend/web/src/routes.tsx. Disjoint blocks of the route table — coordinate via queue note. Later claimant rebases."
verification:
  - cd frontend/web && npm run typecheck
  - cd frontend/web && npm run test -- strategies
  - cd frontend/web && npm run lint
acceptance:
  - **Toggle in the `/strategies` header.** The `/strategies` route renders a `List | Folder` segmented control in the page header. Default view is `list`. URL state: `/strategies?view=list` and `/strategies?view=folder`. Toggling the control updates the URL via `useSearchParams` (or react-router equivalent); back/forward navigates between views without remounting the page shell.
  - **Folder view re-uses the existing component.** The folder view in the `view=folder` branch mounts the existing `StrategiesFolderRoute` content (or its underlying view component if `StrategiesFolderRoute` is a route-level wrapper). No duplicate implementation. Existing folder behavior — listing, navigation, search/filter — works identically inside the toggled view.
  - **Backward-compat alias for `/strategies-folder`.** The standalone `/strategies-folder` route either:
    - (a) stays registered and resolves to a small redirect component that navigates to `/strategies?view=folder` on mount; OR
    - (b) is replaced with a react-router `<Navigate to="/strategies?view=folder" replace />` config in `routes.tsx`.
    Whichever the worker picks, deep links to `/strategies-folder` continue to land on the folder view.
  - **Sidebar nav.** If the sidebar nav has a separate "Strategies Folder" entry, it is removed (the toggle subsumes it). The sidebar's "Strategies" entry continues to point at `/strategies`.
  - **Test coverage.** Update or add tests:
    - `strategies.test.tsx`: toggle from List → Folder updates `?view=` and renders folder content; toggle back renders list content.
    - `strategies-folder.test.tsx`: existing tests still pass when the component is mounted from inside `/strategies?view=folder`. If the file is removed (alias variant b), its assertions either migrate into `strategies.test.tsx` or are dropped because the test was specific to the standalone route shell.
  - **No new top-level routes.**
  - **No changes outside listed allowed paths.**
---

# Scope

Operator IA change, 2026-05-21: "Folder isn't a separate destination,
it's just how you're looking at strategies." Collapse the top-level
`/strategies-folder` route into a `List | Folder` view toggle on
`/strategies`. The URL convention is `/strategies?view=folder`.
Existing folder behavior is preserved by re-mounting the existing
`StrategiesFolderRoute` component (or its view body) inside the
toggled branch.

Touch points are deliberately small: the route table, the
`/strategies` page header, and either a tiny redirect at
`/strategies-folder` or a route-config `<Navigate>`. No engine
changes.

# Out of scope

- Memory route IA. Separate track (`memory-into-agents-section`).
- Engine, wizard, or chat-rail changes.
- Refactoring the folder view component beyond what is needed to
  mount it inside the toggled view.
- New search/filter/list behavior. The merged
  `list-search-filter-completion-audit` work governs that surface.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/strategies-folder-into-view-toggle status
git -C .worktrees/strategies-folder-into-view-toggle log --oneline -3 origin/main..HEAD
# Check whether memory-into-agents-section is also touching routes.tsx
gh pr list --state open --search "memory-into-agents-section in:title"
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/strategies-folder-into-view-toggle \
  -b task/strategies-folder-into-view-toggle origin/main
```

# Notes

Coordinate with `memory-into-agents-section` via `team/queue/` if
both tracks are claimed simultaneously. Both edit
`frontend/web/src/routes.tsx` in disjoint blocks (the `strategies` /
`strategies-folder` route entries vs. the `memory` route entry);
later claimant rebases.

Append checkpoints / PR links below.

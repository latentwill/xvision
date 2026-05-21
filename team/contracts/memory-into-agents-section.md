---
track: memory-into-agents-section
lane: leaf
wave: qa-chat-rail-2026-05-21
worktree: .worktrees/memory-into-agents-section
branch: task/memory-into-agents-section
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes.tsx
  - frontend/web/src/routes/agents.tsx
  - frontend/web/src/routes/agents.test.tsx
  - frontend/web/src/features/memory/MemoryPage.tsx
  - frontend/web/src/features/memory/MemoryPage.test.tsx
  - frontend/web/src/features/memory/MemorySurface.tsx
  - frontend/web/src/components/layout/**
forbidden_paths:
  - crates/**
  - frontend/web/src/components/agent/MemoryTab.tsx
  - frontend/web/src/components/agent/MemoryTab.test.tsx
  - frontend/web/src/features/eval-runs/review/MemoryPanel.tsx
  - frontend/web/src/features/eval-runs/review/MemoryPanel.test.tsx
  - frontend/web/src/routes/strategies*
  - frontend/web/src/api/types.gen/**
interfaces_used:
  - react-router (route registration)
  - MemoryPage / MemorySurface component exports (existing)
parallel_safe: false
parallel_conflicts:
  - "strategies-folder-into-view-toggle: also edits frontend/web/src/routes.tsx. Disjoint blocks of the route table — coordinate via queue note. Later claimant rebases."
verification:
  - cd frontend/web && npm run typecheck
  - cd frontend/web && npm run test -- memory agents
  - cd frontend/web && npm run lint
acceptance:
  - **`/memory` is no longer a top-level route.** The entry at `frontend/web/src/routes.tsx:95` (`{ path: "memory", element: page(<MemoryPage />) }`) is replaced with a `/agents/memory` registration (or `/agents` with a Memory sub-route, worker picks the shape that fits react-router's existing structure in this app).
  - **`MemoryPage` mounts under the Agents section.** The page is reachable at `/agents/memory` and shows the same "memory across all agents" view that lived at `/memory`. No behavior change in the page itself beyond its mount point — `MemorySurface.tsx` is repointed, not refactored.
  - **Per-agent memory is untouched.** `frontend/web/src/components/agent/MemoryTab.tsx` and its test remain as-is. The Memory tab inside an individual agent's edit page continues to work.
  - **Eval-run memory panel is untouched.** `frontend/web/src/features/eval-runs/review/MemoryPanel.tsx` is forbidden territory for this track.
  - **Sidebar nav.** The top-level "Memory" entry in the sidebar is removed. The Agents section gains a Memory entry/sublink pointing at `/agents/memory`. The exact UX (tab inside Agents page vs. left-rail item inside the Agents section) is the worker's call; document the chosen shape in the PR description.
  - **Backward-compat alias for `/memory`.** The standalone `/memory` URL continues to resolve, either via a `<Navigate to="/agents/memory" replace />` route or a small redirect component. Deep links survive.
  - **Test coverage.** `MemoryPage.test.tsx` continues to pass when the page is mounted from the new path. New test asserts `/memory` redirects to `/agents/memory`. New test asserts the Agents page surfaces the Memory entry.
  - **No new top-level routes.**
  - **No changes outside listed allowed paths.**
---

# Scope

Operator IA change, 2026-05-21: "Memory is already an agent concept
in your code — `components/agent/MemoryTab.tsx` exists. Demote
`/memory` to `/agents/memory` and surface it from the Agents page.
The global view becomes 'memory across all agents' rather than its
own top-level concept."

Three small moves:

1. Route table: drop `/memory` as a top-level route; register
   `/agents/memory` (or a Memory sub-route under `/agents`,
   whichever fits react-router's existing nested-route pattern).
2. Sidebar nav: drop the top-level Memory entry; add the Memory
   entry inside the Agents section.
3. Backward-compat redirect from `/memory` so deep links survive.

`MemoryPage.tsx` and `MemorySurface.tsx` are repointed at the new
mount point; their internals are not refactored.

# Out of scope

- Per-agent `MemoryTab.tsx` and the eval-run `MemoryPanel.tsx` —
  forbidden paths.
- Strategies folder IA — separate track
  (`strategies-folder-into-view-toggle`).
- Engine changes. No backend changes; this is route + nav only.
- Refactoring the MemoryPage internals.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/memory-into-agents-section status
git -C .worktrees/memory-into-agents-section log --oneline -3 origin/main..HEAD
# Check whether strategies-folder-into-view-toggle is also touching routes.tsx
gh pr list --state open --search "strategies-folder-into-view-toggle in:title"
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/memory-into-agents-section \
  -b task/memory-into-agents-section origin/main
```

# Notes

Coordinate with `strategies-folder-into-view-toggle` via `team/queue/`
if both tracks are claimed simultaneously. Both edit
`frontend/web/src/routes.tsx` in disjoint blocks; later claimant
rebases.

Append checkpoints / PR links below.

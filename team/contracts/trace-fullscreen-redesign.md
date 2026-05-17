---
track: trace-fullscreen-redesign
lane: leaf
wave: agent-run-observability-followups
worktree: .worktrees/trace-fullscreen-redesign
branch: task/trace-fullscreen-redesign
base: origin/main
status: claimed
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/agent-runs-detail.tsx
  - frontend/web/src/routes/agent-runs-detail.test.tsx
  - frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx
  - frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.test.tsx
  - frontend/web/src/features/agent-runs/AgentRunRailTree.tsx
  - frontend/web/src/features/agent-runs/AgentRunRailTree.test.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/TraceDock.tsx
  - frontend/web/src/features/agent-runs/FlameGraph.tsx
  - frontend/web/src/api/**
  - frontend/web/src/stores/**
interfaces_used:
  - RunSpan / AgentRunDetail types
  - useSpanFilter
  - deriveDecisions
  - SpanInspector (read-only)
  - FilterBar (read-only)
parallel_safe: true
parallel_conflicts: []
verification:
  - (cd frontend/web && pnpm typecheck)
  - (cd frontend/web && pnpm test --run)
  - (cd frontend/web && pnpm build)
acceptance:
  - The pop-out `/agent-runs/:runId` route stops rendering two redundant
    hierarchy columns (rail tree + indented timeline). The rail tree is
    removed; a single Logfire-style waterfall timeline replaces both.
  - Each row in the new timeline shows: indent for tree depth, color dot
    by kind category, short kind label (e.g. `MODEL`), span name (e.g.
    model id), and an inline waterfall bar positioned on a per-run
    timeline that visualizes the span's start offset and duration
    relative to the run window. Duration text is right-aligned next to
    the bar. Status indicators (error, in_progress) remain.
  - The page no longer renders the `MODEL` etc. label twice per row.
    The kind chip and the model/tool name read as two distinct pieces
    of information, not the same label restated.
  - Layout uses available width: at `xl` breakpoint the surface splits
    into a main timeline column (flex) + SpanInspector (400px). Below
    `xl` the timeline takes the full width and the inspector stacks
    underneath.
  - Existing detail-route tests still pass after dropping the rail-tree
    assertion. Add a unit test asserting each timeline row exposes a
    waterfall bar with a `data-testid="span-waterfall-bar-<id>"` and
    `style.left` / `style.width` percentages computed against the run's
    start..end window (`s1` at 0%, `s4` strictly to the right of `s2`).
  - The deleted `AgentRunRailTree` and its tests are removed from the
    bundle (no dead imports, no `pnpm typecheck` warnings).
---

# Scope

User-reported polish on the pop-out full-screen agent-run view
(2026-05-18): the three-column layout repeats the same hierarchical
information twice (rail tree on the left + indented timeline in the
middle), the timeline only shows `kind`/`name`/`duration` text rows
with no temporal layout, and the visual hierarchy is weaker than the
docked `TraceDock` — the opposite of what the pop-out should be.

This track replaces the rail+timeline pair with one Logfire-style
waterfall timeline that puts a per-row time bar inline next to the
hierarchical tree, deletes the rail tree, and keeps the existing
SpanInspector untouched on the right.

# Out of scope

- The dock surface itself (`TraceDock.tsx`, `FlameGraph.tsx`) — owned by
  the merged `trace-dock-ux-polish` track.
- `SpanInspector.tsx` — claimed by
  `agent-run-observability-blob-fetch-route`.
- Any backend / observability / API changes.
- FilterBar adjustments.
- Mobile-specific route (`eval-runs-detail-mobile.tsx`).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/trace-fullscreen-redesign status
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/trace-fullscreen-redesign \
  -b task/trace-fullscreen-redesign origin/main
```

# Notes

Claimed 2026-05-18. User feedback referenced the dock as the "nicer"
surface; goal is to bring the pop-out to at least parity and use the
full width for a true waterfall (vs. the dock's compact flame graph).

---
track: strategy-agent-card-collapse
lane: leaf
wave: ux-polish
worktree: .worktrees/strategy-agent-card-collapse
branch: task/strategy-agent-card-collapse
base: origin/main
status: merged
pr: https://github.com/latentwill/xvision/pull/194
merged_at: 2026-05-16T15:32:47Z
merge_commit: 6ac2373acdb35aa6509f765572d05ab479220f1c
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/authoring.tsx
  - frontend/web/src/routes/authoring.test.tsx
  - frontend/web/src/routes/authoring-risk.test.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/api/**
  - frontend/web/src/components/agent/**
interfaces_used:
  - Strategy / AgentRef / Agent types from `@/api/types.gen`
parallel_safe: true
parallel_conflicts: []
verification:
  - corepack pnpm --dir frontend/web typecheck
  - corepack pnpm --dir frontend/web test -- authoring
acceptance:
  - Each attached-agent row on the strategy authoring page (`AgentsCard` in `authoring.tsx`, around line 314) has a collapse/expand toggle.
  - Collapsed state is a single bar showing role, agent name, and `provider / model`.
  - Expanded state shows the existing detail (role, agent_id, name, provider/model, actions).
  - A "Open in window" affordance pops the agent detail into a dedicated window (modal/dialog popout — implementation detail, must be dismissible).
  - Collapse state per row persists across page reloads under a namespaced localStorage key (use the existing safe-storage helper).
  - No regressions in existing authoring tests; new tests cover collapsed-bar model rendering and toggle behavior.
---

# Scope

Polish the attached-agent rows in the strategy authoring page so each row can
be collapsed to a compact bar that still shows the agent's `provider / model`,
expanded back to the current detail layout, or popped out into a window for a
focused view. Current rendering is in `frontend/web/src/routes/authoring.tsx`
inside `AgentsCard` (the `attached.map((a) => …)` block around line 314).

# Out of scope

- Changing strategy/agent data model or API.
- Editing the standalone agent editor under `frontend/web/src/components/agent/**`.
- Adding new fields to `AgentRef` or `Strategy` types.
- Any backend or crate changes.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/strategy-agent-card-collapse -b task/strategy-agent-card-collapse origin/main
```

# Notes

- Use existing primitives (`Card`, theme tokens, dark-mode-safe borders — never bare `border-white` per workspace CLAUDE.md).
- Persist collapse state under a namespaced localStorage key (e.g. `xvn:authoring:agent-collapse:<strategy_id>:<role>`); use the existing safe-storage helper that mobile-safari work introduced rather than touching `localStorage` directly.
- "Open in window" can be implemented as a modal/dialog popout reusing existing dialog primitives — no need to spawn a separate browser window.

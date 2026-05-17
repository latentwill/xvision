---
track: strategy-agent-card-collapse-resync
lane: leaf
wave: ux-polish
worktree: .worktrees/strategy-agent-card-collapse-resync
branch: task/strategy-agent-card-collapse-resync
base: origin/main
status: merged
pr: https://github.com/latentwill/xvision/pull/196
merged_at: 2026-05-16T15:45:47Z
merge_commit: c6dab008545c2857f93da1c22d244cb6267febdd
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/authoring.tsx
  - frontend/web/src/routes/authoring.test.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/api/**
  - frontend/web/src/components/agent/**
interfaces_used:
  - Strategy / Agent types from `@/api`
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- authoring
acceptance:
  - `AttachedAgentRow` re-syncs `collapsed` from `safeStorageGet(storageKey)` whenever the storage key changes (i.e. `strategyId` or `role` changes).
  - Navigating between two strategies that share `(agent_id, role)` reflects each strategy's own persisted collapse state rather than the first-mounted strategy's state.
  - New unit test asserts the resync via `rerender` with a different `strategyId`.
  - Existing `authoring.test.tsx` + `authoring-risk.test.tsx` continue to pass.
---

# Scope

Follow-up to #194. The `AttachedAgentRow` component only reads
`safeStorageGet(storageKey)` once via the `useState` lazy initializer. When
React Router navigates between two strategies that share `(agent_id, role)`,
the row's React key (`${agent_id}:${role}`) is stable, so the same component
instance is reused and the new strategy-scoped storage key is never consulted.
Result: stale collapse state across cross-strategy navigation.

Fix: add a `useEffect` that resyncs `collapsed` (and clears `popoutOpen`) when
`storageKey` changes. Export `AttachedAgentRow` for unit testing.

# Out of scope

- Any behaviour change beyond the resync semantic.
- Refactoring storage keys or the safe-storage helper.
- Backend or crate changes.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/strategy-agent-card-collapse-resync \
  -b task/strategy-agent-card-collapse-resync origin/main
```

# Notes

Reviewer comment that prompted this fix: cross-strategy navigation can show
the wrong collapsed/expanded state because the React key
`${agent_id}:${role}` collides across strategies that share that pair, and
the component never resyncs on `strategyId` change.

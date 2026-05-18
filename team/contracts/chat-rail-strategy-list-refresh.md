---
track: chat-rail-strategy-list-refresh
lane: leaf
wave: qa-operator-2026-05-18-r3
worktree: .worktrees/chat-rail-strategy-list-refresh
branch: task/chat-rail-strategy-list-refresh
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/components/chat/**
  - frontend/web/src/components/chat/cards/**
  - frontend/web/src/components/shell/ChatRail.tsx
  - frontend/web/src/components/shell/ChatRail.test.tsx
  - frontend/web/src/api/chat_rail.ts
  - frontend/web/src/api/strategies.ts
  - frontend/web/src/api/strategies.test.ts
  - frontend/web/src/api/scenarios.ts
  - frontend/web/src/api/scenarios.test.ts
  - frontend/web/src/api/agents.ts
  - frontend/web/src/api/agents.test.ts
  - frontend/web/src/api/eval.ts
  - frontend/web/src/api/eval.test.ts
  - frontend/web/src/routes/strategies.tsx
  - frontend/web/src/routes/scenarios.tsx
  - frontend/web/src/routes/agents.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/features/agent-runs/**
  - frontend/web/src/features/eval-runs/**
interfaces_used:
  - TanStack Query queryClient.invalidateQueries
  - chat rail tool-result handlers (existing)
  - strategies / scenarios / agents / eval-runs query keys
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run chat strategies scenarios agents
  - pnpm --dir frontend/web build
acceptance:
  - Creating a strategy via the chat rail on `/strategies` causes
    the strategies list to update **without** a manual refresh.
    The new row appears in <1s after the chat rail finishes the
    `create_strategy_draft` tool result.
  - Same for `/scenarios` (create_scenario), `/agents` (create_agent
    or whichever the chat rail uses), and `/eval-runs` (eval-run
    creation from chat). The contract audits each surface and
    confirms the invalidation fires.
  - Update / delete from the chat rail invalidates the same query
    keys (a follow-up `set_*` tool that mutates an existing record
    triggers a list refetch, not just the detail refetch).
  - Test coverage: for each surface, an integration test that
    simulates a chat-rail tool result and asserts
    `queryClient.invalidateQueries` was called with the matching
    key (mock or spy on the query client).
  - The audit is documented in
    `team/status/chat-rail-strategy-list-refresh.md` as a table:
    `surface | tool name | query key invalidated | test name`.
---

# Scope

Operator (2026-05-18): "Creating a strategy in chat rail on
strategies screen and the listing does not refresh with the
strategy. Only shows up on manual refresh. Need updates to push
live on all lists."

The chat rail receives wizard tool results via SSE. Successful
tool calls that mutate records should trigger TanStack Query cache
invalidations so the relevant list query re-fetches. Today the
strategies list does not, and the operator complaint implies a
category-wide audit gap.

Audit each chat-rail tool-result handler that creates / updates /
deletes a record, confirm the matching query is invalidated.

# Out of scope

- Server-side push (websockets, SSE for list mutations from other
  clients). This is single-operator live-refresh.
- Optimistic updates / cache patching. Invalidate-and-refetch is the
  baseline; optimistic UI is a polish follow-up.
- Backend changes — the dashboard already returns the right shape;
  the gap is purely on the frontend invalidation wiring.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/chat-rail-strategy-list-refresh status
git -C .worktrees/chat-rail-strategy-list-refresh log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/chat-rail-strategy-list-refresh \
  -b task/chat-rail-strategy-list-refresh origin/main
```

# Notes

**Path correction (2026-05-18):** initial contract scoped
`allowed_paths` to `components/chat/**`, but the chat rail's
SSE-event consumer lives in `components/shell/ChatRail.tsx::applyEvent`
(line 396-444). That's where `tool_result` events are processed, so
that's where invalidation has to hook in. Added `ChatRail.tsx` +
test file + `api/chat_rail.ts` to allowed_paths.

The audit confirmed the gap is **never invalidates** — zero
`invalidateQueries` calls exist in `components/chat/**` or
`components/shell/ChatRail.tsx`. Today the chat rail mutates server
state via tool calls but TanStack Query has no idea so list queries
stay stale until the operator hard-refreshes.

Append checkpoints / PR links below.

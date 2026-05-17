---
track: agent-run-observability-ui
branch: task/agent-run-observability-ui
worktree: .worktrees/agent-run-observability-ui
status: done
updated_at: 2026-05-17
---

# Status

Mock -> real cutover for agent-runs API shim + retention badge/banner UI.

## Plan

1. Add `VITE_USE_MOCK_AGENT_RUNS` env flag in `agent-runs.ts`; default to mock
   in dev/test, real HTTP otherwise.
2. Inline runtime validator for `AgentRunDetail` response shape.
3. SSE: `openAgentRunStream` -> `EventSource` with reconnection backoff in
   real mode; mock branch unchanged.
4. Add `retention_mode` to `AgentRunSummary`; default mocks to `hash_only`;
   add badge + `full_debug` warning banner in detail route.
5. New tests in `agent-runs.test.ts` + `agent-runs-detail.test.tsx`.
6. Verify `pnpm test`, `pnpm typecheck`, `pnpm build`.

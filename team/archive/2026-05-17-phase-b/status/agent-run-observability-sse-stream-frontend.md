---
track: agent-run-observability-sse-stream-frontend
worktree: .worktrees/agent-run-observability-sse-stream-frontend
branch: task/agent-run-observability-sse-stream-frontend
phase: in-progress
last_updated: 2026-05-17T14:13:20Z
owner: claude-opus-4.7-1m
---

# What I'm doing right now

Picked up after #227 (UI mock→real openAgentRunStream) and #235 (SSE
backend) merged to `main`. Wiring the SSE consumer:

1. Extend `AgentRunStreamEvent` union with real wire variants (additive;
   keep mock `summary` / `span` arms).
2. Map SSE `event:` names → typed events in `openAgentRunStream`'s real
   branch, keeping existing exponential-backoff reconnect.
3. Add `streamingState` slice + reducers to `trace-dock.ts`.
4. SpanInspector streaming indicator + RunStatusStrip live-active-span.
5. Tests: ≥3 reducer + 1 inspector + 1 status strip + EventSource mock
   tests in `agent-runs.test.ts`.

Verification: `pnpm test`, `pnpm typecheck`, `pnpm build` from
`frontend/web/`.

# Blocked on

Nothing — #227 + #235 are merged.

# Next up

Open PR; update `qa-trace-error-surfacing` (blocked-on note) once this
lands.

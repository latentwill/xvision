---
track: agent-run-observability-sse-stream-frontend
lane: leaf
wave: agent-run-observability-followups
worktree: .worktrees/agent-run-observability-sse-stream-frontend
branch: task/agent-run-observability-sse-stream-frontend
base: origin/main
status: pr-open
depends_on:
  - agent-run-observability-ui            # PR #227 — mock→real openAgentRunStream surface
  - agent-run-observability-sse-stream    # PR #235 — backend SSE route + wire protocol
blocks:
  - qa-trace-error-surfacing              # frontend half needs the streaming reducers landed here
stacking: none
allowed_paths:
  - frontend/web/src/api/agent-runs.ts
  - frontend/web/src/api/agent-runs.test.ts
  - frontend/web/src/api/types-agent-runs.ts
  - frontend/web/src/stores/trace-dock.ts
  - frontend/web/src/stores/trace-dock.test.ts
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/SpanInspector.test.tsx
  - frontend/web/src/features/agent-runs/RunStatusStrip.tsx
  - frontend/web/src/features/agent-runs/RunStatusStrip.test.tsx
forbidden_paths:
  - crates/**
  - xvision-agentd/**
  - frontend/web/src/features/agent-runs/TopbarModeToggle.tsx
interfaces_used:
  - GET /api/agent-runs/:id/stream (SSE — landed in #235)
  - openAgentRunStream callback contract (landed in #227)
parallel_safe: false
parallel_conflicts:
  - qa-trace-error-surfacing (frontend half — wants the same streaming reducers; can stack on top once this lands)
  - qa-eval-trace-fidelity (prompt/completion preview wants `assistant_text_delta` from #234 + the reducers landed here)
verification:
  - (cd frontend/web && pnpm test)
  - (cd frontend/web && pnpm typecheck)
  - (cd frontend/web && pnpm build)
acceptance:
  - `openAgentRunStream` real branch (post-#227) maps SSE `event:` names (`snapshot`, `run_started`, `model_call_finished`, `tool_call_started`, `tool_call_finished`, `tool_call_failed`, `tool_call_cancelled`, `assistant_text_delta`, `sidecar_error`, `lagged`, plus the lifecycle variants) into typed events and routes them through the callback. Existing exponential-backoff reconnect retained.
  - `AgentRunStreamEvent` union in `types-agent-runs.ts` extended to cover the real wire variants (additive — `summary` + `span` arms kept for the mock branch).
  - `trace-dock.ts` gains a `streamingState` slice: `{ activeSpanIds: Set<string>, deltaCharsBySpan: Record<string, number>, droppedEvents: number }` and reducers `markSpanActive(spanId)`, `markSpanInactive(spanId)`, `appendDelta(spanId, len)`, `recordLag(n)`.
  - `SpanInspector.tsx`: when a `model.call` span is selected AND `activeSpanIds` contains it, render a "Streaming response… (N chars)" indicator with the accumulated delta-len count. Falls back to the persisted prompt/response hash display once the stream finishes for that span.
  - `RunStatusStrip.tsx`: when a stream is open and any span is active, show the currently-active span (highest `started_at` among active spans) with elapsed time. Existing post-hoc behavior unchanged.
  - On `lagged`, increment `droppedEvents` and surface a quiet inline warning in the dock header (no popup — `console.warn` + a small badge if the count > 0). The connection retry is already handled by the existing EventSource reconnect path.
  - Tests: 3+ for the trace-dock reducer (snapshot ingest, delta accumulation, lag handling); 1 for SpanInspector streaming indicator; 1 for RunStatusStrip live-active-span display. Mock the EventSource for the api/agent-runs.test.ts additions.
---

# Scope

Frontend consumer for the SSE wire protocol shipped in #235. Wires the
streaming events into the trace dock store and renders live indicators
in the inspector + status strip. Stream-only — delta text is not
persisted (per Phase A privacy decision), so the inspector shows a chars-
so-far counter rather than the actual text; full response materialises
when `model_call_finished` arrives via the snapshot refresh.

Closes the "frontend wiring deferred to follow-up" note in
`team/status/agent-run-observability-sse-stream.md`.

# Out of scope

- Backend changes — the SSE route + wire protocol are stable on #235.
- Persisting delta text — Phase A decision is hash-only by default;
  full text only under `redacted` / `full_debug` retention modes, and
  those still don't surface to the streaming layer.
- Adopting `assistant_text_delta` content rendering — the delta carries
  `delta_len` only, not text.
- Re-rendering the trace dock layout — additive only.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-run-observability-sse-stream-frontend status
```

If the worktree does not exist (create only after #227 + #235 merge):

```bash
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-sse-stream-frontend \
  -b task/agent-run-observability-sse-stream-frontend origin/main
```

# Notes

Append checkpoints / PR links below.

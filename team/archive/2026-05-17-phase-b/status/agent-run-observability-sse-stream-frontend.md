---
track: agent-run-observability-sse-stream-frontend
worktree: .worktrees/agent-run-observability-sse-stream-frontend
branch: task/agent-run-observability-sse-stream-frontend
phase: pr-open
last_updated: 2026-05-17T14:27:07Z
owner: claude-opus-4.7-1m
---

# What I'm doing right now

PR open. Branch was rebased onto `origin/main` after PR #239
(`qa-eval-trace-fidelity`) merged earlier in the day, so the
streaming indicator preempts the new `model.call` post-hoc
hash/ref fallback shipped in that PR (per the queue note left by
#239).

# Done in this PR

- `AgentRunStreamEvent` union extended additively with the real
  SSE wire variants (`snapshot`, `run_started`, `run_finished`,
  `run_interrupted`, `span_started`, `span_finished`,
  `model_call_finished`, `tool_call_started`, `tool_call_finished`,
  `tool_call_failed`, `tool_call_cancelled`,
  `assistant_text_delta`, `sidecar_error`, `lagged`). Mock arms
  (`span` / `summary`) retained.
- `openAgentRunStream` real branch subscribes to all wire events,
  validates the leading `snapshot` via `validateAgentRunDetail`,
  retains exponential-backoff reconnect, and dispatches each
  parsed event into the `trace-dock` store as a side effect.
- `trace-dock` store grew a `streamingState` slice:
  `{ activeSpanIds, activeSpanMeta, deltaCharsBySpan,
  droppedEvents }` plus `markSpanActive` / `markSpanInactive` /
  `appendDelta` / `recordLag` / `applyStreamEvent` /
  `resetStreamingState` reducers. **Slight spec deviation**:
  added `activeSpanMeta` alongside `activeSpanIds` so the
  RunStatusStrip can render a chip (name + kind label + elapsed
  ms) from the streaming slice alone, without a second query for
  span detail. `setActiveRun` resets the slice between runs.
- `SpanInspector` reads the streaming slice. For `isLive`
  `model.call` spans that are in `activeSpanIds`, it renders a
  `Streaming response… (N chars)` indicator with
  `data-testid="span-inspector-streaming"`, **preempting** the
  PR-#239 RESPONSE hash/ref fallback. Once the span leaves the
  active set, the fallback re-appears.
- `RunStatusStrip` derives a `CurrentSpanChip` from the streaming
  slice when `currentSpan` prop is null and `isLive` is true,
  picking the highest `started_at` among active spans. Explicit
  prop still wins; post-hoc runs ignore stale streaming state.
- 17 new tests: 5 reducer (trace-dock), 2 inspector (preempt +
  finish-fallback), 3 status strip (live-derive + prop-precedence
  + post-hoc-ignore), 2 EventSource mock (event mapping +
  malformed-frame resilience), plus rebase-touch ups of existing
  inspector tests. Total: 258 → 275 tests (suite still green).

# Spec deviations + follow-ups

- **Slice schema:** added `activeSpanMeta` next to the
  `activeSpanIds` set spelled out in the contract acceptance, so
  the strip can render without an external span lookup. The set
  is still there; `activeSpanMeta` is purely additive.
- **No actual prompt/response text** in the streaming indicator
  — `AssistantTextDelta` carries `delta_len` only, by design.
  Filed queue note
  `team/queue/agent-run-observability-sse-stream-frontend__2026-05-17T141500Z__blob-fetch-route-needed.md`
  proposing a `crates/**` follow-up to add
  `GET /api/agent-runs/:id/blobs/:ref` so a future leaf can
  hydrate the on-disk payload.

# Blocked on

Nothing.

# Next up

PR review. After merge: file the blob-fetch route contract
referenced in the queue note.

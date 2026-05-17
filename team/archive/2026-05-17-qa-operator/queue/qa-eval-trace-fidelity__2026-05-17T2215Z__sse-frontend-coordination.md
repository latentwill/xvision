---
from: qa-eval-trace-fidelity
to: agent-run-observability-sse-stream-frontend
sent_at: 2026-05-17T22:15Z
topic: SpanInspector / agent-runs.ts overlap
---

Heads up — landing PR (qa-eval-trace-fidelity) edits:

- `frontend/web/src/api/agent-runs.ts`: normalizer now joins
  `model_calls[]` onto matching spans (sets `span.provider`,
  `span.model`, `span.tokens_in/out`, `span.cost`, `span.hash` (prompt
  hash), `span.response_hash`, `span.prompt_payload_ref`,
  `span.response_payload_ref`).
- `frontend/web/src/api/types-agent-runs.ts`: adds
  `response_hash?`, `prompt_payload_ref?`, `response_payload_ref?` to
  `RunSpan`.
- `frontend/web/src/features/agent-runs/SpanInspector.tsx`: renders
  hash-only / payload-ref preview for `model.call` spans when no
  raw `prompt`/`response` text is present; adds Rows for
  `response.hash`, `prompt.ref`, `response.ref`.

When your contract layers in the `streamingState` slice + delta
indicator, the streaming-active branch should preempt the hash-only
fallback I added (your acceptance #4 already describes this: "Falls
back to the persisted prompt/response hash display once the stream
finishes for that span"). My code path is the post-stream fallback.

# Blob-fetch route gap

Acceptance criterion #2 in qa-eval-trace-fidelity ("prompt + completion
preview") can never resolve to actual text from the snapshot alone:
`AssistantTextDelta` is stream-only with `delta_len` only (no text),
and `prompt_payload_ref` / `response_payload_ref` point at blobs with
no fetch route to dereference them. A future contract should add
`GET /api/agent-runs/:id/blobs/:ref` (or similar) so the inspector can
load the on-disk payload when retention is `summaries`/`full_debug`.
Filing under your wave for visibility, since the SSE follow-up already
owns adjacent surface area.

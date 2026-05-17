---
track: qa-eval-trace-fidelity
status: in-progress
owner: claude/2026-05-17
last_update: 2026-05-17T21:30Z
---

# Status

Unblocked 2026-05-17 by the mass Phase B merge (PRs #224, #234, #235
all on `main`).

Per the contract's Conductor note, the arrow-icon resize was already
absorbed into `qa-ui-micro-fixes` (#229, merged). Remaining work:

1. **Per-call model id** — spans currently show only what `RunSpan` carries.
   The export response from `GET /api/agent-runs/:id` includes a
   `model_calls[]` table with `provider`/`model` per `span_id`, but the
   normalizer in `frontend/web/src/api/agent-runs.ts` discards this when
   shaping `RunSpan`. Fix: join `model_calls` back into the matching
   spans during normalization so SpanInspector's existing `provider` /
   `model` rows render real per-call data instead of nothing.

2. **Prompt + completion preview** — what's available in the snapshot
   today:
   - `model_calls[].prompt_hash` (always)
   - `model_calls[].response_hash` (when call completed)
   - `model_calls[].prompt_payload_ref` / `response_payload_ref` (only
     when retention is `summaries` or `full_debug`)
   - `AssistantTextDeltaEvent` is **stream-only and explicitly not
     persisted** (`sqlite.rs:274` — "Stream-only; not persisted. Plan
     ADR.") and the event itself only carries `delta_len`, not the text.

   That means a literal "prompt text + completion text" preview cannot
   come from the snapshot path under any retention mode without a new
   blob-fetch HTTP route (which would be a `crates/**` change and is
   out of scope for this contract). The pragmatic interpretation is:
   surface hashes + payload refs + retention-mode context in SpanInspector
   so operators can at least pivot to the on-disk payload via CLI.

   A queue note will be filed for whichever follow-up owns the
   blob-fetch route — that's the missing piece for real text preview.

## Out of scope this PR

- SSE wiring for `model_call_finished` / `assistant_text_delta` — owned
  by `agent-run-observability-sse-stream-frontend` (not yet filed as
  contract). The dock's SSE handler currently subscribes only to the
  `span`/`summary` event names which the backend doesn't emit; that's
  the follow-up.
- Blob-fetch route to dereference `prompt_payload_ref` /
  `response_payload_ref` into raw text.
- Arrow-icon resize (already in qa-ui-micro-fixes PR #229).

## Allowed-paths note

Contract's `allowed_paths` list references `SpanDetail.tsx` /
`StripDockSlot.tsx` which were renamed pre-merge. Working files are
`SpanInspector.tsx` and `TraceDock.tsx`. Treating the intent
(span-detail + dock + agent-runs API) as authoritative.

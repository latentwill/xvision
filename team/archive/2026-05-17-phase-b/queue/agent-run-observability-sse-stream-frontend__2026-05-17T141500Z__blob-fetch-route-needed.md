---
from: agent-run-observability-sse-stream-frontend
to: conductor
created_at: 2026-05-17T14:15:00Z
topic: follow-up — blob-fetch HTTP route needed for real prompt/response preview
---

# Context

PR #239 (`qa-eval-trace-fidelity`, merged 2026-05-17) noted that
SpanInspector can only show:

- hash (under `hash_only` retention) — body not on disk.
- payload ref (under `summaries`/`full_debug` retention) — body on
  disk but **no HTTP route exists** to dereference the ref from the
  browser.

This frontend track (#239 → #240 succession) just landed the SSE
streaming indicator, which preempts the post-hoc fallback while a
`model.call` span is in-flight. The indicator shows accumulated
character count from `assistant_text_delta` frames (the wire only
carries `delta_len`, not text — by design, per Phase A privacy
decision in `docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md`
§retention). So even with the streaming wiring complete, **operators
still cannot see actual prompt/response text** in the inspector.

To close that gap, a blob-fetch route is needed:

```
GET /api/agent-runs/:id/blobs/:ref
```

It should:

1. Accept a `prompt_payload_ref` / `response_payload_ref` (as already
   exposed on `model.call` spans).
2. Stream the blob bytes from the recorder's on-disk blob store.
3. Gate by retention mode — return 403 when the ref's owning row is
   `hash_only` (defense in depth; refs shouldn't exist there in the
   first place, but the route should not silently 200).
4. Apply the same auth gating as `/api/agent-runs/:id` (see #237).

# Why not me

That's a `crates/**` change (likely `xvision-dashboard/src/routes/agent_runs.rs`
+ a new `xvision-observability` accessor). This contract's
`allowed_paths` is frontend-only; touching crates here would have
required a contract-update PR.

# Suggested next contract

- track: `agent-run-observability-blob-fetch-route`
- lane: leaf
- allowed_paths: `crates/xvision-dashboard/src/routes/agent_runs.rs`,
  `crates/xvision-observability/src/blob_store.rs` (or whatever the
  blob-store accessor file is), the new test file alongside.
- depends_on: none (the producer/recorder already writes the blobs;
  the route just needs to surface them).
- blocks: nothing currently blocked, but unlocks the "real
  prompt/response preview" panel that #239 disclaimed.

After the route lands, a small frontend follow-up to swap the
`payload ref:` line in `SpanInspector` for a fetch-on-demand
`<details>` block would close the loop. That's a leaf and can stack
on this track.

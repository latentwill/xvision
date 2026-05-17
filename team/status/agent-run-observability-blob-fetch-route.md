---
track: agent-run-observability-blob-fetch-route
worktree: .worktrees/agent-run-observability-blob-fetch-route
branch: task/agent-run-observability-blob-fetch-route
phase: pr-open
last_updated: 2026-05-17T14:45:20Z
owner: claude-opus-4.7-1m
---

# What I'm doing right now

Done. PR open. Closes the queue note filed by
`agent-run-observability-sse-stream-frontend` (PR #243) — surfaces
the on-disk prompt/response payloads referenced by
`prompt_payload_ref` / `response_payload_ref` so SpanInspector
can render actual text under `redacted` / `full_debug` retention.

# Done in this PR

- `xvision-observability::find_blob_owner` — single
  parameterized SQL lookup across `model_calls`,
  `tool_calls`, and `checkpoints`. Returns the run's
  `retention_mode` string when the ref is owned by the run,
  `None` otherwise. 5 unit tests cover: model-call prompt/response
  match, checkpoint refs, cross-run isolation, hash_only run
  (helper returns the mode so the route can refuse), wrong-run /
  wrong-ref → None.
- `xvision-dashboard` route
  `GET /api/agent-runs/:id/blobs/:ref` — validates ref shape
  (`^[0-9a-f]{64}$`, defense vs. path traversal), looks up owner,
  403 on `hash_only`, 404 when ref isn't owned by run or missing
  on disk, otherwise reads from `BlobStore` and returns bytes
  with `Content-Type: application/octet-stream` +
  `Cache-Control: private, no-store`. No `Content-Disposition`
  (inline preview, not download). Route registered in
  `server.rs` alongside the other agent-runs endpoints. 6
  integration tests cover 200/400/403/404 across four shapes.
- Frontend `fetchAgentRunBlob(runId, ref)` helper + a
  `PayloadRefDetails` sub-component in `SpanInspector` that
  swaps the inline `payload ref:` text for a `<details>` element
  with the ref as `<summary>`. Click to expand → first-load
  fetch → body rendered as `<pre>` text. Errors land inline as
  muted text (no popup; project UI rule). One-shot fetch
  (collapse + re-expand doesn't re-fetch). `runId` is read from
  the `trace-dock` store's `activeRunId` so the inspector stays
  a leaf prop-wise.
- 6 new tests on the frontend: 4 `fetchAgentRunBlob` (200 round
  trip, URL encoding, 403 + 404 → `ApiError`), 2 SpanInspector
  (expand → fetch → body shown; 403 → inline error). Total
  suite: 258 → 266; all green.

# Verification

- `cargo test -p xvision-observability` — green (existing 14
  tests + 5 new = 19).
- `cargo test -p xvision-dashboard --test agent_runs_blob_route`
  — 6/6 green.
- `cargo check -p xvision-dashboard` — clean (pre-existing
  warnings only).
- `pnpm typecheck` — clean.
- `pnpm test --run` — 266/266 green.
- `pnpm build` — clean Vite build.

Pre-existing `crates/xvision-dashboard/tests/http.rs` failures
(`create_scenario_*`, `eval_compare_*`) confirmed present on
`origin/main` before this PR — unrelated to this change.

# Blocked on

Nothing.

# Next up

PR review.

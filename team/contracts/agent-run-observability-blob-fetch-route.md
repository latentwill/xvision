---
track: agent-run-observability-blob-fetch-route
lane: leaf
wave: agent-run-observability-followups
worktree: .worktrees/agent-run-observability-blob-fetch-route
branch: task/agent-run-observability-blob-fetch-route
base: origin/main
status: pr-open
depends_on:
  - agent-run-observability-sse-stream-frontend  # PR #243 — exposes prompt/response refs in SpanInspector
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-observability/src/blobs.rs
  - crates/xvision-observability/src/export.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-dashboard/src/routes/agent_runs.rs
  - crates/xvision-dashboard/src/server.rs
  - crates/xvision-dashboard/src/state.rs
  - crates/xvision-dashboard/tests/agent_runs_blob_route.rs
  - frontend/web/src/api/agent-runs.ts
  - frontend/web/src/api/agent-runs.test.ts
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/SpanInspector.test.tsx
forbidden_paths:
  - xvision-agentd/**
  - crates/xvision-engine/migrations/**
  - frontend/web/src/features/agent-runs/TopbarModeToggle.tsx
interfaces_used:
  - xvision_observability::BlobStore (read by hex ref)
  - sqlx::SqlitePool (lookup model_calls / tool_calls / checkpoints rows)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-observability
  - cargo test -p xvision-dashboard
  - (cd frontend/web && pnpm typecheck)
  - (cd frontend/web && pnpm test --run)
  - (cd frontend/web && pnpm build)
acceptance:
  - New helper `find_blob_owner(&pool, run_id, ref) -> Result<Option<RetentionMode>, ExportError>` in xvision-observability looks up whether `ref` is referenced by any `model_calls.{prompt,response}_payload_ref`, `tool_calls.{input,output}_payload_ref`, or `checkpoints.{input,output}_payload_ref` row whose owning `run_id` matches. Returns the run's `retention_mode` on hit. SQL is one query via UNION ALL, parameterized.
  - New route `GET /api/agent-runs/:id/blobs/:ref` in `xvision-dashboard`. Auth gating follows the existing `agent-runs::get` pattern (covered by the `qa-dashboard-auth-hardening` gate). Behavior:
      - Validate `:ref` matches `^[0-9a-f]{64}$`; 400 otherwise (defense in depth against path traversal).
      - Call `find_blob_owner`. 404 if `None`.
      - If retention_mode is `hash_only`, 403 (refs shouldn't exist in that mode; this is defense in depth).
      - Read the blob from `BlobStore` rooted at `<xvn_home>/agent_runs/blobs/`. 404 on `BlobStoreError::NotFound`.
      - Return bytes with `Content-Type: application/octet-stream` and `Cache-Control: private, no-store` (payload may be sensitive). No `Content-Disposition` — this is for inline preview, not download.
  - Route registered in `server.rs` alongside the other agent-runs endpoints.
  - Integration test in `crates/xvision-dashboard/tests/agent_runs_blob_route.rs`: spin up an in-memory dashboard, seed a run + model_call referencing a known blob, GET the route, assert 200 + matching bytes; assert 404 for bad run id; assert 404 for ref not owned by run; assert 403 when retention_mode = `hash_only`; assert 400 for non-hex ref.
  - Frontend `agent-runs.ts` gains a typed helper `fetchAgentRunBlob(runId, ref): Promise<string>` that GETs the route, returns body as text, and surfaces `ApiError` codes (`forbidden`, `not_found`, `invalid_response`).
  - `SpanInspector.tsx`: when a model.call span shows a payload ref, render a `<details>` (no popup) with the ref as `<summary>`; clicking expands to a fetch-on-demand body preview (the response from `fetchAgentRunBlob`). Errors surface inline as muted text — no toast/popup. Loading state shown as `Loading…`. Already-fetched bodies are cached for the lifetime of the span selection.
  - Tests cover: 1 frontend test for the details-expand → fetch path (mock `fetch`), 1 backend integration test for each of 200/400/403/404. Existing tests still pass.
---

# Scope

Closes the queue note filed by `agent-run-observability-sse-stream-frontend`
(2026-05-17T14:15:00Z): adds the missing HTTP route so the dashboard can
hydrate the on-disk prompt/response payloads referenced by
`prompt_payload_ref` / `response_payload_ref` (and the analogous tool /
checkpoint refs). Without this route, SpanInspector can only show the
ref string, not the body it points to.

Retention discipline preserved: `hash_only` runs never expose blobs
(none should exist; the route still 403s defensively).

# Out of scope

- New retention modes or schema changes.
- A janitor or GC pass over orphaned blobs (existing janitor handles it).
- Hydrating the streaming indicator with actual delta text — the
  `AssistantTextDelta` wire still carries `delta_len` only.
- Tool input/output preview UI — the route covers tool refs, but only
  the model.call surface is wired in this PR. Other surfaces can be a
  follow-up.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-run-observability-blob-fetch-route status
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-blob-fetch-route \
  -b task/agent-run-observability-blob-fetch-route origin/main
```

# Notes

Append checkpoints / PR links below.

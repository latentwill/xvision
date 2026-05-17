---
track: qa-trace-json-download
lane: leaf
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-trace-json-download
branch: task/qa-trace-json-download
base: origin/main
status: pr-open
depends_on:
  - agent-run-observability-export-cli   # in PR #226 — provides GET /api/agent-runs/:id/export.json
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/features/agent-runs/TraceDock.tsx
  - frontend/web/src/features/agent-runs/TraceDock.test.tsx
  - frontend/web/src/features/agent-runs/TraceDownloadButton.tsx
  - frontend/web/src/features/agent-runs/TraceDownloadButton.test.tsx
  - frontend/web/src/api/agent-runs.ts
forbidden_paths:
  - crates/**                              # collapsed scope: backend route exists (#226)
  - crates/xvision-engine/migrations/**
  - crates/xvision-observability/src/bus.rs
  - frontend/web/src/features/agent-runs/TopbarModeToggle.tsx
parallel_safe: false
parallel_conflicts:
  - "qa-eval-trace-fidelity: also edits TraceDock.tsx. Coordinate disjoint regions (download button in toolbar vs span rendering)."
  - "qa-eval-running-status-streaming: edits adjacent components on the same surface. Coordinate."
  - "qa-trace-error-surfacing: also edits TraceDock. Coordinate UI region."
verification:
  - cargo test -p xvision-dashboard
  - cargo clippy -p xvision-dashboard -- -D warnings
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run trace-dock trace-download
  - pnpm --dir frontend/web build
acceptance:
  - Trace dock toolbar carries a "Download trace JSON" button
  - Clicking the button downloads a single JSON file containing every
    span and every event for the active run, sourced from the
    `xvision-observability` event store
  - File name is deterministic: `xvn-trace-<run_id>.json`
  - Format documented inline (top-level keys `run`, `spans`, `events`)
    or referenced from an existing schema doc
  - A new dashboard route serves the export: `GET /api/agent-runs/:id/trace.json`
    (or similar) — added to `team/OWNERSHIP.md` in the same PR
  - Empty / completed runs export without error
  - No regression on the per-span detail / inspect button — those
    remain available
---

# Scope

Add a "download entire trace as JSON" affordance on the trace dock.
Currently the only inspection path is per-span; the operator needs the
full event timeline as a single JSON blob.

Two halves:

1. **Backend route.** Add a dashboard route that queries the
   `xvision-observability` event store for all spans + events for a
   given run id and serves them as a single JSON document. Mirror the
   per-run query already used by retention / janitor.
2. **Frontend button.** Add a "Download trace JSON" control to the
   trace dock toolbar. On click, fetch the route and trigger a
   browser download.

# Out of scope

- Changing the event schema or adding new event variants.
- Per-span CSV export or other formats — JSON only.
- The CLI-side `xvn run inspect` export (`xvn_run.json` + `xvn_report.md`)
  is a separate contract on the Reserved list under
  `agent-run-observability-export-cli`. Coordinate via `team/queue/` to
  share the export format if both land in this wave.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-trace-json-download \
  -b task/qa-trace-json-download origin/main
git -C .worktrees/qa-trace-json-download status
```

# Notes

**Blocked 2026-05-17: Phase B `agent-run-observability-export-cli`
adds `GET /api/agent-runs/:id` and a CLI export.** This contract
should ride on that route rather than inventing a new one. Possible
collapse: once Phase B ships the route, this contract reduces to a
~50-line frontend toolbar button that hits the existing endpoint and
triggers a browser download. If the Phase B route emits
`xvn_run.json` in the right shape, the backend half of this contract
is unnecessary.

Implementation hints (post-Phase-B):

- The observability crate exposes a per-run event iterator used by the
  janitor and retention paths. Reuse it; do not add a new query.
- For large runs the response can be many MB. Stream the response
  rather than collecting into a single `String` if practical, but a
  collected response is acceptable for v1.
- Frontend download trigger: standard `Blob` + `URL.createObjectURL` +
  hidden `<a download>` click pattern.
- Coordinate with `agent-run-observability-export-cli` (Reserved on
  `team/board.md`) — the CLI export format may set a precedent worth
  matching.

# Conductor note (2026-05-17, post-Phase-B-PRs)

**Backend half collapsed.** PR #226 (`agent-run-observability-export-cli`)
landed `GET /api/agent-runs/:id/export.json` returning a fully-formed
`xvn.agent_run.v1` payload with the right `Content-Disposition`
header. This contract is now **frontend-only**:

1. Add `TraceDownloadButton.tsx` to the trace dock toolbar.
2. On click, fetch `/api/agent-runs/:id/export.json` and trigger
   `Blob` + `URL.createObjectURL` + hidden `<a download>` click.
3. Filename: `xvn_run_<id>.json` (matches the backend's
   Content-Disposition default).
4. No new dashboard route; no new Rust code.

Scope is small enough to combine with `qa-trace-error-surfacing`'s
frontend half if a worker picks both up at once — they share the
TraceDock/SpanDetail real estate. Otherwise coordinate file regions.

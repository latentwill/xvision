---
track: qa-trace-json-download
worktree: .worktrees/qa-trace-json-download
branch: task/qa-trace-json-download
phase: ready-for-review
last_updated: 2026-05-17T00:00:00Z
owner: claude-opus
---

# What I shipped

Frontend-only consumer for `GET /api/agent-runs/:id/export.json` (the
route landed in #226). No backend or Rust changes.

- `frontend/web/src/features/agent-runs/TraceDownloadButton.tsx`
  — toolbar button that fetches the export URL with
  `credentials: "include"`, reads `Content-Disposition` for the
  filename (falls back to `xvn_run_<runId>.json`), and triggers a
  download via `Blob` + `URL.createObjectURL` + hidden `<a download>`
  click. Failures surface via `console.warn` per the no-popups rule.
  Exports a `filenameFromContentDisposition(header)` helper that
  handles quoted, unquoted, and RFC 5987 `filename*=` forms.
- `frontend/web/src/features/agent-runs/TraceDownloadButton.test.tsx`
  — 11 tests: 4 header-parser cases + 7 component cases (render,
  fetch URL + credentials, server-supplied filename, default
  filename, blob URL lifecycle, non-2xx warn path, fetch rejection,
  run-id URL encoding).
- `frontend/web/src/features/agent-runs/TraceDock.tsx` — mounts the
  button inside a dedicated `data-testid="trace-dock-export"` region,
  visually separated from the height/pop-out/minimize cluster so the
  blocked `qa-eval-trace-fidelity` and `qa-trace-error-surfacing`
  tracks can add adjacent export-style controls without a merge
  conflict.
- `frontend/web/src/api/agent-runs.ts` — additive
  `agentRunExportUrl(id)` helper. The dashboard's mock-only
  `getAgentRun` / `openAgentRunStream` branching from #227 is
  untouched.

# Verification

- `pnpm --dir frontend/web test` — 237 tests pass across 44 files
  (11 new in TraceDownloadButton.test.tsx; the pre-existing
  TraceDock.test.tsx still passes — body/header/minimize/inspector
  cases all green).
- `pnpm --dir frontend/web typecheck` — clean.
- `pnpm --dir frontend/web build` — clean (`vite build` writes into
  `crates/xvision-dashboard/static/assets/`, gitignored).

# Edge cases handled

- Large runs / slow connections: button shows a busy state (`…`) and
  disables while in-flight; re-enables in `finally` so a slow or
  failed download can be retried. Blob URL is revoked in `finally`
  to avoid leaking memory on large payloads.
- Server omits `Content-Disposition`: filename defaults to
  `xvn_run_<runId>.json` (matches the backend's own default).
- Reserved characters in run id (e.g. slash, space): URL is built via
  `encodeURIComponent`.
- Fetch rejection (network down): warned + button re-enabled, no
  partial blob URL created.
- jsdom test environment logs "Not implemented: navigation" for the
  synthetic anchor click — expected, the test asserts on
  `createObjectURL` and `appendChild`, not on actual navigation.

# Deviations from contract

- The contract's `verification` block lists cargo/clippy/pnpm-lint
  tasks; I ran the frontend-only subset (test, typecheck, build) that
  matches the collapsed scope from the Conductor note. No Rust files
  were touched, so `cargo test -p xvision-dashboard` was not run
  here.
- No PR opened (per the instructions for this track).

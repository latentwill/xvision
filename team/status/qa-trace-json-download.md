---
track: qa-trace-json-download
worktree: .worktrees/qa-trace-json-download
branch: task/qa-trace-json-download
phase: claimed
last_updated: 2026-05-17T00:00:00Z
owner: claude-opus
---

# Claimed

Frontend-only collapse of `qa-trace-json-download`. Backend half landed
in #226 (`GET /api/agent-runs/:id/export.json`). This track adds:

- `TraceDownloadButton.tsx` — toolbar button that fetches the export
  endpoint, parses `Content-Disposition` for the filename, and triggers
  a Blob-based browser download.
- `TraceDownloadButton.test.tsx` — covers render, click → fetch URL,
  successful download path (mocked `URL.createObjectURL`), and error
  path (console.warn fallback per no-popups rule).
- `TraceDock.tsx` — mount the button in the existing toolbar in a
  disjoint region from height controls / pop-out / minimize, leaving
  room for the blocked `qa-eval-trace-fidelity` and
  `qa-trace-error-surfacing` tracks.
- `agent-runs.ts` — additive `agentRunExportUrl(id)` helper.

Verification:
- `pnpm --dir frontend/web test`
- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web build`

# Browser Console Logging Pass

Date: 2026-05-15
Status: Implemented in `codex-browser-console-logging`

## Goal

Make browser-side QA diagnosable from DevTools without exposing prompts,
credentials, chat transcripts, broker secrets, chart payloads, or provider
responses. The browser should show a compact, ordered trail for app boot,
route changes, API calls, query/mutation failures, chat streams, eval launch,
eval cancellation, provider setup, and run chart streams.

## Runtime Controls

Logging is controlled at runtime:

- `window.xvnLog.setLevel("debug" | "info" | "warn" | "error" | "silent")`
- `window.xvnLog.getLevel()`
- `window.xvnLog.enableDebug()`
- `window.xvnLog.disable()`
- `window.xvnLog.dumpBuffer()`
- `window.xvnLog.clearBuffer()`
- `localStorage["xvn.log.level"]`
- `?xvn_log=debug|info|warn|error|silent`

Defaults:

- development: `info`
- production: `warn`

## Redaction Rules

All context is sanitized before it reaches `console.*` or the in-memory ring
buffer.

Redacted keys include `api_key`, `key`, `token`, `authorization`, `secret`,
`password`, `cookie`, `body`, `prompt`, `message`, `content`, `transcript`,
`raw`, and `response`.

Large arrays are summarized by length. Long strings are truncated unless they
are known-safe identifiers/status fields.

## Instrumented Surfaces

- `frontend/web/src/lib/logger.ts`: central logger, redactor, ring buffer,
  trace IDs, global `window.xvnLog`, unhandled error/rejection logging.
- `frontend/web/src/main.tsx`: app boot.
- `frontend/web/src/App.tsx`: QueryClient query/mutation failures and route
  navigation.
- `frontend/web/src/api/client.ts`: `apiFetch` start, success, HTTP error,
  abort, network error, and JSON parse error.
- `frontend/web/src/api/chat_rail.ts`: POST/SSE chat stream lifecycle,
  streamed event counts, tool start/completion, malformed frames, aborts, and
  stream errors.
- `frontend/web/src/api/eval.ts`: eval launch and cancellation.
- `frontend/web/src/api/chart.ts` and
  `frontend/web/src/components/chart/use-run-stream.ts`: run SSE open, events,
  parse errors, terminal status, reconnects, chart snapshot load, and chart
  merge summaries.
- `frontend/web/src/api/settings.ts` and provider settings UI: provider/broker
  load, save, model catalog, test connection, and form submit summaries.

## Acceptance

- `rg "console\\." frontend/web/src` should only find logger internals or test
  spies.
- `window.xvnLog.enableDebug()` increases detail immediately.
- Failed eval launch has a visible trail from route action through API error.
- Chat stream failures show stream ID, event index, parser/frame summary, and
  provider/model without raw text.
- Eval stream reconnect and terminal states show `run_id`, `trace_id`, event
  count, and phase.
- `window.xvnLog.dumpBuffer()` returns the last 500 sanitized entries.

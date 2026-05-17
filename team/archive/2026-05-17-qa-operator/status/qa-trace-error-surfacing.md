---
track: qa-trace-error-surfacing
status: pr-open
last_update: 2026-05-17
worker: Claude (xvision conductor session)
---

## Outcome

PR #238 open. Branch pushed (commit `5804b2a`).

This contract delivers two narrowly-scoped pieces. The third (engine
instrumentation) is filed as a follow-up because the gap is wider than
the contract's original scope anticipated.

## What shipped

1. **`RunSpan.error_message` first-class field** (`frontend/web/src/api/types-agent-runs.ts`).
   The observability export already carries `error_json: Option<String>`
   per span, but the frontend `flattenExportSpans` was discarding it.
   New `parseErrorJson` helper accepts both
   `{"message": "..."}`-shaped JSON and bare strings; the parsed
   message is attached as `error_message` on the `RunSpan`.

2. **`SpanInspector` renders the error** (`frontend/web/src/features/agent-runs/SpanInspector.tsx`):
   - Header strip shows a red `ERROR` badge in place of `STREAMING`
     when `span.status === "error"`.
   - Body renders an `ERROR` pull-quote at the top (before
     prompt/response) carrying the parsed message — operator's primary
     debug signal.

## Investigation note: the wider gap

The operator's specific complaint was an **eval run** failing with
`[unclassified] error decoding response body: EOF while parsing a
value at line 1145 column 0` that did NOT appear in the trace.
Phase B IPC emission (#224 / #234 / #235) was billed as the unblocker,
but the actual gap is structural and Phase B does not bridge it:

### Producer side: eval runs do not connect to the observability bus

Two separate event buses exist in the workspace and they share a name:

- `xvision_observability::RunEventBus` — the agent-run observability
  bus (typed `RunEvent` payloads → `recorder` → `spans` / `model_calls`
  / `tool_calls` tables → `/api/agent-runs/:id` JSON).
- `xvision_engine::api::chart::RunEventBus` — the live-chart event
  bus (`RunChartEvent` payloads → SSE for the eval-runs detail
  surface's equity curve / live decisions panel).

`crates/xvision-engine/src/eval/executor/{backtest,paper}.rs` accept
the **chart** bus (via `with_event_bus`) but never construct or
receive the **observability** bus. `agent/execute.rs::execute_slot`
is the actual LLM call site for evals (and for the wizard / chat-rail);
it returns `anyhow::Result<LlmResponse>` and propagates errors via `?`
to its caller — there is no `SpanStarted`/`ModelCallStarted` emission
around the dispatch, so a failed call never lands on any bus.

Phase B IPC emission (#224 / #234) wires the **sidecar's**
notifications into the observability bus. Eval runs do not use the
sidecar — they call `LlmDispatch::complete` directly. So Phase B
unblocks `/agent-runs` traces for sidecar-routed work (wizard,
chat-rail, future agent-driven workflows) but leaves eval runs
unobserved.

### Proposal: `qa-eval-observability-wiring` follow-up

A future contract should:

1. Make the eval executors hold an `Option<Arc<xvision_observability::RunEventBus>>`
   alongside the existing chart bus.
2. Wrap `execute_slot` so every dispatch emits `SpanStarted`
   immediately before `dispatch.complete(req).await`, `SpanFinished`
   with the appropriate status afterward, and `ModelCallStarted` /
   `ModelCallFinished` (or `ToolCallFailed` when wrapping the error
   classifier in `xvision-engine/src/agent/llm.rs`).
3. Register the eval `run_id` with the observability run-table so
   `/api/agent-runs/<eval_run_id>` returns spans.
4. Either dual-route (eval runs appear in both surfaces) or unify
   (`/eval-runs/:id` reads spans from the observability tables).

That work touches `eval/executor/{backtest,paper}.rs`, `agent/execute.rs`,
`agent/llm.rs`, and the dashboard route layer — out of scope for this
contract's allowed_paths beyond a follow-up note.

### What the user-facing fix in this PR does cover

The error-rendering improvements DO apply to every agent run that
already flows through observability (wizard tool runs, future
sidecar-driven workflows, the `agent-run-observability-ui` mock
fixtures). When `qa-eval-observability-wiring` lands, eval-run errors
will surface in the same SpanInspector unchanged — the rendering is
producer-agnostic.

## Path drift handled (conductor amendments)

- Contract listed `SpanDetail.tsx` / `SpanDetail.test.tsx`; the actual
  files are `SpanInspector.tsx` / `SpanInspector.test.tsx`. Added
  both names to `allowed_paths` (keeping SpanDetail as future-proofing
  in case a separate component is added).
- Added `frontend/web/src/api/types-agent-runs.ts` to allowed_paths
  — needed for the new `error_message?: string` field on `RunSpan`.
- Added `frontend/web/src/api/agent-runs.test.ts` to allowed_paths
  — new regression test for `parseErrorJson` + `flattenExportSpans`.

## Verification

| Command | Result |
|---|---|
| `pnpm --dir frontend/web typecheck` | clean |
| `pnpm --dir frontend/web test -- --run agent-runs SpanInspector` | 91/91 pass (15 files, 6 new tests) |
| `pnpm --dir frontend/web build` | clean |

Engine + observability tests not run for this PR (no Rust changes).

## Out of scope (deferred to `qa-eval-observability-wiring`)

- Engine-side instrumentation of `execute_slot` with observability
  span/model-call events.
- Wiring `xvision_observability::RunEventBus` into eval executors.
- Unifying eval-run-id with observability run-id so eval runs show
  up at `/agent-runs/<id>`.
- New `model_call_failed` event variant (existing `SpanFinished`
  with `status: Error` + `error_json` is sufficient for now).

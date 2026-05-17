---
track: agent-run-observability-ipc-emission
owner: claude (autonomous Phase B sprint)
status: pr-ready
last_update: 2026-05-17
---

## Current

Branch `task/agent-run-observability-ipc-emission` pushed to origin.

Implements Phase B Step 8 (Cline SDK migration):
- Sidecar event-socket notification surface (`xvision-agentd/src/transport/event-client.ts`)
- Per-tool / per-step emit hooks (`xvision-agentd/src/session/emit.ts`, `active-run.ts`, updates to `tool-shim.ts`, `methods/session.ts`)
- Rust translation layer (`xvision-agent-client/src/event_sink.rs`) — 1:1 mapping from notification kinds to `RunEvent`s, with span lifecycle pairs expanded around tool/model details so the recorder writes both `spans` and detail rows
- Sidecar fingerprint stamping (sidecar_version / cline_sdk_version / protocol_version captured from handshake, threaded onto every `RunStarted`)
- `mark_runs_interrupted` helper for supervisor crash detection

Verification (all green):
- `cargo test -p xvision-agent-client` — 4 unit + 3 smoke + 6 existing integration tests pass
- `cargo test -p xvision-agent-client --test event_sink_smoke` — drives a fake sidecar end-to-end against a real `SqliteRecorder`; asserts agent_runs / spans / tool_calls / model_calls rows land with correct fingerprint, tool data, and token counts
- `pnpm test` (xvision-agentd) — 55 tests pass including new `event-emission.test.ts` (id-less JSON-RPC 2.0 round-trip)
- `cargo build -p xvision-engine` clean (downstream consumer)

## Deferred to follow-up

- `event.assistant_text_delta` — requires Cline Agent model wrapping to intercept streaming text; v1 only emits aggregate per-step model_call_finished
- Per-iteration `ModelCallStarted` — same blocker as text deltas; v1 emits a single SpanStarted+SpanFinished pair per step
- `event.overloaded` — sidecar-side queue saturation detection; the Rust dispatch path handles `event.overloaded` notifications when they arrive, but the sidecar's emit-client doesn't currently track outbound depth
- `event.tool_call_cancelled` — Cline cancellation hook surface required first

These do not block downstream Phase B tracks (export-cli, otel-bridge, ui-cutover) — those operate on the recorded rows, which the v1 emission set already produces in full for `agent_runs`, `spans`, `tool_calls`, `model_calls`, and `supervisor_notes`.

## Notes

- Coordination: `qa-agentd-budget-enforcement` (P1) overlaps on `xvision-agentd/src/methods/session.ts` + `session/build-agent.ts` + `crates/xvision-agent-client/src/protocol.rs`. That track has not landed yet; this track does not touch `protocol.rs` (no DTO changes), and the session.ts edits are scoped to emit-call insertion, disjoint from budget enforcement. Rebase impact should be small.

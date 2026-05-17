---
track: agent-run-observability-ipc-emission-v2
owner: claude (autonomous follow-up sprint)
status: claimed
last_update: 2026-05-17
---

## Current

Claimed. Implementing 4 deferred notification kinds on stacked branch
`task/agent-run-observability-ipc-emission-v2` (base: `task/agent-run-observability-ipc-emission`).

Work plan:
1. Add `xvision-agentd/src/session/model-wrapper.ts` (forwarding AgentModel).
2. Add `emitAssistantTextDelta`, `emitModelCallStarted` helpers in `emit.ts`.
3. Wire wrapper into `build-agent.ts` for both mock + real models.
4. Move `model_call_finished` emission from `methods/session.ts` into the wrapper.
5. Add buffer-depth tracking + `event.overloaded` in `transport/event-client.ts`.
6. Add AbortSignal cancellation hook (or feature-gated stub) in `tool-shim.ts`.
7. Extend Rust `event_sink.rs` with dispatch for the 4 new notifications.
8. Tests: vitest + Rust integration (`event_sink_v2.rs`).

## Notes

Coordination: parent branch unchanged; this stacks. No protocol DTO changes
required (notifications already defined). Touches only allowed paths from
the contract.

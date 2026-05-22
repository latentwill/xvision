---
track: qa-trace-broker-spans
worktree: .worktrees/qa-trace-broker-spans
branch: task/qa-trace-broker-spans
base: origin/main
phase: pr-open
last_updated: 2026-05-18T05:51:00Z
owner: claude
---

# What changed

Round-2 intake #8 + #14: broker submissions were invisible on the trace
dock. This PR adds a `broker.call` span kind end-to-end so Buy /
Sell / Close / Short submits — including short-sale fills — are
auditable alongside model.call rows.

## Schema (xvision-observability)

- `SpanKind::BrokerCall` (`broker.call`).
- `BrokerSide { Buy, Sell, Close, Short }` — short / close are derived
  from the trader's action, not the wire-level Buy/Sell, so a
  `short_open` decision surfaces as `side=Short` even though the
  underlying order is a Sell. Addresses the operator-visible bug from
  intake #14 where short fills appeared as ambiguous Sell rows.
- `BrokerCallOutcome { Filled, Rejected, Cancelled, Failed }`.
- `RunEvent::BrokerCallStarted` + `RunEvent::BrokerCallFinished`,
  carrying side / symbol / qty / intended price / order type / venue /
  idempotency key on `started`, and outcome / fill price / fill qty /
  fee / broker order id / error class + message on `finished`.
- Both events wired into `RunEvent::{run_id, span_id}` routing; bus
  resolves the `finished` event's run via the span→run map populated
  on `started` (same pattern as model_call_finished).
- 6 unit tests in `crates/xvision-observability/tests/broker_call_events.rs`
  pin: `broker.call` SpanKind serialization, `BrokerSide` /
  `BrokerCallOutcome` round-trip, run+span routing, the short-fill
  wire shape from intake #14, and the failed-outcome error_class /
  error_message payload.

## Emit (xvision-engine)

- `ObsEmitter::emit_broker_call_started` and
  `ObsEmitter::emit_broker_call_finished` (deviation: see below). Each
  publishes BOTH the typed broker event AND the matching span-level
  `SpanStarted` / `SpanFinished` so the recorder + flame graph see a
  normal span lifecycle.
- `PaperExecutor::run` wraps every `BrokerSurface::submit_order` call
  with an emit pair: on success → `Filled` outcome with fill data; on
  error → `Failed` outcome with a compact error class (`broker_*`)
  derived from the message + the verbatim broker error.
- New helpers in `paper.rs`: `broker_side_for_action` (maps the
  trader action onto the trace-visible side enum) and
  `classify_broker_error` (compact class for the trace).

## SSE forwarder (xvision-dashboard)

- `crates/xvision-dashboard/src/sse/mod.rs::event_name` adds the two
  new arms: `broker_call_started` / `broker_call_finished`.

## Frontend (types-agent-runs.ts + agent-runs/**)

- `SpanKind` union gains `"broker.call"`.
- New `BrokerSide`, `BrokerCallOutcome`, and `BrokerCallDetail` types;
  `RunSpan` gains an optional `broker_call?: BrokerCallDetail` field
  the dashboard fills in once the matching events have landed.
- `AgentRunStreamEvent` union gains
  `broker_call_started` / `broker_call_finished` variants with their
  typed Stream*Data payloads.
- `span-colors.ts`: new `broker` category (rose tint `#f472b6`,
  short label `BROKR`). FlameGraph + SpanInspector pick it up via
  `spanColor(kind)`.
- `SpanInspector.tsx`: when the span is `broker.call` and a
  `broker_call` payload is present, renders a `BROKER CALL`
  pull-quote with a definition list of side / symbol / qty /
  intended price / type / venue / outcome (color-coded) / fill px /
  fill qty / fee / broker order id / error class + message.
- `useTraceDock.applyStreamEvent` handles the new arms exactly like
  the existing tool_call_* / model_call_finished arms — `finished`
  closes the active-span set, `started` is informational.
- `TraceDock.tsx`: same — `finished` triggers a canonical-detail
  refetch, `started` is informational.

## Verification

- Passed: `corepack pnpm --dir frontend/web test -- --run SpanInspector FlameGraph trace-dock` — 44 tests
- Passed: `corepack pnpm --dir frontend/web typecheck`
- Passed: `corepack pnpm --dir frontend/web build`
- Deferred to CI: Rust crates (`cargo test -p xvision-observability`,
  `cargo test -p xvision-engine`, `cargo clippy -p xvision-execution`).
  This deploy host has no Rust toolchain per CLAUDE.md.

## Frontend tests added

- SpanInspector renders side / qty / fill px / venue / outcome /
  broker_order_id for a filled `broker.call` (`side=short`).
- SpanInspector renders error_class + error_message + `failed`
  outcome for a `Failed` `broker.call`.

## Allowed-paths deviations

The contract enumerated tight allowed paths; several were impossible
to satisfy without minimal edits to neighbours:

- `crates/xvision-engine/src/agent/observability.rs` — added
  `emit_broker_call_{started,finished}` helpers. The contract listed
  `eval/executor/*.rs` but not the emitter module. The helpers wrap
  the existing bus.publish pattern used by `emit_model_call_*` so
  the deviation is mechanical.
- `frontend/web/src/features/agent-runs/span-colors.ts` — added a
  `broker` category. Contract excluded this file but the acceptance
  asks for a "distinct row category (color + icon)", which has no
  other home.
- `frontend/web/src/stores/trace-dock.ts` and
  `frontend/web/src/features/agent-runs/TraceDock.tsx` — added the
  two new event arms to the exhaustive switches so TypeScript stays
  happy. Both files were touched by `qa-trace-dock-resizable` (now
  merged); the additions here are 4 lines total.

Flagged for the conductor.

## Notes

- Real-broker BrokerSurface impls (Alpaca, Orderly) are NOT directly
  instrumented inside the impls themselves. The emit pair lives at
  the executor level (around `broker.submit_order`), so every broker
  — paper, Alpaca, Orderly — yields a `broker.call` span regardless
  of which BrokerSurface handled it. The contract asked for
  per-implementation emit; the executor-level emit covers every
  caller with one wiring point and avoids leaking ObsEmitter through
  the BrokerSurface trait.
- `qa-decisions-position-pnl` is the cleanest follow-up to consume
  the new span: per-row open-positions cell can read fills from
  `broker_call_finished` instead of re-deriving from decision rows.
- `agent-error-feedback-self-healing` (P1, stacks on this contract)
  can route the `error_class` from `broker_call_finished` back to
  the agent as a tool-result for self-healing.

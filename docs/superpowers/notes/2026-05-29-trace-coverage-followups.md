# Trace coverage — remaining gaps and follow-ups

Date: 2026-05-29
Branch: `feat/trace-coverage`
Companion to: `docs/superpowers/notes/2026-05-25-clinesdk-trace-alignment-audit.md`,
`docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`

## What this branch landed

Three spans are now emitted with full fidelity across the agentd sidecar →
`xvision-agent-client` event sink → SQLite recorder path:

- **`model.call`** — now carries the full **plaintext prompt and response**
  (`prompt_text` / `response_text` on `ModelCallFinishedEvent`), not just the
  pre-existing `prompt_hash` / `response_hash`. The sidecar `model-wrapper.ts`
  serializes the request and accumulates streamed `text-delta`,
  `reasoning-delta`, and `tool-call-delta` events into the response capture.
  The recorder persists the plaintext to a `model_call_payload` event row.
- **`tool.call`** — unchanged; tool identity, redacted input/output, hashes,
  and status are preserved per existing retention rules.
- **`agent.decision`** — emitted by the `submit_decision` lifecycle tool with
  `bought`/`sold`/`closed`/`held`/`unknown` outcome semantics, the raw action,
  asset, active-position/portfolio context, and the full decision JSON.

**Ordering guarantee:** `event.model_call_finished` (which records prompt +
response) is emitted before `event.decision_recorded`. This is proven by
`xvision-agentd/test/session/submit-decision.test.ts` ("emits model
prompt/response before the decision span") and the unit dispatch tests in
`crates/xvision-agent-client/src/event_sink.rs`.

## Remaining observability gaps (follow-up work)

The engine (`xvision-engine`) eval/backtest path already has emitter helpers for
several spans that are reserved in `SpanKind` but not uniformly emitted from the
live agentd path. The gaps below are candidates for follow-up tracks.

### 1. Filter / pre-trade screen spans

There is no dedicated `SpanKind` for filter (pre-trade screen / capability
`Filter`) stages. Multi-stage strategies whose agents have the `Filter`
capability run a screen before the trader, but that stage is currently only
visible as a generic `model.call` under the run, with no semantic marker that
it rejected vs. passed an asset.

- **Proposed:** add `SpanKind::FilterEval` (`filter.eval`) emitted around each
  filter-capability agent invocation, carrying `{asset, verdict: pass|reject,
  reason}` in `attributes_json`.
- Until then, filter decisions are reconstructable only from the filter agent's
  `model.call` plaintext response (now visible thanks to this branch).

### 2. Risk-gate spans

`RiskDecision` (Approved / Modified / Vetoed) and `RiskLevel` exist as types,
but the risk gate's verdict is not emitted as its own span. A vetoed or modified
trade currently shows up only as a changed downstream order, not as an explicit
risk event.

- **Proposed:** add `SpanKind::RiskGate` (`risk.gate`) bracketing the risk
  stage, with `{verdict: approved|modified|vetoed, risk_level, modified_qty?,
  veto_reason?}`. Emit via a new `ObsEmitter::emit_risk_gate_*` pair mirroring
  the broker-call helpers.
- This closes the "why was the trade resized / blocked" question on the trace
  dock without diffing trader output against the executed order.

### 3. Broker / fill spans (live agentd path)

`SpanKind::BrokerCall` and `emit_broker_call_started` /
`emit_broker_call_finished` already exist in `xvision-engine` and are emitted by
the **eval executor** (side / qty / price / fill status / error class). The gap
is that the **live agentd path** does not emit broker spans — when a decision
results in a real `submit_order`, there is no `broker.call` span on the sidecar
wire.

- **Proposed:** add an `event.broker_call_started` / `event.broker_call_finished`
  notification pair to `xvision-agentd` and a dispatch arm in `event_sink.rs`
  that maps them to the existing `BrokerCallStarted` / `BrokerCallFinished`
  `RunEvent`s, so live fills get the same trace coverage as eval fills.

### 4. Other reserved-but-unemitted spans

- `SpanKind::ToolValidateInput` / `ToolValidateOutput` — wire format pinned by
  F-4 with no-op bodies; F-6 typed schema validator fills them in.
- `SpanKind::RecoveryAttempt` — reserved by F-4; F-5 recovery state machine
  owns emission.
- `SpanKind::StateTransition` — emitted from `emit_state_transition`; verify the
  live agentd path emits the full `Queued → Running → Completed` timeline rather
  than only terminal status.

### 5. Retention / redaction parity

The new `model.call` plaintext fields are subject to the same trace-retention
setting as the rest of the trace surface. Confirm the retention picker
(`feedback_no_privacy_overkill` — the file-backed picker that exists for future
secret redaction) gates `prompt_text` / `response_text` persistence, and that a
future redaction toggle scrubs them before the recorder writes the
`model_call_payload` row. Producers remain responsible for scrubbing secrets out
of free-text fields before serialization (the recorder trusts the producer).

## Priority

1. Risk-gate spans (#2) — highest operator value; veto/resize is currently invisible.
2. Broker/fill spans on the live path (#3) — eval already covered; live is the gap.
3. Filter spans (#1) — needed once multi-stage filter strategies ship widely.
4. Retention/redaction parity (#5) — required before plaintext capture is enabled
   on any surface handling real credentials.

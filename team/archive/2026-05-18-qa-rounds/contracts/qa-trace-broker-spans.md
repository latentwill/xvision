---
track: qa-trace-broker-spans
lane: integration
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-trace-broker-spans
branch: task/qa-trace-broker-spans
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: declared:alpaca-paper-crypto-submit
allowed_paths:
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-execution/src/broker_surface.rs
  - crates/xvision-execution/src/alpaca.rs
  - crates/xvision-execution/src/orderly.rs
  - crates/xvision-execution/tests/broker_surface_spans.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/src/eval/executor/trader_output.rs
  - crates/xvision-dashboard/src/sse/mod.rs
  - frontend/web/src/api/types-agent-runs.ts
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/FlameGraph.tsx
  - frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-observability/src/bus.rs
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/src/redactor.rs
interfaces_used:
  - agent-run-observability event bus
  - BrokerSurface trait
  - TraderDecision → broker submit path
parallel_safe: false
parallel_conflicts:
  - "alpaca-paper-crypto-submit: single-writer claim on crates/xvision-engine/src/eval/executor/paper.rs and edits broker_surface.rs / alpaca.rs. This contract MUST stack on its branch."
  - "qa-decisions-30day-count: also edits eval/executor/. Coordinate disjoint regions."
  - "qa-decisions-position-pnl: may also touch executor / trader_output. Coordinate."
  - "trace-fullscreen-redesign, trace-dock-ux-polish, qa-trace-dock-resizable: all touch frontend/web/src/features/agent-runs/. Coordinate regions; this contract adds a new span kind, not a layout change."
verification:
  - cargo test -p xvision-observability
  - cargo test -p xvision-execution
  - cargo test -p xvision-engine
  - cargo clippy -p xvision-execution -- -D warnings
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run agent-runs SpanInspector FlameGraph
  - pnpm --dir frontend/web build
acceptance:
  - The agent-run-observability event schema gains a `broker_call`
    span kind (or extends an existing span kind with a `broker_call`
    sub-variant) carrying: side (Buy / Sell / Close / Short), symbol,
    qty, intended price, limit/stop type, broker venue, status
    (submitted / filled / rejected / cancelled), fill price, fill qty,
    error class + message on failure.
  - `BrokerSurface` implementations (Alpaca, Orderly) emit
    `broker_call.started` on submit, `broker_call.finished` on fill /
    reject / cancel, and `broker_call.failed` on transport / 5xx
    errors. The paper executor (`paper.rs`) emits the same events for
    its simulated fills.
  - The dashboard SSE forwarder passes the new variant through
    unchanged.
  - The trace dock's flame graph and timeline render broker_call
    spans as a distinct row category (color + icon) alongside
    model.call rows. SpanInspector shows side / qty / price / fill
    status / error in the detail pane.
  - Short-sale fills (#14 in the round-2 intake) become visible on
    the trace as a `broker_call` span with side=Short and a fill
    status, not silently missing.
  - Round-trip test in `crates/xvision-execution/tests/broker_surface_spans.rs`
    asserts that a simulated submit → fill flow emits exactly two
    events (started + finished) carrying the full payload.
  - No schema migration. If the existing schema can't carry the
    payload without a column add, FILE A CONTRACT UPDATE first and
    reserve a migration number through `team/MANIFEST.md`.
---

# Scope

Operator reported (2026-05-18): trace currently shows model.call spans
but not broker calls. Buy / Sell / Close / Short submissions are
invisible on the trace, and short-sale fills don't show up at all.
This contract adds broker-call instrumentation to
`crates/xvision-execution/**` and the paper executor, wires it through
the agent-run-observability bus, and renders the new span kind in the
trace dock.

This is the trace-fidelity follow-up the operator wanted after round 1
landed model-call rendering. Real broker activity must be auditable on
the same surface that shows the LLM activity.

# Out of scope

- New agent-run-observability schema columns / migrations. Reuse the
  existing event variants; if a fit is genuinely missing, push a
  contract update and reserve a migration through `team/MANIFEST.md`
  before adding schema changes.
- The broker_call → live SSE streaming feed cadence — same cadence as
  existing span events; no new SSE channel.
- PnL / position rollup display on the decisions surface
  (`qa-decisions-position-pnl`).
- Replacing the paper executor's fill simulation. Just instrument it.
- Adding a new venue (Coinbase, etc.).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-trace-broker-spans status
git -C .worktrees/qa-trace-broker-spans log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-trace-broker-spans \
  -b task/qa-trace-broker-spans origin/main
```

Stacking note: `alpaca-paper-crypto-submit` holds the single-writer
claim on `paper.rs`. This contract must base its branch on
`task/alpaca-paper-crypto-submit` (declared in frontmatter). Confirm
that contract's status before claiming — if it's still `ready`, push
through it first or coordinate ordering via `team/queue/`.

# Notes

Span-kind name + payload shape decision: prefer extending an existing
`tool_call` variant if the bus already has one with a side / fill
shape; otherwise add `broker_call`. Record the choice in the status
note before the PR opens.

Append checkpoints / PR links below.

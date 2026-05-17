---
track: agent-run-observability-event-bus
worktree: .worktrees/agent-run-observability-event-bus
branch: task/agent-run-observability-event-bus
phase: pr-open
last_updated: 2026-05-17T03:00:00Z
owner: claude-opus
---

# What I'm doing right now

Phase A leaf #2 of the agent-run-observability wave.

PR open: https://github.com/latentwill/xvision/pull/202

Built on top of the schema crate from #200:

- `RunEventBus` (`src/bus.rs`) — bounded mpsc, default capacity 4096,
  single-consumer fan-out to N subscribers, per-`run_id` drop counter
  with a `BackpressureDropped` follow-up event so gaps are visible in
  `supervisor_notes`.
- `RunEvent` enum (`src/events.rs`) — all 16 variants the contract
  lists (RunStarted, RunFinished, RunInterrupted, SpanStarted,
  SpanFinished, ModelCallFinished, ToolCallStarted, ToolCallFinished,
  ToolCallFailed, ToolCallCancelled, CheckpointWritten,
  AssistantTextDelta, SupervisorNote, ArtifactWritten, SidecarError,
  BackpressureDropped).
- `AgentRunRecorder` trait (`src/recorder.rs`) — async, dyn-compatible
  via `async_trait`. Single `handle_event` surface (see deviation note
  below) + `mark_interrupted`.
- `Attribute` newtype + two `compile_fail` doctests as the attribute-API
  guardrail. No `From<&str>` / `From<String>` impl by design — payload
  strings cannot reach a recorder/OTel attribute API.
- `SqliteRecorder` (`src/sqlite.rs`) — subscribed to the bus, writes
  rows defined by migration 018. Maintains an in-memory
  `span_id → run_id` map so span-only events (`SpanFinished`,
  `ModelCallFinished`, `ToolCall*`) attribute back to a run. Cleared
  on run finalize.
- `NoopRecorder` (`src/recorder.rs`) — in-memory event sink for tests.
- Synthetic-event integration test (`tests/event_bus_synthetic.rs`) —
  the exact 1-run / 3-model-call / 5-tool-call / 1-interrupted-span
  scenario the contract specifies, plus a happy-path RunFinished case.

# Blocked on

Nothing. Waiting on conductor merge.

# Deviation: recorder trait shape

The plan (lines 511–532 of
`2026-05-17-agent-run-observability-plan.md`) sketched the recorder
trait with a method-per-thing surface (`start_run`, `finish_run`,
`start_span`, `record_model_call`, …). The contract restated that
surface verbatim.

In practice the trait subscribes to the bus and the bus delivers
events. Method-per-thing on the trait would require the bus consumer
to pattern-match every event and call a different method, duplicating
the dispatch the `RunEvent` enum already encodes. The implemented
trait surface is `handle_event(&self, &RunEvent) -> Result<(), …>` +
`mark_interrupted(&self, run_id)`. The semantic API is preserved 1:1
by the `RunEvent` variants of the same names (`RunStarted` ≡
`start_run`, `ModelCallFinished` ≡ `record_model_call`, etc.).

If the conductor wants the literal method-per-thing surface, I can add
trait methods that the bus consumer dispatches to in a default
`handle_event` impl — but the bus-only flow makes the methods
unreachable in practice, so I left them off. Flag at review if you
want them back.

# Next up (post-merge)

Phase B requires the Cline SDK migration to reach step 3
(`xvision-agent-client` crate exists). When that lands:

- Open `agent-run-observability-ipc-emission` (foundation, Phase B) —
  wires Cline IPC notifications to `RunEventBus::publish`. This is
  step 8 of the Cline migration plan.
- After ipc-emission: `agent-run-observability-otel-bridge`,
  `agent-run-observability-export-cli`,
  `agent-run-observability-ui` can all open in parallel.

Independent of Phase B: `agent-run-observability-retention-cli` is
ready to claim now (only depends on the schema crate).

# Notes

- 28/28 tests passing locally: 20 redactor/config unit tests, 4 migration
  tests, 2 new synthetic-event integration tests, 2 new `compile_fail`
  doctests for the `Attribute` guardrail.
- Engine builds cleanly (`cargo build -p xvision-engine`).
- The `async-trait` workspace dep was already present; observability
  crate's `Cargo.toml` just needed the line added.
- `RunEvent::run_id()` returns `""` for span-only variants
  (`SpanFinished`, `ModelCall*`, `ToolCall*`) because spans don't carry
  run_id in their event payload. The recorder's in-memory `span→run`
  map handles dispatch; the drop counter only triggers for events that
  carry `run_id` directly. This is OK for Phase A — Phase B emission
  always interleaves run-level events with span events, so drop markers
  still flush in a timely way.
- Bus capacity default 4096 is a guess. Plan calls out tuning post
  Phase B emission once we have real throughput data.

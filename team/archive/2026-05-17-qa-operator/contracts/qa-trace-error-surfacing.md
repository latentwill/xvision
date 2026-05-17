---
track: qa-trace-error-surfacing
lane: leaf
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-trace-error-surfacing
branch: task/qa-trace-error-surfacing
base: origin/main
status: pr-open
depends_on: []   # unblocked: #224 + #234 + #235 merged 2026-05-17
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/eval/dispatcher.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/llm/**
  - crates/xvision-observability/src/**
  - frontend/web/src/features/agent-runs/SpanDetail.tsx
  - frontend/web/src/features/agent-runs/SpanDetail.test.tsx
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/SpanInspector.test.tsx
  - frontend/web/src/features/agent-runs/TraceDock.tsx
  - frontend/web/src/api/agent-runs.ts
  - frontend/web/src/api/agent-runs.test.ts
  - frontend/web/src/api/types-agent-runs.ts
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-observability/src/bus.rs
  - frontend/web/src/features/agent-runs/TopbarModeToggle.tsx
parallel_safe: false
parallel_conflicts:
  - "qa-execute-slot-cap (qa-2026-05-17 wave): also edits crates/xvision-engine/src/agent/execute.rs. Coordinate disjoint regions; the cap is iteration-bound, this is error-emission. Stack if needed."
  - "qa-role-normalization (qa-2026-05-17 wave): also edits eval/executor/{backtest,paper}.rs. Coordinate disjoint regions."
  - "qa-remove-agent-max-tokens: also edits eval/dispatcher.rs and agent/execute.rs. Coordinate."
  - "qa-openrouter-pricing-pull: also edits llm/**. Coordinate."
  - "qa-eval-trace-fidelity / qa-trace-json-download: edit SpanDetail / TraceDock. Coordinate region (error display vs prompt display vs download button)."
verification:
  - cargo test -p xvision-engine
  - cargo test -p xvision-observability
  - cargo clippy -p xvision-engine -- -D warnings
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run trace-dock agent-runs
  - pnpm --dir frontend/web build
acceptance:
  - An eval run that fails inside an LLM call (provider error, body
    decode failure, timeout, etc.) produces a trace event carrying the
    error class + message
  - The failing span renders in the trace dock with a visible error
    badge and the error message in the span detail panel — not silently
    omitted or rendered as "Completed"
  - Investigation note (separate from code) lands in `team/status/qa-trace-error-surfacing.md`
    confirming whether trace events wrap the actual LLM call path
    (i.e. that span emission is on the real `xvision-intern` /
    `xvision-engine` call site, not a no-op shim). If a gap is found,
    the contract is allowed to fix the wrapping in `crates/xvision-engine/src/agent/execute.rs`
  - Regression coverage: a test exercises the error path end-to-end
    (simulate provider returning malformed JSON; assert event is
    emitted and surfaced)
---

# Scope

Operator reported (2026-05-17) that an eval run errored with
`[unclassified] error decoding response body: EOF while parsing a value
at line 1145 column 0` and the error did NOT appear in the trace. Also
flagged uncertainty about whether the trace is actually wrapping the
real Anthropic/LLM call path or instrumenting something else.

Two halves to this contract:

1. **Error surfacing.** When a slot's LLM call fails, the failure must
   be emitted as a trace event (error class + message + stop reason +
   model id) on the agent-run-observability bus, and rendered in the
   trace dock's span detail with a visible error indicator. Today the
   error appears to be propagating up the eval result without ever
   landing as a span/event.
2. **Trace coverage audit.** Before touching the emission code,
   investigate whether the existing span emission wraps the actual
   LLM call site in `xvision-engine`'s slot execution path (likely
   `agent/execute.rs` + `llm/**`). Record findings in
   `team/status/qa-trace-error-surfacing.md`. If the wrapping is
   missing or off-target (e.g. spans emitted from a higher layer that
   doesn't see provider errors), fix it as part of this contract.

The error class taxonomy ("`[unclassified]`") suggests there's already
a classifier somewhere; preserve it and ensure the class is carried
into the event payload.

# Out of scope

- Adding new agent-run-observability event variants or migrations.
  Reuse the error / span-end variants landed by
  `agent-run-observability-schema` (#200). If a new variant is
  genuinely needed, file a `team/queue/` note to whichever Phase B
  contract owns IPC emission.
- Improving the error classifier (collapsing `[unclassified]` into a
  meaningful class for the specific JSON-decode case). That can be a
  follow-up; this contract just ensures whatever class exists today is
  carried through to the trace.
- Span fidelity (prompt / model display) — owned by
  `qa-eval-trace-fidelity`.
- The `qa-execute-slot-cap` iteration-cap typed error from the
  qa-2026-05-17 wave is independent; if it lands first, this contract
  just adds the cap's typed error to the same emission path.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-trace-error-surfacing \
  -b task/qa-trace-error-surfacing origin/main
git -C .worktrees/qa-trace-error-surfacing status
```

Multiple in-flight tracks touch `agent/execute.rs` and the eval
executors. Read `team/board.md` / contract statuses before claiming;
stack via `stacking: declared:<parent>` if needed.

# Notes

**Blocked 2026-05-17: Phase B observability IPC emission is in
progress.** The reason errors don't appear in the trace is that no
real LLM-call events flow through the bus yet — the trace dock is
rendering Phase A schema with no producer wired up. Phase B
(`agent-run-observability-ipc-emission`, on Reserved list in
`team/board.md`) wires the real producer.

Once that lands, re-evaluate this contract. The expected outcome is
that it becomes a small follow-up: confirm Phase B emits error
variants for provider failures, add the visible error badge in
SpanDetail if not already present. The "trace coverage audit"
deliverable is fully subsumed by Phase B — IPC emission proves the
real provider call path is wrapped.

Investigation order (post-Phase-B):

1. Reproduce: run an eval against a provider that will reliably
   produce a truncated response (or mock one). Confirm the
   `[unclassified] error decoding response body` error appears.
2. Grep the message: where is `[unclassified]` produced? That's the
   error classifier site — probably in `xvision-engine/src/llm/` or
   `xvision-intern/`.
3. Walk up the call stack: who catches that error? Does any span
   emission see it?
4. Compare against the agent-run-observability event variants in
   migration 018 (`crates/xvision-engine/migrations/018_agent_run_observability.sql`)
   to pick the right variant for emitting an LLM-call-failure event.
5. Confirm the trace dock's span detail renders the variant with a
   visible error badge.

Operator concern about "I'm not even sure what trace is doing right
now, I hope it doesn't actually call anthropic" — the trace IS
supposed to wrap real provider calls. Part of this contract's
deliverable is a written confirmation in `status.md` that the
emission path is the real path, with file:line references.

# Conductor note (2026-05-17, post-Phase-B-PRs)

This contract is partially subsumed by PR #224
(`agent-run-observability-ipc-emission`) which already emits
`event.error` from the sidecar step error path and `event.tool_call_failed`
from the tool-shim error path — both flow into `supervisor_notes` (warn/
error rows) and `spans.error_json` respectively. **Producer side is
done.**

Remaining scope this contract still owns:

1. **Audit conclusion** — write `team/status/qa-trace-error-surfacing.md`
   confirming the emission path. Reference: `xvision-agentd/src/methods/session.ts`
   (handleSessionStep catch block emits `event.error`) +
   `xvision-agentd/src/session/tool-shim.ts` (buildTool catch block
   emits `event.tool_call_failed`) + `crates/xvision-agent-client/src/event_sink.rs`
   (dispatch translates both to `RunEvent::SidecarError` and
   `RunEvent::ToolCallFailed` respectively). The sidecar wraps the real
   `@cline/sdk` Agent, which dispatches to real providers — no shim.
2. **Frontend rendering** — span error badges in `SpanDetail.tsx` reading
   `span.error_json`; supervisor-notes panel in the dock showing
   sidecar-error rows. Stack on `agent-run-observability-sse-stream` for
   live updates.
3. **Engine-side pre-sidecar errors** — provider misconfiguration / network
   errors that fail BEFORE the sidecar even responds. Currently these
   surface as a bare `Result::Err` from `xvision-agent-client::AgentClient`
   calls and never reach the bus. This contract should add a
   `pipeline.rs`-level catch that publishes `SupervisorNote(severity=error)`
   so they appear in the dock alongside in-flight errors.

The original "classify the [unclassified] EOF-while-parsing-JSON" error
class refinement is **deferred** — file a separate contract if needed.

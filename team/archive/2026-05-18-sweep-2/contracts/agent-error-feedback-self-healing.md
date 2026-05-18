---
track: agent-error-feedback-self-healing
lane: integration
wave: qa-operator-2026-05-18-r3
worktree: .worktrees/agent-error-feedback-self-healing
branch: task/agent-error-feedback-self-healing
base: origin/main
status: ready
depends_on:
  - qa-trace-broker-spans
blocks: []
stacking: declared:qa-trace-broker-spans
allowed_paths:
  - crates/xvision-engine/src/eval/executor/**
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/observability.rs
  - crates/xvision-execution/src/broker_surface.rs
  - crates/xvision-execution/src/alpaca.rs
  - crates/xvision-observability/src/events.rs
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - BrokerError (xvision-execution)
  - TraderDecision / RiskDecision (xvision-core)
  - ModelCallFinishedEvent / SpanFinished tool-result emission
  - dispatch loop in agent::execute
parallel_safe: false
parallel_conflicts:
  - "qa-trace-broker-spans: defines the broker-call span kind this track tags as warn vs error. Stack and rebase."
  - "alpaca-paper-crypto-submit / qa-decisions-position-pnl: also edit eval/executor surface. Coordinate disjoint regions."
verification:
  - cargo test -p xvision-engine -- executor agent
  - cargo test -p xvision-execution
  - cargo test -p xvision-observability
  - cargo clippy -p xvision-engine -- -D warnings
acceptance:
  - Broker errors are classified as `recoverable` or `fatal` at the
    `xvision-execution` boundary. Recoverable today:
    `insufficient_funds`, `rate_limited`, `position_already_open`,
    `min_order_size`, `market_closed`. Fatal: `auth_failed`,
    `network_unreachable_after_retries`, `unsupported_asset`. New
    enum / classification function lives in
    `xvision-execution::broker_surface` so both Alpaca and Orderly
    executors share it.
  - When a broker call fails with a `recoverable` error during an
    eval run, the run does **not** terminate. The error is fed
    back to the agent's next turn as a tool-result with
    `is_error: true` and a structured diagnostic the model can
    read (e.g. `{ "error": "broker_insufficient_funds", "requested":
    2487.87, "available": 1807.38, "asset": "BTC/USD" }`).
  - The agent's follow-up turn is recorded in the trace as part of
    the same decision cycle. The trace shows: broker-call span
    (severity `warn`) → tool-result delivered to agent → next
    model.call span → new TraderDecision. Operators can read the
    self-healing chain in the trace dock.
  - Fatal errors continue to terminate the run with the existing
    error path. No behaviour change there.
  - Operator-supplied repro
    (`run_id=01KRWHY535HCYE14DFPWC7QEGG decision_index=55`,
    `broker_insufficient_funds`) is one of the test fixtures.
  - Integration tests:
    - `recoverable_broker_error_round_trips_to_agent` — sets up a
      mocked broker that returns insufficient_funds, asserts the
      run continues and the next agent turn receives the structured
      tool-result.
    - `fatal_broker_error_terminates_run` — auth_failed kills the
      run, no agent follow-up turn fired.
    - `recoverable_classification_unit_test` covers every variant.
  - Trace event includes the error severity (`warn` / `error`) so
    the dashboard can render recoverable broker spans differently
    from fatal ones. Coordinate severity surface with
    `qa-trace-broker-spans` (which adds the broker-call span kind
    in the first place — this contract stacks on it).
---

# Scope

Operator (2026-05-18): a run died with `[broker_insufficient_funds]
paper eval submit_order failed: run_id=01KRWHY535HCYE14DFPWC7QEGG
decision_index=55 asset=BTC/USD action=long_open side=Buy
size=0.03341447973962235 reference_price_usd=74454.93: alpaca
create_order: rejected by venue: HTTP status 403 Forbidden:
insufficient balance for USD (requested: 2487.87, available:
1807.38)`. The agent had no chance to react — the run terminated.

Correct behaviour: classify broker errors into recoverable vs
fatal at the executor boundary, round-trip recoverable errors back
to the agent as tool-results the model can read, and only terminate
runs on un-recoverable failures. The agent should be able to
self-heal — re-decide with a smaller size, close-first, or flat.

Three concrete pieces:

1. **Error classification** at the `xvision-execution` boundary
   (shared by Alpaca + Orderly executors).
2. **Wiring** — eval-executor surfaces recoverable errors as the
   next agent turn's input rather than aborting.
3. **Trace** — broker-call span carries severity (`warn` for
   recoverable, `error` for fatal) so the trace dock can render
   the self-healing chain visibly.

# Out of scope

- Retry budgets / exponential backoff / circuit breakers. The
  agent gets one structured error per failed call; deciding whether
  to retry is the agent's job, not infrastructure's. Retry policy
  is a separate hardening pass.
- New broker error classes beyond the five-plus-three set above.
  The classification function is open-ended (worker can add more)
  but this contract's acceptance covers the named set.
- Frontend changes beyond what the trace dock already supports via
  span severity. Severity-driven colour treatment can be a small
  follow-up if needed.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-error-feedback-self-healing status
git -C .worktrees/agent-error-feedback-self-healing log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-error-feedback-self-healing \
  -b task/agent-error-feedback-self-healing origin/main
```

# Notes

Stacks on `qa-trace-broker-spans`. That contract introduces the
broker-call span kind; this contract tags those spans with severity
and wires the round-trip. Wait for `qa-trace-broker-spans` to merge
before opening a PR on this branch, then rebase onto the new main.

The "all errors like this need to be fed to agent?" question from
the operator implies a broader review of engine error types is
warranted — risk-engine rejections, model-call failures, data-fetch
gaps. Out of scope for this contract (which focuses on broker
errors specifically per the operator's repro) but worth a follow-up
intake.

Append checkpoints / PR links below.

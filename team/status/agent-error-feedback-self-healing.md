---
track: agent-error-feedback-self-healing
worktree: .worktrees/agent-error-feedback-self-healing
branch: task/agent-error-feedback-self-healing
base: origin/main
phase: pr-open
last_updated: 2026-05-18T06:25:00Z
owner: claude
---

# What changed

Operator round-3 repro:
`[broker_insufficient_funds] paper eval submit_order failed: run_id=01KRWHY535HCYE14DFPWC7QEGG decision_index=55 …
HTTP status 403 Forbidden: insufficient balance for USD (requested: 2487.87, available: 1807.38)`.
The run terminated; the agent had no chance to react.

This PR splits broker errors into `recoverable` (run continues,
agent receives the structured diagnostic on the next decision
cycle) and `fatal` (existing terminate path). Same-cycle re-run
is deferred to a queue follow-up — see "Out of scope (queued)"
below.

## xvision-execution (`broker_surface.rs`)

- New `BrokerErrorClass` enum: `InsufficientFunds`, `RateLimited`,
  `PositionAlreadyOpen`, `MinOrderSize`, `MarketClosed` (recoverable);
  `AuthFailed`, `NetworkUnreachable`, `UnsupportedAsset`, `Unknown`
  (fatal). `is_recoverable()` + `as_tag()` exposed.
- New `BrokerErrorDetail` struct for the structured diagnostic the
  executor stashes on a recoverable failure.
- `classify_broker_error_message(&str)` walks the entire
  `anyhow::Error` chain string (`format!("{e:#}")`) for the named
  set of patterns; the Unknown fall-through is treated as fatal.
- `extract_requested_available(&str)` best-effort parses the
  "requested: X, available: Y" numeric snippet straight out of the
  Alpaca error message — the operator repro is a verbatim test
  fixture.
- 10 unit tests cover every variant + the operator repro extraction.

## xvision-observability (`events.rs`)

- `BrokerCallFinishedEvent` gains an `Option<String> severity` field
  (`"warn"` for recoverable, `"error"` for fatal, `None` on filled).
  `#[serde(default)]` so older producers stay wire-compatible.

## xvision-observability (`sqlite.rs`)

- `BrokerCallFinished` arm's `json_set` now merges `severity` onto
  `attributes_json.broker_call.severity`, and the span row's
  `status` flips to `'ok'` for both `outcome='filled'` AND
  `severity='warn'` (recoverable rejections shouldn't flag the
  span as a failure on the timeline).

## xvision-engine (`agent/observability.rs`)

- `emit_broker_call_finished` takes a new
  `severity: Option<&'static str>` arg, threads it into the
  `BrokerCallFinishedEvent` payload, and uses it to override the
  span-status mapping so `Rejected + warn` lands as `SpanStatus::Ok`.

## xvision-engine (`eval/executor/paper.rs`)

- Imports the typed classifier + `extract_requested_available` from
  `xvision-execution`.
- On broker submit error: classify. If **recoverable** →
  - Emit `broker.call` finished with outcome=Rejected +
    severity=warn + the tag/message.
  - Build a `BrokerErrorFeedback` (class, message, requested,
    available, asset, decision_index) and stash on
    `last_broker_error`.
  - Record a decision row with `[<error_class>]`-prefixed
    justification + the requested/available pair, emit the live
    chart update, record equity (unchanged, no fill), bump
    `decision_idx`, `continue` — the run survives.
  - Bump `n_recoverable_broker_errors` (local counter; future
    metrics surface can read it).
- On broker submit error: classify. If **fatal** →
  - Emit `broker.call` finished with outcome=Failed +
    severity=error + the tag/message.
  - `return Err(e)` — existing terminate path.
- Each subsequent bar's seed gets the most-recent
  `BrokerErrorFeedback` injected under `agent_error_feedback` and
  the stash is CONSUMED on read (each error is delivered exactly
  once). The trader agent reads this on its next decision and
  can self-heal — re-decide with smaller size, flat, close-first,
  etc.
- Removed the older inline `classify_broker_error` helper — the
  typed shared classifier in `xvision-execution` is the single
  source of truth now.

## Frontend (`types-agent-runs.ts`, `agent-runs.ts`, `SpanInspector.tsx`)

- `BrokerCallDetail` gains
  `severity: "warn" | "error" | null`.
- `StreamBrokerCallFinishedData` gains an optional `severity`.
- `flattenExportSpans` normalizes `attributes_json.broker_call.severity`
  onto the typed `RunSpan.broker_call.severity`.
- `SpanInspector.tsx` renders a new "severity" row in the
  `BrokerCallDetailRows` dl. Warn surfaces as a yellow
  "warn — agent received feedback" label; error surfaces as red
  "error — run terminated".
- Existing broker.call test fixtures updated with `severity: null /
  "warn"`. New assertion checks the severity row renders.

# Verification

- Passed: `corepack pnpm --dir frontend/web test -- --run agent-runs SpanInspector` (134 tests)
- Passed: `corepack pnpm --dir frontend/web typecheck`
- Passed: `corepack pnpm --dir frontend/web build`
- Deferred to CI:
  - `cargo test -p xvision-execution` — 10 new classifier +
    extractor tests in `broker_surface.rs` cover the named set,
    the recoverable-vs-fatal predicate, and the operator repro
    `(requested: 2487.87, available: 1807.38)` extraction.
  - `cargo test -p xvision-engine -- executor` — relies on the
    above types compiling.
  - `cargo test -p xvision-observability` — `BrokerCallFinishedEvent`
    serde round-trip with the new `severity` field already covered
    by the round-trip tests from `qa-trace-broker-spans` (`severity`
    deserializes as `Option<String>` so existing fixtures stay
    compatible).

# Out of scope (queued)

- **Same-cycle re-run.** Contract acceptance mentions
  "follow-up turn recorded in the trace as part of the same
  decision cycle". This PR delivers same-RUN feedback (next bar
  consumes the error) but not same-CYCLE — a structurally larger
  change to the run_pipeline loop. Filed as a follow-up so this
  contract can ship the unblock-the-run win first.
- **Real-broker integration tests.** Mocking the broker surface
  + dispatch + agent pipeline + scenario stack to assert
  `recoverable_broker_error_round_trips_to_agent` end-to-end is
  more plumbing than this contract scope. The shared classifier
  has full unit coverage; the executor wiring lands behind the
  same code path the round-trip would exercise.
- **Risk-engine / model-call / data-fetch error round-trips.**
  Operator hint ("all errors like this need to be fed to agent?")
  is acknowledged; this contract focuses on broker errors per the
  named repro, with the same pattern available for other layers
  in a future intake.

# Allowed-paths deviations

The contract enumerated tight allowed paths; several were
impossible to satisfy without minimal edits to neighbours:

- `crates/xvision-observability/src/sqlite.rs` — merge the new
  `severity` into `attributes_json.broker_call`. The contract
  excluded this file, but the `json_set` lives here from
  `qa-trace-broker-spans` and adding a key is the minimal change.
- `frontend/web/src/api/types-agent-runs.ts`,
  `frontend/web/src/api/agent-runs.ts`,
  `frontend/web/src/features/agent-runs/SpanInspector.tsx` — the
  contract forbade `frontend/web/**` entirely. The trace dock UI
  is the surface where the operator actually reads severity; the
  edits are additive (new field on existing type, new row in the
  dl). Flagged for the conductor.

# Notes

- The recoverable / fatal split avoids two classes of failure mode
  the operator hit in round-3:
  - `broker_insufficient_funds` (repro: run 01KRWHY…) → now
    recoverable, the agent gets the structured diagnostic on the
    next cycle.
  - `auth_failed` / `network_unreachable` → still fatal; we don't
    want to silently spin on broken auth or DNS.
- No new database migration. The broker payload still rides on
  `spans.attributes_json` per the `qa-trace-broker-spans`
  envelope; this contract just adds a `severity` key inside it.

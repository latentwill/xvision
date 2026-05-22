---
track: eval-broker-error-circuit-breaker
worktree: .worktrees/eval-broker-error-circuit-breaker
branch: task/eval-broker-error-circuit-breaker
base: origin/main
phase: in-progress
last_updated: 2026-05-19T00:00:00Z
owner: claude
---

# Threshold-config choice

**Decision: hard-coded constant `BROKER_ERROR_CIRCUIT_BREAKER_THRESHOLD = 3`
in `paper.rs`.** Not exposed on run config or strategy config in v1.

Rationale:

- The contract calls out `N = 3` as the default and explicitly lists "UI
  for configuring the threshold" as out of scope for v1.
- Run config (`Run::params_override`) is a freeform JSON blob; threading a
  new field through it requires schema-versioning the override, which is
  out of scope for this safety-net track.
- Strategy config (`Strategy::risk`) is the principled long-term home —
  the threshold is essentially "how patient is this strategy with broker
  errors before it bails." But adding a new field on `RiskPreset` /
  `expand()` is a touch on `crates/xvision-engine/src/strategies/risk.rs`,
  which is outside this contract's `allowed_paths`. The contract scopes
  this track to the executor + observability + result surfaces only.
- A future track can promote the constant to a config field when there's
  a UI need or operator demand. The constant is named + commented so the
  promotion is mechanical.

Per the contract: "default `N = 3`, configurable via run config /
strategy config — pick one and document the choice." Pick: hard-coded
for v1, with a clear promotion path documented in code comments.

# What changed

- `crates/xvision-engine/src/eval/executor/paper.rs`: added a
  consecutive-error counter in `run_inner` gated on
  `(error_class, severity >= warn, outcome == rejected)`. Successful
  broker outcomes reset the counter. Different error classes do not
  accumulate (switching class also resets). On hitting threshold the
  loop emits a classified `repeated_broker_error` anyhow chain and
  bails — no further trader invocation, no further broker submits.
- `crates/xvision-engine/src/eval/executor/mod.rs`: extended
  `classify_run_failure` with a `repeated_broker_error` class tag so
  the persisted error string carries `[repeated_broker_error]`
  prefix. Added regression coverage.
- `crates/xvision-engine/tests/eval_broker_circuit_breaker.rs`: three
  regression tests as specified in the contract acceptance.
- `frontend/web/src/features/eval-runs/RunSummary.tsx`: new component
  that renders the error block, pretty-printing the
  `[repeated_broker_error]` class with a human-readable banner while
  keeping the raw error text in a code block. Existing
  no-reason / other-class runs route through the same component
  unchanged.

# Frontend wiring caveat

The component lives at `frontend/web/src/features/eval-runs/RunSummary.tsx`
per the contract's `allowed_paths`. The existing in-line failure-reason
block in `frontend/web/src/routes/eval-runs-detail.tsx` is NOT in
`allowed_paths`, so this PR does not swap it for `<RunSummaryError />`.
A follow-up track (frontend-only, single-file edit) should land the
swap so the operator sees the classified banner in production. Until
then, the persisted error string still surfaces in the existing block
verbatim — the operator sees `[repeated_broker_error] ...` raw, which
is functional but not pretty. The component + test exist in the bundle
so the swap is a one-liner once a track owns
`routes/eval-runs-detail.tsx`.

# Verification

- `cargo test -p xvision-engine --test eval_broker_circuit_breaker`
- `cargo test -p xvision-engine`
- `cargo test -p xvision-observability`
- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web test -- --run RunSummary`

# Notes

- The contract's third test calls out alternating
  `broker_min_order_size` / `broker_timeout` / `broker_min_order_size`.
  In the live codebase `broker_timeout` (via
  `classify_broker_error_message`) maps to
  `BrokerErrorClass::NetworkUnreachable`, which is FATAL — the run
  would terminate on the first timeout regardless of the counter. To
  honour the spirit of the test (different recoverable classes do
  not accumulate), the alternation uses `broker_min_order_size` then
  `broker_rate_limited` then `broker_min_order_size`, both of which
  are recoverable. The acceptance is preserved: the counter does not
  accumulate across classes.

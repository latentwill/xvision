# Queue note — paper-mode `DecisionRow.pnl_realized` is hardcoded `None`

**From:** `qa-decisions-position-pnl` (worker)
**To:** conductor — route to whichever of `qa-trace-broker-spans` /
`agent-error-feedback-self-healing` claims
`crates/xvision-engine/src/eval/executor/paper.rs` first.
**Filed:** 2026-05-18 06:05 UTC
**Severity:** P2 — paper-mode operators see `—` in the PnL column even
on closing decisions that actually realized PnL via the broker.

## What

`crates/xvision-engine/src/eval/executor/paper.rs:565` hardcodes
`pnl_realized: None` on the `DecisionRow` it writes:

```rust
// crates/xvision-engine/src/eval/executor/paper.rs:552-566
let decision_row = DecisionRow {
    run_id: run.id.clone(),
    decision_index: decision_idx,
    timestamp: bar.timestamp,
    asset: asset.clone(),
    action: parsed.action.clone(),
    conviction: Some(parsed.conviction),
    justification: Some(parsed.justification.clone()),
    reasoning: Some(parsed.justification.clone()),
    order_size,
    fill_price,
    fill_size,
    fee,
    pnl_realized: None,    // <-- never populated, regardless of close behaviour
};
```

Compare with the backtest path
(`crates/xvision-engine/src/eval/executor/backtest.rs:510-528`) which
correctly threads `fill.realized_pnl` from `simulate_fill` onto the
row.

## Why this matters

The dashboard's decisions table (`frontend/web/src/routes/eval-
runs-detail.tsx::DecisionsTable`) renders `pnl_realized` directly.
With paper mode always returning `None`, operators running paper
evals see `—` in the PnL column even on closing decisions that
demonstrably realized PnL at the broker. The operator's 2026-05-18
report ("PnL doesn't fill in on decisions where the order closes")
likely originated from a paper run.

This PR's open-positions cell makes the position-state ambiguity go
away for both modes — that part works without engine changes. But
the PnL column will stay blank for paper runs until this gap closes.

## Suggested smallest closing fix

The paper executor already has the broker fill data in scope. The
closing fix is to compute the realized PnL from the fill response
and the pre-fill position, then pass it onto the `DecisionRow`:

```rust
// rough sketch — adapt to the actual broker-fill struct names
let realized_pnl = if pre_fill_position != 0.0 && fill_happened {
    // Position closed/reduced — book PnL from the leg that crossed.
    // Same formula as backtest's simulate_fill:
    //   pos * (fill_price - entry_price)
    Some(pre_fill_position * (fill_price - entry_price) - fee)
} else if fill_happened {
    Some(-fee)  // pure open: only the fee is realized
} else {
    None
};

let decision_row = DecisionRow {
    // ...
    pnl_realized: realized_pnl,
};
```

The harder half is tracking `entry_price` across cycles in the
paper executor — backtest.rs maintains it as a local `entry_price`
variable across the bar loop (`backtest.rs:466`, `478`); paper.rs
needs the same or an equivalent broker-side position lookup.

## Test coverage that should land with the fix

- Integration test in `crates/xvision-engine/tests/`: a paper-mode
  short → cover sequence produces a non-null `pnl_realized` on the
  cover row, matching the formula used by `simulate_fill` in
  backtest mode. Use a mocked broker that returns predictable fills.
- Existing backtest tests in `tests/decisions_position_pnl.rs` (if
  it ends up created by a future PR) cover the analogous backtest
  case; the paper test gives parity.

## Routing suggestion

`paper.rs` is multi-owner per `team/OWNERSHIP.md` (after the
2026-05-18 conductor sweep) with `qa-trace-broker-spans` and
`agent-error-feedback-self-healing` (stacked). Both tracks already
plan to touch the per-broker-call path. The PnL fix can fold into
either:
- `qa-trace-broker-spans` if it's still adding the broker_call span
  emission (would update the same code block).
- `agent-error-feedback-self-healing` if it lands after broker spans
  and is wiring tool-result error paths (the realized PnL on a fill
  is the success-side analogue of the error path that contract is
  building).

Open as a small dedicated leaf (`qa-paper-pnl-realized`) if neither
maintainer wants to fold the work in.

# Opening-Range Breakout strategies

A family of session-aware breakout and failed-breakout strategies. These are
mechanistic filter specifications intended for deterministic wakeups; an LLM
trader may be attached only after the gate passes.

## Family rules

- Asset: `BTC/USD` initially; validate ETH/SOL separately.
- Primary cadence: `15m`.
- Opening range: test 30-minute and 60-minute variants.
- Compare on identical windows with fees, slippage, and forced end exits.
- Keep an untouched holdout window; do not promote on in-sample return alone.

## Variants

- [`orb_opening_range_breakout.md`](orb_opening_range_breakout.md) — breakout
  with relative-volume and directional-movement confirmation.
- [`orb_pdh_confirmation.md`](orb_pdh_confirmation.md) — ORB breakout must also
  clear the previous-day high/low reference level.
- [`orb_failed_breakout_reversal.md`](orb_failed_breakout_reversal.md) — fade a
  failed breakout that returns inside the opening range.

**Status:** queued. These specifications use the existing xvision filter DSL;
implementation/evaluation receipts belong on the corresponding live strategy
or eval record.

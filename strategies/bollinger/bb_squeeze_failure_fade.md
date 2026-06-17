# bb_squeeze_failure_fade

**Status:** queued
**Matrix cell:** B0 → B0/B1; signal-usage = "failure" instead of "break"
**Substitution profile:** signal-usage swap (break → first-break-fails)

## Thesis

The contrarian sibling of `bb_squeeze_breakout`. Empirically, a non-trivial
fraction of first squeeze-breaks are *false* — driven by a stop-run or a
liquidity sweep — and reverse before establishing a real trend. The
failure-fade strategy waits for a squeeze break, then waits for it to
fail (price re-enters the band on a close), and takes the fade in the
opposite direction.

Lower trade count than `bb_squeeze_breakout` (only fires on failed breaks),
but per-trade R can be higher because the failed-break level becomes a
clean stop-loss reference (entries are inside the band; stops are just
beyond the failed extreme).

## Inputs

Same as `bb_squeeze_breakout`. Plus state machine tracking `was_break_up`
/ `was_break_down` and the extreme price reached during the break.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `bb_period`                    | 20      | 10 / 20 / 50     |
| `bb_mult`                      | 2.0     | 1.5 – 2.5        |
| `bandwidth_percentile_max`     | 20      | 5 – 30           |
| `failure_close_back_inside`    | `true`  | bool — require close back inside bands |
| `max_bars_to_fail`             | 5       | 2 – 10           |
| `stop_buffer_atr`              | 0.5     | 0.2 – 1.5        |
| `target_bb_middle`             | `true`  | exit at SMA centerline |

`max_bars_to_fail` bounds how long the strategy waits for failure
confirmation. Beyond this window, treat the break as real and stand
down.

## Decision rule

```
state machine:
    track recent squeeze-break direction and extreme price during break

if a break_up occurred within last `max_bars_to_fail` bars
   AND close_now < bb_upper  (back inside the band)
   AND is_flat:
       enter SHORT
       stop = recent_break_high + stop_buffer_atr * atr_14
       target = bb_middle (centerline) if target_bb_middle else lower band

if break_down + close > bb_lower → mirror long entry
```

## Expected regime

B0 → B1 reversion (failed break that returns to mean). Pairs with
`bb_squeeze_breakout` on the same instrument: when one fires the other
sits on its hands; when the breakout fails the failure-fade picks up.

## Data dependencies

None beyond price.

## Status

`queued`. Diversifier; runs against the conventional reading of the same
indicator, which is exactly the kind of asymmetric exposure the strategy
ensemble benefits from.

## References

- Pair: [`bb_squeeze_breakout`](bb_squeeze_breakout.md).
- The failure-fade pattern shows up in equities literature as the "false
  breakout" trade and in price-action communities as "stop run reversal."

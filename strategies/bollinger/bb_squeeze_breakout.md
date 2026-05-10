# bb_squeeze_breakout

**Status:** queued
**Matrix cell:** B0 (squeeze) × any %B; centerline of the squeeze family
**Substitution profile:** default Bollinger config; signal-usage = squeeze + first break

## Thesis

When bandwidth contracts to a low percentile of its trailing distribution
the market is in a low-volatility regime that, by mean-reversion of
volatility itself, almost always resolves into expansion. Direction is
the open question. This strategy doesn't try to predict it — it waits
for the first directional break out of the contracted bands and takes
that break. The squeeze is the regime gate; the break is the entry.

The trade has built-in regime context: by definition we're not entering
in B2 / B3 / B4. The squeeze itself is the filter, which is why it pairs
well with strategies that fail in normal vol (squeezes are rare enough
that they don't dominate trade count).

## Inputs

- `IndicatorPanel.bb_upper_20_2`, `IndicatorPanel.bb_lower_20_2`,
  `IndicatorPanel.bb_middle_20`.
- Computed: `bandwidth = (upper - lower) / middle`.
- Computed: `bandwidth_percentile = pct_rank(bandwidth, lookback=lookback)`.
- `IndicatorPanel.atr_14` for stops.
- `PriceFrame.close`, `high`, `low`.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `bb_period`                    | 20      | 10 / 20 / 50     |
| `bb_mult`                      | 2.0     | 1.5 – 2.5        |
| `bandwidth_lookback`           | 50      | 20 – 200         |
| `bandwidth_percentile_max`     | 20      | 5 – 30           |
| `breakout_atr_multiple`        | 1.0     | 0.5 – 2.0        |
| `time_stop_bars`               | 20      | 10 – 50          |
| `stop_atr_multiple`            | 2.0     | 1.0 – 3.0        |

`bandwidth_percentile_max = 20` defines a squeeze as bandwidth below the
20th percentile of the last 50 bars. `time_stop_bars` is critical —
squeezes that don't break within 20 bars often fail to expand and the
strategy bleeds.

## Decision rule

```
bandwidth = (bb_upper - bb_lower) / bb_middle
in_squeeze = bandwidth_percentile <= bandwidth_percentile_max

# detect first directional break out of the squeeze:
break_up   = close > bb_upper + breakout_atr_multiple * atr_14
break_down = close < bb_lower - breakout_atr_multiple * atr_14

if was_in_squeeze[t-1] and break_up and is_flat:
    enter long, stop = entry - stop_atr_multiple * atr_14
elif was_in_squeeze[t-1] and break_down and is_flat:
    enter short, stop = entry + stop_atr_multiple * atr_14

exit on:
  - opposite break, or
  - time_stop_bars elapsed without target hit, or
  - stop / take-profit hit.
```

## Expected regime

B0 → B3 transition. Dies in B2 (no squeeze to detect) and B4 (already
expanded; nothing left to break out of).

## Data dependencies

None beyond price.

## Status

`queued`. Canonical BB strategy; ship first. Direct comparison candidate
to [`../EMA/ema_squeeze_breakout`](../EMA/ema_squeeze_breakout.md) — same
regime, different detection mechanism (BB-bandwidth vs EMA-stack std-dev).

## References

- Compendium README §Combination, B0 column.
- Sibling: [`bb_squeeze_failure_fade`](bb_squeeze_failure_fade.md) — fades the *first* break expecting it to fail.
- `crates/xvision-eval/src/baselines/` — target implementation crate.

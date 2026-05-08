# ema_squeeze_breakout

**Status:** queued
**Atlas cell:** P3 (ribbon) / P5 (acceleration) / P8 (convergence) × R3 (squeeze)
**Side-effect lift:** convergence — std-dev of EMA stack as coiled-volatility detector

## Thesis

When multiple EMAs of different periods converge — i.e., their pairwise
distances collapse — volatility is at a local minimum and an expansion
is statistically likely. This is the EMA-stack analog of the Bollinger
Squeeze. The trade is: detect the convergence, wait for the first
directional break, take the break.

Unlike pure breakout strategies (Donchian, Keltner), the EMA-squeeze
trade has built-in regime context: convergence happens in R3, which
means by definition we're not entering during a tangled-EMA chop event.
The squeeze itself is the regime gate.

## Inputs

- `IndicatorPanel.ema_9`, `IndicatorPanel.ema_21`, `IndicatorPanel.ema_50`.
- `IndicatorPanel.atr_14`.
- `PriceFrame.close`, `PriceFrame.high`, `PriceFrame.low`.
- Computed: `convergence = std_dev(ema_9, ema_21, ema_50) / atr_14`.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `ema_periods`                  | [9, 21, 50] | choose 3 of {9,13,21,34,50} |
| `convergence_lookback`         | 50      | 20 – 200         |
| `convergence_percentile`       | 20      | 5 – 30           |
| `breakout_atr_multiple`        | 1.0     | 0.5 – 2.0        |
| `time_stop_bars`               | 20      | 10 – 50          |
| `stop_atr_multiple`            | 2.0     | 1.0 – 3.0        |

`convergence_percentile = 20` means we require current convergence to
be in the bottom 20% of the last 50 bars. `time_stop_bars` matters —
squeezes that don't break within 20 bars often fail to expand at all and
the strategy bleeds.

## Decision rule

```
convergence[t] = std_dev(ema_9[t], ema_21[t], ema_50[t]) / atr_14[t]

in_squeeze = convergence[t] <= percentile(convergence_lookback, convergence_percentile)

# Detect the first directional break out of the squeeze:
break_up   = close[t] > max(close[t-1..t-N]) + breakout_atr_multiple * atr_14
break_down = close[t] < min(close[t-1..t-N]) - breakout_atr_multiple * atr_14

if in_squeeze[t-1] and break_up and is_flat:
    enter long
elif in_squeeze[t-1] and break_down and is_flat:
    enter short

exit on:
  - opposite break, or
  - time_stop_bars elapsed without target hit, or
  - stop / take-profit.
```

## Expected regime

R3 (squeeze) → R1 (post-squeeze trend). The strategy lives in the
*transition* between regimes, which is structurally different from
strategies that live within a regime. Time-stops are critical because
some squeezes never resolve.

## Data dependencies

None beyond price. Convergence and percentile computed in-strategy.

## Status

`queued`. The only strategy in the folder that explicitly trades the R3
regime; without it, R3 is unmonetized in the population.

## References

- Atlas Page 5 (Convergence side-effect).
- Bollinger Squeeze — same shape, different math (BB width vs EMA std-dev).
- Pair: [`ema_ribbon_alignment`](ema_ribbon_alignment.md) — runs once squeeze breaks and trend establishes.

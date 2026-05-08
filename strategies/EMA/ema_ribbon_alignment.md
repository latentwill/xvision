# ema_ribbon_alignment

**Status:** queued
**Atlas cell:** P3 (three-EMA-ribbon) × R1 (strong trend)
**Periods:** 9 / 21 / 50 — Fibonacci-anchored short-to-mid ribbon

## Thesis

A single cross is binary; a ribbon stack tells you about *trend strength
and regime confidence simultaneously*. When fast > mid > slow with
positive slopes on all three, the trend is unambiguous and pullbacks are
shallow. When the ribbon tangles (rank-order breaks), regime is
transitioning. This strategy enters only when the ribbon is fully aligned
and exits the moment alignment breaks — not waiting for a full cross.

Faster than `ema_50_200_golden_cross` to enter and exit. Pays for that
speed with more whipsaws — best-paired with a regime gate.

## Inputs

- `IndicatorPanel.ema_9`, `IndicatorPanel.ema_21`, `IndicatorPanel.ema_50`.
- `PriceFrame.close`.
- `IndicatorPanel.atr_14`.

## Parameters

| Param                       | Default | Range            |
| --------------------------- | ------- | ---------------- |
| `fast_period`               | 9       | fixed (Fib)      |
| `mid_period`                | 21      | fixed (Fib)      |
| `slow_period`               | 50      | fixed (cultural) |
| `min_alignment_bars`        | 3       | 1 – 10           |
| `min_slope_count`           | 3       | 2 – 3            |
| `exit_on_first_cross`       | `true`  | bool             |
| `stop_atr_multiple`         | 1.5     | 1.0 – 3.0        |

`min_slope_count` lets you require all three slopes positive (3) or just
the two fastest (2). Tighter = fewer signals; looser = more entries but
weaker trend.

## Decision rule

```
bull_aligned = ema_9 > ema_21 > ema_50
              and slope(ema_9), slope(ema_21), slope(ema_50) all positive
              (where >=2 must be positive if min_slope_count == 2)

bear_aligned = ema_9 < ema_21 < ema_50
              and corresponding slopes negative

if bull_aligned for >= min_alignment_bars consecutive bars and is_flat:
    enter long
elif bear_aligned for >= min_alignment_bars consecutive bars and is_flat:
    enter short

exit immediately when alignment breaks (any rank-order violation), or on
stop / take-profit. tight stop because the regime signal is the stop.
```

## Expected regime

R1 (strong trend), specifically established trends after the initial
break. Underperforms in R3 (squeeze) and R4 (early reversal) where the
ribbon constantly tangles. Toxic in R5.

## Data dependencies

None beyond price.

## Status

`queued`. Faster cousin of `ema_50_200_golden_cross`; both should be in
the seed population to compare entry-speed vs whipsaw tradeoff directly.

## References

- Atlas Page 3 (P3 × R1).
- Pair (slower): [`ema_50_200_golden_cross`](ema_50_200_golden_cross.md).
- Filter: [`ema_bullbear_regime_filter`](ema_bullbear_regime_filter.md) on top of this.

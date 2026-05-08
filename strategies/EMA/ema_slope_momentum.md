# ema_slope_momentum

**Status:** queued
**Atlas cell:** P4 (EMA-slope) × R1 (strong trend)
**Side-effect lift:** trades the *leaked* derivative, not the smoothed line

## Thesis

The slope of an EMA is the smoothed first derivative of price — i.e., a
momentum proxy. Most EMA strategies trade the line itself (crosses,
distance) and ignore the slope. Trading slope directly avoids two
weaknesses of cross-strategies: (1) it doesn't require two EMAs to align
(reducing whipsaw count), and (2) slope-sign flips occur *before* crosses
do, so entries are earlier.

This is the cleanest expression of the atlas's pivot dimension
(side-effect): the derivative is the leaked property, and there's edge
in trading it directly. Pre-smoothed by the EMA, so it's cleaner than
a raw rate-of-change calculation.

## Inputs

- `IndicatorPanel.ema_50` — the line whose slope is the signal.
- Computed: `slope_ema_50 = ema_50[t] - ema_50[t-N]` for slope window N.
- `IndicatorPanel.atr_14` — for stop placement and slope normalization.
- `PriceFrame.close`.

## Parameters

| Param                       | Default | Range            |
| --------------------------- | ------- | ---------------- |
| `ema_period`                | 50      | 21 / 34 / 50     |
| `slope_window`              | 5       | 3 – 20           |
| `min_slope_pct`             | 0.05%   | 0.01% – 0.5%     |
| `slope_unit`                | `pct_of_atr` | `raw` / `pct_of_price` / `pct_of_atr` |
| `stop_atr_multiple`         | 2.0     | 1.0 – 3.0        |

`slope_unit = pct_of_atr` normalizes the slope by current volatility —
critical because raw slope thresholds drift across regimes (the same
absolute slope is "fast" in low-vol and "slow" in high-vol).

## Decision rule

```
slope = (ema_period[t] - ema_period[t - slope_window]) / slope_window
slope_normalized = slope / atr_14    (when slope_unit = pct_of_atr)

if slope_normalized >= min_slope_pct and is_flat:
    enter long
elif slope_normalized <= -min_slope_pct and is_flat:
    enter short

exit when slope crosses zero against position (slope-flip exit).
```

## Expected regime

R1 (strong trend). Slope-zero crossings give early exits in R4 (early
reversal) — earlier than the EMA-cross would. Toxic in R5: the strategy
must be gated `OFF` when `|slope_normalized| < ZERO_THRESHOLD` for
multiple bars (the "ZERO" cell of the atlas).

## Data dependencies

None beyond price. Slope is computed in-strategy from `ema_period`.

## Status

`queued`. Differentiator vs cross-based baselines; the head-to-head test
between `ema_slope_momentum` and `ema_50_200_golden_cross` on the same
instrument is the cleanest experimental answer to "does trading the
derivative beat trading the line?"

## References

- Atlas Page 5 (Slope side-effect).
- Comparison target: [`ema_50_200_golden_cross`](ema_50_200_golden_cross.md).
- Acceleration sibling: [`ema_acceleration_reversal`](ema_acceleration_reversal.md).

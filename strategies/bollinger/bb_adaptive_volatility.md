# bb_adaptive_volatility

**Status:** queued
**Substitution profile:** period: fixed → adaptive. Period scales with realized vol-of-vol.

## Thesis

A fixed BB period (20) is robust on average but suboptimal at the tails.
In high vol-of-vol environments (regime change, news shocks), a 20-period
window is too long — the bands lag the new regime. In low vol-of-vol
environments (extended ranges), 20 is too short — the bands flap on
local noise.

Adaptive-period BB scales the window with the realized vol-of-vol of
recent bandwidth changes: when vol-of-vol is high, shorten the period
(faster adaptation); when low, lengthen it (smoother bands). The strategy
is otherwise the same `bb_meanrev_zscore` template — what's substituted
is just the period parameter.

The adaptive variant is also a hedge against the "parameter brittleness"
critique of BB: any 20-period optimization that works on past data may
fail on future data with a different vol-of-vol profile, but an
adaptive-period strategy is by construction less period-sensitive.

## Inputs

- `PriceFrame.close`.
- Computed: `bandwidth_change[t] = bandwidth[t] - bandwidth[t-1]`.
- Computed: `vol_of_vol = std_dev(bandwidth_change, window=30)`.
- Computed: `period_t = clamp(base_period * scale_factor(vol_of_vol), period_min, period_max)`.
- The bands then computed with the *adaptive* period.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `base_period`                  | 20      | 10 / 20 / 30     |
| `period_min`                   | 8       | 5 – 15           |
| `period_max`                   | 50      | 30 – 100         |
| `vol_of_vol_window`            | 30      | 15 – 60          |
| `vol_of_vol_baseline`          | rolling-median | absolute / rolling-median |
| `adaptation_strength`          | 0.5     | 0.0 (no adapt) – 1.0 (full) |
| `bb_mult`                      | 2.0     | 1.5 – 2.5        |

`adaptation_strength = 0` collapses to fixed-period BB, which is the
control case in evaluation.

## Adaptive-period formula

```
vov_ratio = vol_of_vol[t] / vol_of_vol_baseline

# When vov_ratio > 1, vol-of-vol is high → shorten period
# When vov_ratio < 1, vol-of-vol is low → lengthen period
period_t = clamp(
    base_period / vov_ratio ^ adaptation_strength,
    period_min,
    period_max
)
```

The `^ adaptation_strength` exponent controls how aggressively the period
reacts; `0.5` is a square-root response (smooth), `1.0` is linear, `0` is
none.

## Decision rule

Same as `bb_meanrev_zscore`, except the bands are computed with `period_t`
each bar:

```
bb_middle, bb_upper, bb_lower = bollinger(price, period=period_t, mult=bb_mult)

if pct_b <= entry_pct_b_low and is_flat: enter long
elif pct_b >= entry_pct_b_high and is_flat: enter short

exit on bb_middle touch or stop.
```

## Expected regime

All regimes; the strategy's *point* is to be robust across regime
transitions without re-tuning. Best evaluated on a long window that
includes a regime shift (e.g., a calm-to-crisis transition).

## Data dependencies

None beyond price.

## Status

`queued`. Tests the period-substitution axis. Most likely outcome:
adaptive-period helps marginally on average but matters more during
regime transitions, where it avoids the lag-then-overshoot pattern of
fixed-period BB.

## References

- Compendium README §The 7 axes, axis 3 (period).
- Conceptually related: KAMA (Kaufman Adaptive Moving Average) — same
  adaptation philosophy applied to the centerline rather than the period.
- Comparison target: [`bb_meanrev_zscore`](bb_meanrev_zscore.md) at
  `adaptation_strength = 0` is the same strategy with fixed period.

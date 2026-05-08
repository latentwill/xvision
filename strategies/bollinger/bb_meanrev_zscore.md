# bb_meanrev_zscore

**Status:** queued
**Matrix cells:** S0/S1 × B1/B2 (long), S5/S6 × B1/B2 (short)
**Substitution profile:** default Bollinger config; signal-usage = touch + revert-to-mean

## Thesis

The canonical mean-reversion play. In normal-volatility regimes (B1, B2),
price touching the lower band is a 2-σ event under the SMA's local
distribution and reverts toward the mean more often than it doesn't.
Symmetric for upper-band touches. This is the strategy most people *mean*
when they say "Bollinger Bands."

The strategy depends critically on regime gating. In B3 (expansion), the
same touch becomes a *walk* — price rides the band rather than reverting
— and the mean-revert trade catches a falling knife. The
`max_bandwidth_percentile` parameter prevents firing in expansion regimes.

## Inputs

- `IndicatorPanel.bb_upper_20_2`, `bb_lower_20_2`, `bb_middle_20`.
- Computed: `pct_b = (price - bb_lower) / (bb_upper - bb_lower)`.
- Computed: `bandwidth_pct = pct_rank(bandwidth)` for regime gate.
- `IndicatorPanel.atr_14`.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `bb_period`                    | 20      | 10 / 20 / 50     |
| `bb_mult`                      | 2.0     | 1.5 – 2.5        |
| `entry_pct_b_low`              | 0.05    | 0.0 – 0.20       |
| `entry_pct_b_high`             | 0.95    | 0.80 – 1.0       |
| `max_bandwidth_percentile`     | 70      | 50 – 90          |
| `target` (exit)                | `bb_middle` | bb_middle / bb_opposite_band |
| `stop_atr_multiple`            | 1.5     | 1.0 – 3.0        |
| `time_stop_bars`               | 10      | 5 – 30           |

`max_bandwidth_percentile = 70` keeps this strategy *out* of B3 / B4
expansion regimes — the inverse of the breakout strategy's gate.

## Decision rule

```
in_normal_vol = bandwidth_percentile <= max_bandwidth_percentile

if in_normal_vol and pct_b <= entry_pct_b_low and is_flat:
    enter long, target = bb_middle (or bb_upper if target = bb_opposite_band)
                stop = entry - stop_atr_multiple * atr_14

elif in_normal_vol and pct_b >= entry_pct_b_high and is_flat:
    enter short, target = bb_middle (or bb_lower)
                 stop = entry + stop_atr_multiple * atr_14

exit on target, stop, or time_stop_bars elapsed.
```

## Expected regime

B1 (calm) and B2 (normal). Toxic in B3 / B4 — `max_bandwidth_percentile`
is the regime gate that excludes those.

## Data dependencies

None beyond price.

## Status

`queued`. Highest-volume strategy in the BB family; trades fire frequently
in normal regimes. The strategy that benefits most from the
`max_bandwidth_percentile` gate; without it, edge collapses.

## References

- Compendium README §Combination, S0/S1 and S5/S6 rows × B1/B2 columns.
- Symmetry-aware variant: [`bb_asymmetric_breakouts`](bb_asymmetric_breakouts.md) — uses different rules for upper-touch vs lower-touch.
- Inverse: [`bb_band_walk_follow`](bb_band_walk_follow.md) — same touches, opposite regime, opposite direction.

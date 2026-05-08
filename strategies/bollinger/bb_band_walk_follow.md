# bb_band_walk_follow

**Status:** queued
**Matrix cells:** S5/S6 × B3 (long-walk), S0/S1 × B3 (short-walk)
**Substitution profile:** signal-usage = walk (the inverse of touch-fade)

## Thesis

In an expansion regime (B3 — bandwidth in the top 20% of the trailing
distribution and rising), price often *walks* the band: it rides along
the upper or lower band for an extended sequence of bars rather than
reverting. The walk is the trend-follow signature inside the BB family.

This strategy is the *exact inverse* of `bb_meanrev_zscore`: same
touches, opposite regime, opposite direction. Both strategies look at
S5/S6 — what makes them differ is the bandwidth gate. Together they cover
the BB-touch signal across all volatility regimes.

## Inputs

- `IndicatorPanel.bb_upper_20_2`, `bb_lower_20_2`, `bb_middle_20`.
- Computed: `pct_b`, `bandwidth_percentile`.
- Computed: `bandwidth_slope` — is bandwidth still expanding?
- `IndicatorPanel.atr_14`.

## Parameters

| Param                              | Default | Range            |
| ---------------------------------- | ------- | ---------------- |
| `bb_period`                        | 20      | 10 / 20 / 50     |
| `bb_mult`                          | 2.0     | 1.5 – 2.5        |
| `min_bandwidth_percentile`         | 80      | 60 – 95          |
| `min_bandwidth_slope`              | 0       | 0 – +small       |
| `walk_confirmation_bars`           | 2       | 1 – 5            |
| `entry_pct_b_walk_high`            | 0.85    | 0.70 – 1.0       |
| `entry_pct_b_walk_low`             | 0.15    | 0.0 – 0.30       |
| `exit_on_close_below_middle`       | `true`  | bool             |
| `stop_atr_multiple`                | 2.0     | 1.5 – 3.0        |

`walk_confirmation_bars` requires price to be near the band for ≥N bars,
not a single touch — that's what distinguishes a walk from a touch.

## Decision rule

```
in_expansion = bandwidth_percentile >= min_bandwidth_percentile
               and bandwidth_slope >= min_bandwidth_slope

walking_upper = in_expansion
                and pct_b >= entry_pct_b_walk_high
                for >= walk_confirmation_bars consecutive bars

walking_lower = in_expansion
                and pct_b <= entry_pct_b_walk_low
                for >= walk_confirmation_bars consecutive bars

if walking_upper and is_flat:
    enter long, stop = bb_middle - stop_atr_multiple * atr_14
                exit = close < bb_middle (if exit_on_close_below_middle)
elif walking_lower and is_flat:
    enter short, mirror
```

## Expected regime

B3 (expansion). Hard exit when price closes back through `bb_middle` —
that's the canonical "walk has ended" signal.

## Data dependencies

None beyond price.

## Status

`queued`. Trend-follow counterpart to mean-reversion; ensures BB strategy
population covers both regime types, not just B1/B2.

## References

- Inverse: [`bb_meanrev_zscore`](bb_meanrev_zscore.md) — same touches, B1/B2 instead of B3.
- The walk-the-band pattern was popularized by Connors / Alvarez (TPS strategies).

# bb_climax_fade

**Status:** queued
**Matrix cells:** S0/S1 × B4 (capitulation, long), S5/S6 × B4 (euphoria, short)
**Substitution profile:** signal-usage = touch in extreme regime; symmetric structure, asymmetric tuning

## Thesis

When bandwidth is at an extreme (B4) and price is at the band's outside
edge (S0/S1 or S5/S6), the move has likely overshot. Volatility itself
mean-reverts — extreme bandwidth doesn't sustain — and the extreme touch
typically marks the climax of the move. Fade it.

The strategy explicitly encodes the symmetry-breaking insight: capitulation
(S0/S1 + B4) resolves *faster* than euphoria (S5/S6 + B4). Long-side
trades use tighter time-stops than short-side trades. This is the cleanest
operational expression of the pivot dimension in the compendium.

Tail-risk-aware. The "climax" can extend for several bars before reverting,
so position size must be modest and stops must be loose — but final
realized R can be large because the reversion is often violent.

## Inputs

- `IndicatorPanel.bb_upper_20_2`, `bb_lower_20_2`, `bb_middle_20`.
- Computed: `pct_b`, `bandwidth_percentile`.
- `IndicatorPanel.atr_14`.
- `PriceFrame.volume` — high volume at the extreme increases conviction.

## Parameters

| Param                              | Default | Range            |
| ---------------------------------- | ------- | ---------------- |
| `min_bandwidth_percentile`         | 95      | 90 – 99          |
| `entry_pct_b_low`                  | 0.0     | -0.05 – 0.10     |
| `entry_pct_b_high`                 | 1.0     | 0.90 – 1.05      |
| `min_volume_z`                     | 1.0     | 0.0 – 2.0        |
| `time_stop_long`                   | 5       | 3 – 10           |
| `time_stop_short`                  | 10      | 5 – 20           |
| `target` (exit)                    | `bb_middle` | bb_middle / 50% retrace |
| `stop_atr_multiple`                | 3.0     | 2.0 – 5.0        |
| `max_position_size_factor`         | 0.5     | 0.3 – 1.0        |

`time_stop_long < time_stop_short` is the asymmetric rule — encoded as a
parameter constraint, not as one number applied to both sides.

`max_position_size_factor` reduces the position relative to the strategy
manager's normal size; tail-risk respect.

## Decision rule

```
in_crisis_vol = bandwidth_percentile >= min_bandwidth_percentile

vol_confirm = (volume - rolling_mean(volume, 20)) / rolling_std(volume, 20)
              >= min_volume_z

# Capitulation fade (long-side):
if in_crisis_vol and pct_b <= entry_pct_b_low and vol_confirm and is_flat:
    enter long with size = base_size * max_position_size_factor
    target = bb_middle
    time_stop = time_stop_long bars
    stop = entry - stop_atr_multiple * atr_14

# Euphoria fade (short-side):
if in_crisis_vol and pct_b >= entry_pct_b_high and vol_confirm and is_flat:
    enter short with size = base_size * max_position_size_factor
    target = bb_middle
    time_stop = time_stop_short bars   # wider — euphoria persists longer
    stop = entry + stop_atr_multiple * atr_14
```

## Expected regime

B4 only. Few firings per year on any single instrument; the strategy is a
*specialist* not a workhorse. Best evaluated on multi-instrument runs to
get enough sample.

## Data dependencies

Volume in addition to price.

## Status

`queued`. Encodes the symmetry-breaking pivot directly; even if it
underperforms in evaluation, the post-mortem is informative because it
tests whether the asymmetry-as-edge thesis is real on the venue.

## References

- Compendium README §Symmetry-breaking — the operational rules above are this section.
- Equity analog: VIX > 95th-pct + SPX 2-σ below mean = high-conviction long entry historically.

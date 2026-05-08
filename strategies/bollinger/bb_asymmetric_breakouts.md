# bb_asymmetric_breakouts

**Status:** queued
**Substitution profile:** symmetry: symmetric → asymmetric. The flagship symmetry-breaking strategy.

## Thesis

The structural symmetry of Bollinger Bands (upper and lower equidistant
from the SMA) is a property of the indicator, not of the market. Markets
exhibit behavioral asymmetry:

- **Capitulation is faster than euphoria.** Drawdowns close in fewer bars than rallies, with sharper volume spikes.
- **Volume profile differs.** Lower-band touches under high volume = capitulation flush (long-bias); upper-band touches under high volume = distribution into strength (short-bias).
- **Walks differ.** Lower-band walks (downtrends) often end with a sharp V-bottom; upper-band walks (uptrends) often end with extended distribution / rounding tops.

This strategy encodes those asymmetries: upper-band events and lower-band
events follow *different* decision rules, not mirrored versions of one
rule. It is the operational answer to the compendium's pivot dimension.

## Inputs

- Standard BB indicators: `bb_upper_20_2`, `bb_lower_20_2`, `bb_middle_20`.
- `pct_b`, `bandwidth_percentile`.
- `PriceFrame.volume` and rolling z-score thereof.
- `IndicatorPanel.atr_14`.

## Parameters (split: upper-rule / lower-rule)

### Upper-band rule

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `upper_touch_pct_b`            | 1.0     | 0.95 – 1.05      |
| `upper_volume_z_min`           | 1.5     | 1.0 – 3.0        |
| `upper_action_normal_vol`      | `short` | short / flat     |
| `upper_action_expansion_vol`   | `flat`  | flat / long_walk |
| `upper_time_stop_bars`         | 10      | 5 – 30           |

### Lower-band rule

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `lower_touch_pct_b`            | 0.0     | -0.05 – 0.05     |
| `lower_volume_z_min`           | 2.0     | 1.0 – 4.0        |
| `lower_action_normal_vol`      | `long`  | long / flat      |
| `lower_action_expansion_vol`   | `long`  | long / short_walk / flat |
| `lower_time_stop_bars`         | 5       | 3 – 15           |

The split parameter table is the strategy's signature: the upper and
lower rules are *not* mirror images of each other. The lower rule allows
`long` even in expansion (capitulation V-bottoms), while the upper rule
defaults to `flat` in expansion (no chasing distribution-tops as long).

## Decision rule

```
upper_event = pct_b >= upper_touch_pct_b and volume_z >= upper_volume_z_min
lower_event = pct_b <= lower_touch_pct_b and volume_z >= lower_volume_z_min

regime = "expansion" if bandwidth_percentile >= 80 else "normal"

if upper_event:
    if regime == "normal":
        action = upper_action_normal_vol     # default: short
    else:
        action = upper_action_expansion_vol  # default: flat (don't chase)

if lower_event:
    if regime == "normal":
        action = lower_action_normal_vol     # default: long
    else:
        action = lower_action_expansion_vol  # default: long (capitulation)
```

## Expected regime

All regimes; the strategy is regime-aware in its decision rule rather
than gated to one regime. The point of the strategy is to *exploit*
regime variation in the asymmetry, not to filter it out.

## Data dependencies

Volume.

## Status

`queued`. Highest-novelty BB strategy; encodes the compendium's pivot
dimension as concrete rules. If it doesn't outperform symmetric variants
in evaluation, the symmetry-as-edge thesis needs revisiting.

## References

- Compendium README §Symmetry-breaking.
- Symmetric counterparts: [`bb_meanrev_zscore`](bb_meanrev_zscore.md), [`bb_band_walk_follow`](bb_band_walk_follow.md), [`bb_climax_fade`](bb_climax_fade.md).
- Head-to-head evaluation: this strategy vs. the symmetric three combined,
  on the same instrument and window.

# bb_multi_sigma_stack

**Status:** queued
**Substitution profile:** cardinality: 2 → many. σ-multiplier substituted with stack {1.0, 2.0, 3.0}.

## Thesis

Standard BB collapses the entire deviation distribution into a single 2σ
threshold. By stacking multiple σ multipliers, the strategy gains a
finer-grained read of *how extreme* a touch is. Touching the 1σ band
means something different from touching the 3σ band — the latter is a
much rarer event and historically has been associated with stronger
mean-reversion (or, in trend, stronger continuation).

This is the cleanest cardinality-substitution: instead of "in-bands or
not," the strategy classifies into 4 zones (inside 1σ / between 1σ-2σ /
between 2σ-3σ / beyond 3σ). Each zone has its own action.

## Inputs

- `bb_upper_20_1`, `bb_lower_20_1` (1σ).
- `bb_upper_20_2`, `bb_lower_20_2` (2σ — standard).
- `bb_upper_20_3`, `bb_lower_20_3` (3σ).
- `bb_middle_20`.
- `IndicatorPanel.atr_14`.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `bb_period`                    | 20      | 10 / 20 / 50     |
| `sigma_levels`                 | [1.0, 2.0, 3.0] | various subsets |
| `zone_actions`                 | (see below) | per-zone strategy |
| `stop_atr_multiple`            | 2.0     | 1.5 – 4.0        |

### Zone action defaults

| Zone                    | Long-side action            | Short-side action           |
| ----------------------- | --------------------------- | --------------------------- |
| inside 1σ               | Flat (no signal)            | Flat                        |
| 1σ — 2σ                 | Reduce existing long        | Reduce existing short       |
| 2σ — 3σ                 | Enter mean-reversion (weak) | Enter mean-reversion (weak) |
| beyond 3σ               | Enter mean-reversion (strong) | Enter mean-reversion (strong, w/ caveat) |

The 1σ zone is the "noise zone" — being inside it is normal and produces
no signal. Crossings between zones are the events; deeper crossings get
larger position sizes.

## Decision rule

```
zone(price) =
    "inside_1"  if abs(price - bb_middle) <= bb_1sigma_dist
    "1_to_2"    if bb_1sigma_dist < abs(price - bb_middle) <= bb_2sigma_dist
    "2_to_3"    if bb_2sigma_dist < abs(price - bb_middle) <= bb_3sigma_dist
    "beyond_3"  otherwise

direction = "above" if price > bb_middle else "below"

# Mean-reversion entries on outer zones:
if zone == "2_to_3":
    if direction == "below" and is_flat: enter long, size = base
    if direction == "above" and is_flat: enter short, size = base

if zone == "beyond_3":
    if direction == "below" and is_flat: enter long, size = base * 1.5  (deeper signal, larger size)
    if direction == "above" and is_flat: enter short, size = base * 1.5

# Reduction in inner-outer zones:
if zone == "1_to_2" and in_long_position: reduce_long_size_pct
if zone == "1_to_2" and in_short_position: reduce_short_size_pct

# Exit when price re-enters inside_1.
```

## Expected regime

B1 / B2 (mean-reversion regimes). In B3 / B4, the deeper-zone touches
become walks rather than reversions; gate with bandwidth_percentile.

## Data dependencies

None beyond price; multi-σ bands are derived from the same SMA + std-dev.

## Status

`queued`. Tests whether the cardinality-substitution from 2 to many
produces a step-function improvement or just smooths the same edge.

## References

- Compendium README §Pivot dimensions, cardinality co-pivot.
- Implementation note: `IndicatorPanel` will need three σ multipliers
  computed; that's a one-line addition to the indicator stack.

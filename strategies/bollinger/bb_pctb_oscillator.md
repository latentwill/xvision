# bb_pctb_oscillator

**Status:** queued
**Substitution profile:** cardinality: 2 → continuous. Bands disappear; %B is the signal.

## Thesis

%B is already a normalized oscillator-style value: 0 = lower band, 0.5 =
SMA, 1 = upper band. By dropping the band-line interpretation entirely
and treating %B as a pure oscillator (analogous to RSI), the strategy
removes threshold artifacts: there is no longer a binary "touched / didn't
touch" — the signal is a continuous score that supports continuous sizing.

This strategy is what BB looks like when cardinality is substituted from
2-bands to continuous. The bands are still computed under the hood (to
derive %B), but they are no longer the trade signal — the position-within
is.

## Inputs

- `IndicatorPanel.bb_upper_20_2`, `bb_lower_20_2` (used only to compute %B).
- Computed: `pct_b = (price - bb_lower) / (bb_upper - bb_lower)`.
- Computed: `pct_b_smoothed` — EMA(%B, period=3) for noise reduction.
- `bandwidth_percentile` for regime gating.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `bb_period`                    | 20      | 10 / 20 / 50     |
| `bb_mult`                      | 2.0     | 1.5 – 2.5        |
| `pct_b_smoothing`              | 3       | 1 – 10           |
| `oversold_threshold`           | 0.10    | 0.0 – 0.30       |
| `overbought_threshold`         | 0.90    | 0.70 – 1.0       |
| `neutral_band`                 | [0.40, 0.60] | various |
| `position_mode`                | `continuous` | `continuous` / `binary` |
| `max_bandwidth_percentile`     | 70      | 50 – 90          |

`position_mode = continuous`: position size scales with how oversold /
overbought %B is. Binary mode reduces to a thresholded RSI-style entry.

## Decision rule

```
in_normal_vol = bandwidth_percentile <= max_bandwidth_percentile
if not in_normal_vol: return Flat

# Continuous-sizing variant:
if position_mode == "continuous":
    if pct_b_smoothed <= oversold_threshold:
        long_signal_strength = (oversold_threshold - pct_b_smoothed) / oversold_threshold
        position_size = base_size * long_signal_strength
        enter long with that size
    elif pct_b_smoothed >= overbought_threshold:
        short_signal_strength = (pct_b_smoothed - overbought_threshold) / (1 - overbought_threshold)
        position_size = base_size * short_signal_strength
        enter short

    exit when pct_b_smoothed re-enters neutral_band

# Binary variant (RSI-style):
elif position_mode == "binary":
    if pct_b_smoothed <= oversold_threshold and is_flat:  enter long
    elif pct_b_smoothed >= overbought_threshold and is_flat: enter short
    exit on opposite threshold or neutral_band re-entry
```

## Expected regime

B1 / B2. The continuous-sizing form fires more often than `bb_meanrev_zscore`
because there's no binary threshold — small signals get small positions.
This shifts the strategy's character toward "always quoted" rather than
"intermittently triggered."

## Data dependencies

None beyond price.

## Status

`queued`. Useful as a *family identity* check: does dropping the bands
and trading %B-as-oscillator outperform threshold-based entries? The
answer informs whether the band-touch is information or just a UI
artifact.

## References

- Compendium README §Pivot dimensions, cardinality.
- Comparison target: [`bb_meanrev_zscore`](bb_meanrev_zscore.md) — same regime, threshold vs continuous form.
- Methodologically similar: RSI-based mean reversion (RSI is an oscillator with no underlying band lines).

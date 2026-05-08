# volume_weighted_rsi

**Status:** queued
**Tree branch:** alternate indicator-level fusion (sibling of MFI)
**Approach:** RSI computed on volume-weighted price; volume re-weights bars without rewriting the math

## Thesis

A second indicator-level fusion path. Where MFI fuses volume by computing
RSI on money-flow, volume-weighted RSI (VW-RSI) fuses volume by
*weighting each bar's contribution to the RSI calculation by its volume*.
Bars with higher volume contribute more to the smoothed gains/losses;
bars with low volume contribute less.

The two fusion approaches surface different properties:

- **MFI** treats *typical-price × volume* as the signal — it's "RSI of dollar flow."
- **VW-RSI** treats price change as the signal but *trusts high-volume bars more* — it's "RSI of price, with volume as evidence weight."

Empirically these can differ on the same data: VW-RSI tracks price more closely (the price-change is still the signal), while MFI can decouple from price when dollar-flow diverges. The compendium includes both because the choice between them is itself a substitution-axis worth evaluating.

## Inputs

- `PriceFrame.close`, `PriceFrame.volume`.
- Computed: `vw_rsi_14` —
  ```
  for each bar:
      change = close[t] - close[t-1]
      gain   = max(change, 0)
      loss   = max(-change, 0)
      weight = volume[t]
  vw_avg_gain = exp_moving_average(gain * weight, period=14) / exp_moving_average(weight, period=14)
  vw_avg_loss = exp_moving_average(loss * weight, period=14) / exp_moving_average(weight, period=14)
  vw_rs = vw_avg_gain / vw_avg_loss
  vw_rsi = 100 - (100 / (1 + vw_rs))
  ```
- `IndicatorPanel.atr_14`.

## Parameters

| Param                     | Default | Range            |
| ------------------------- | ------- | ---------------- |
| `vw_rsi_period`           | 14      | 7 / 14 / 21      |
| `vw_rsi_oversold`         | 25      | 15 – 30          |
| `vw_rsi_overbought`       | 75      | 70 – 85          |
| `confirmation_bars`       | 1       | 1 – 3            |
| `target_vw_rsi`           | 50      | 40 – 60          |
| `stop_atr_multiple`       | 2.0     | 1.5 – 3.5        |

Defaults shifted slightly (25/75 instead of 20/80) because volume-weighted
distributions tend to compress: bars with extreme prices but low volume
contribute less, narrowing the realized range.

## Decision rule

Identical in shape to `mfi_money_flow_index`, with `vw_rsi_14` substituted
for `mfi_14`.

## Expected regime

Calm and normal-volatility regimes (same as MFI / RSI). Differentiator
emerges at *the boundary between regimes*, where VW-RSI's bias toward
high-volume bars gives it a different early-warning profile than MFI.

## Data dependencies

Volume in addition to price.

## Status

`queued`. Pair head-to-head with `mfi_money_flow_index` and the
strategy-level-fused strategies. Three-way comparison: indicator fusion
A (MFI), indicator fusion B (VW-RSI), strategy-level fusion (the rest of
the folder).

## References

- Compendium README §Tree-finding — sibling at the "across" branch.
- Comparison: [`mfi_money_flow_index`](mfi_money_flow_index.md).
- Conceptually related to VWAP (volume-weighted average price); same
  re-weighting principle applied to a different signal.

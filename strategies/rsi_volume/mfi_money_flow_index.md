# mfi_money_flow_index

**Status:** queued
**Tree branch:** RSI's natural sibling — RSI fused with volume *inside* the indicator
**Approach:** indicator-level fusion (vs strategy-level fusion of the other strategies in this folder)

## Thesis

The Money Flow Index (MFI) is RSI computed on *money flow* (typical
price × volume) instead of price changes. Mathematically, it bakes the
volume signal into the RSI computation — a 1σ overbought MFI is
*automatically* an overbought reading that volume agrees with. There's
no separate volume gate to tune.

This strategy is the *indicator-level fusion* contrast to every other
strategy in this folder. Its evaluation question is direct: **does
fusing volume into the indicator give better edge than keeping RSI and
volume as independent signals with strategy-level rules?**

If MFI dominates, the rest of the folder is over-engineered. If the
unfused RSI+volume strategies dominate, MFI fuses too early and discards
information. This is the head-to-head experiment the tree-finding
operator surfaced.

## Inputs

- Computed (or panel field): `mfi_14` —
  ```
  typical_price = (high + low + close) / 3
  raw_money_flow = typical_price * volume
  positive_flow = sum of raw_money_flow on bars where typical_price[t] > typical_price[t-1]
  negative_flow = sum on bars where typical_price[t] < typical_price[t-1]
  money_flow_ratio = positive_flow / negative_flow
  mfi = 100 - (100 / (1 + money_flow_ratio))
  ```
  computed over a 14-bar window.
- `PriceFrame.close`, `IndicatorPanel.atr_14`.

## Parameters

| Param                     | Default | Range            |
| ------------------------- | ------- | ---------------- |
| `mfi_period`              | 14      | 7 / 14 / 21      |
| `mfi_oversold`            | 20      | 15 – 30          |
| `mfi_overbought`          | 80      | 70 – 85          |
| `confirmation_bars`       | 1       | 1 – 3            |
| `target_mfi`              | 50      | 40 – 60          |
| `stop_atr_multiple`       | 2.0     | 1.5 – 3.5        |

The parameter set is identical in *form* to a vanilla RSI mean-reversion
strategy. The difference lives entirely in the indicator computation.

## Decision rule

```
if mfi_14 <= mfi_oversold for confirmation_bars and is_flat:
    enter long, target = mfi >= target_mfi, stop = entry - stop_atr_multiple * atr_14

elif mfi_14 >= mfi_overbought for confirmation_bars and is_flat:
    enter short, target = mfi <= target_mfi, stop = entry + stop_atr_multiple * atr_14
```

## Expected regime

Same as `rsi_mean_reversion` — calm and normal-volatility regimes —
with the structural advantage that the volume agreement is *built in*
and the structural disadvantage that the volume signal cannot be
turned off, gated, or asymmetrically applied.

## Data dependencies

Volume in addition to price. MFI is calculable from existing
`PriceFrame` fields with no external data.

## Status

`queued`. Implementation lives next to `rsi_mean_reversion.rs` in the
baselines crate; the head-to-head evaluation against the strategy-level-
fused variants in this folder is the experiment the tree-finding
operator surfaced.

## References

- Compendium README §Tree-finding — MFI flagged as the structural sibling.
- Comparison targets: every strategy in this folder, especially
  [`rsi_capitulation_long`](rsi_capitulation_long.md) and
  [`rsi_euphoria_short`](rsi_euphoria_short.md), which encode similar
  intent at the strategy level.
- Classical reference: Quong & Soudack, *Stocks & Commodities*, 1989
  (MFI introduction).

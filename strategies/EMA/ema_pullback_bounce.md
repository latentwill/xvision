# ema_pullback_bounce

**Status:** queued
**Atlas cell:** P1 (single-EMA-vs-price) / P6 (price-EMA-distance) × R2 (pullback)
**Periods:** 21-EMA on the trade timeframe; 200-EMA as regime gate

## Thesis

In an established trend (regime R1 / R2), price periodically pulls back
to a fast EMA (typically 21) and bounces. The EMA functions as dynamic
support in uptrends and dynamic resistance in downtrends. The bounce is
high-conviction *only* when the underlying trend is intact — confirmed
by price > 200-EMA for longs.

This is the highest win-rate setup in the EMA family. Pays for it with
relatively small per-trade R, so depends on the win rate being real and
the regime gate keeping it real.

## Inputs

- `IndicatorPanel.ema_21` — bounce EMA.
- `IndicatorPanel.ema_200` — regime gate.
- `IndicatorPanel.atr_14` — for "pullback close enough" tolerance and stop.
- `PriceFrame.close`, `PriceFrame.low`, `PriceFrame.high`.

## Parameters

| Param                      | Default | Range            |
| -------------------------- | ------- | ---------------- |
| `bounce_ema_period`        | 21      | 13 / 21 / 34     |
| `regime_ema_period`        | 200     | 100 / 200        |
| `pullback_tolerance_atr`   | 0.5     | 0.2 – 1.5        |
| `confirm_with_close_above` | `true`  | bool — require close back above EMA after touch |
| `stop_atr_multiple`        | 1.0     | 0.5 – 2.0        |
| `take_profit_rr`           | 1.5     | 1.0 – 3.0        |

`pullback_tolerance_atr` defines "close enough to EMA to count as a
touch" — if price wicks within `0.5 * ATR` of the EMA, it counts. Pure
mid-line touches are too rare; small wick-tolerance materially increases
trade count without degrading win rate.

## Decision rule

```
in_bull_regime  = price > ema_200 and slope(ema_200) >= 0
in_bear_regime  = price < ema_200 and slope(ema_200) <= 0

bull_pullback = in_bull_regime
                and recent_low within pullback_tolerance_atr * atr_14 of ema_21
                and current_close > ema_21  (bounce confirmed)

bear_pullback = mirror condition

if bull_pullback and is_flat:
    enter long
    stop = recent_low - stop_atr_multiple * atr_14   (just below the touched EMA)
    target = entry + take_profit_rr * (entry - stop)
elif bear_pullback and is_flat:
    enter short with mirror stops/targets

exit on stop, target, or break of ema_21 against the position.
```

## Expected regime

R2 (pullback) inside R1 (trend). Strictly bounded by the regime gate —
without `in_bull_regime` / `in_bear_regime`, the strategy fails in R5
(chop) where every "pullback to EMA" is just price wandering through.

## Data dependencies

None beyond price.

## Status

`queued`. Pairs naturally with `ema_50_200_golden_cross` — golden-cross
provides initial entry, pullback-bounce provides re-entries on the same
trend. Both can run on the same instrument simultaneously.

## References

- Atlas Page 3 (P1/P6 × R2).
- Regime gate: [`ema_bullbear_regime_filter`](ema_bullbear_regime_filter.md) implements the filter directly.

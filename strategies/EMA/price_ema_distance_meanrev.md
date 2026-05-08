# price_ema_distance_meanrev

**Status:** queued
**Atlas cell:** P6 (price-EMA-distance) × R5 (chop / range)
**Side-effect lift:** trades the *stretch* — distance leaked by EMA's lag

## Thesis

Price stretches away from its EMA, then snaps back. The further the
stretch (measured in ATRs), the higher the mean-reversion pressure.
This is the simplest form of the Bollinger Band logic, expressed
directly: when `|price - EMA| / ATR` exceeds a threshold, fade the
stretch.

The strategy works *only* in R5 (chop / range) and weakly trending
markets. In R1 (strong trend), price-EMA-distance is high *for a reason* —
the trend is real and fading it is suicide. Therefore this strategy must
be regime-gated to *avoid* R1, the inverse of most other strategies'
gating.

## Inputs

- `IndicatorPanel.ema_21` — typical mean-reversion anchor.
- `IndicatorPanel.atr_14`.
- `PriceFrame.close`.
- `IndicatorPanel.ema_200` and its slope — for the *avoid R1* regime gate.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `anchor_ema_period`            | 21      | 13 / 21 / 34     |
| `min_distance_atr`             | 2.5     | 1.5 – 4.0        |
| `max_regime_slope_atr_pct`     | 0.05%   | 0.0% – 0.2%      |
| `target_distance_atr`          | 0.5     | 0.0 – 1.0        |
| `stop_distance_atr`            | 4.0     | 2.5 – 6.0        |

`max_regime_slope_atr_pct` is the *anti-trend* gate: if the 200-EMA is
sloping faster than this threshold, we're in R1 and the strategy must
not fire.

## Decision rule

```
distance = (price - ema_21) / atr_14
regime_slope = slope(ema_200, window=10) / atr_14

abs_regime_slope = abs(regime_slope)
in_chop_or_weak_trend = abs_regime_slope <= max_regime_slope_atr_pct

if in_chop_or_weak_trend
   and distance >= +min_distance_atr
   and is_flat:
    enter short, target = ema_21 + target_distance_atr * atr_14
                stop   = ema_21 + stop_distance_atr   * atr_14
elif in_chop_or_weak_trend
   and distance <= -min_distance_atr
   and is_flat:
    enter long,  target = ema_21 - target_distance_atr * atr_14
                stop   = ema_21 - stop_distance_atr   * atr_14
```

## Expected regime

R5 (chop) and weak R2/R4 transitions. Strictly excluded from R1 by the
regime gate. Surprisingly poor on intraday — distance signals fire fast
and resolve faster, often inside one bar.

## Data dependencies

None beyond price.

## Status

`queued`. Mean-reversion counterweight to the trend-following strategies;
ensures the strategy population is not 100% trend-aligned.

## References

- Atlas Page 5 (Distance side-effect).
- Bollinger Bands — same logic, different anchor (SMA + σ).
- Regime-anti-gate: this strategy *avoids* R1, opposite of most others.

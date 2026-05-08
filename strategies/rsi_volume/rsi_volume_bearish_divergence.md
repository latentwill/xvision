# rsi_volume_bearish_divergence

**Status:** queued
**Map border zone:** CLIMAX COAST ↔ TRAP CITY (volume fading from extreme to dry while RSI remains overbought)
**Cross-domain template:** astronomy "light-curve and spectroscopy disagree"

## Thesis

Classical divergence. Price makes a new high, RSI makes a new high
(or merely confirms the high), but *volume fades* — each successive
push higher comes on lower participation than the prior. The
disagreement between price/RSI (saying "still going up") and volume
(saying "fewer people care") is information: the rally is running out
of buyers without yet having peaked technically.

The astronomy template fits cleanly: a star whose light-curve says
"variable" but whose spectroscopy says "stable composition" is sending
mixed signals — and the resolution is usually that the simpler
explanation (instrument artifact, third-body interference) wins.
Translating: the simpler explanation for "rallying price + falling
volume" is "a few large holders are running out of paper to sell into."

## Inputs

- `IndicatorPanel.rsi_14`.
- `PriceFrame.high`, `PriceFrame.close`, `PriceFrame.volume`.
- Computed: pivot-high detection on price (rolling N-bar local maxima).
- Computed: volume EMA-20 trend at each pivot.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `rsi_period`                   | 14      | 7 / 14 / 21      |
| `pivot_lookback`               | 5       | 3 – 10           |
| `min_pivot_distance_bars`      | 8       | 5 – 20           |
| `divergence_lookback_bars`     | 30      | 15 – 60          |
| `min_rsi_at_first_pivot`       | 70      | 60 – 80          |
| `volume_decline_pct`           | 20%     | 10% – 40%        |
| `confirmation_close_below`     | `true`  | bool — confirm break of recent low after pivot |
| `stop_buffer_atr`              | 0.5     | 0.2 – 1.5        |

`min_rsi_at_first_pivot` ensures the divergence is forming in
overbought territory, not at a random local high mid-trend.

`volume_decline_pct = 20%` requires the second-pivot volume EMA to be at
least 20% below the first-pivot volume EMA — a meaningful drop, not a
small one.

## Decision rule

```
state machine over pivot-highs:
    - identify pivot-high at bar t1 (rolling-max in last pivot_lookback bars).
    - require rsi_14[t1] >= min_rsi_at_first_pivot.
    - wait for next pivot-high at bar t2, with t2 - t1 >= min_pivot_distance_bars
      and t2 within divergence_lookback_bars of t1.
    - confirm: price[t2] >= price[t1] (new or equal high)
              AND ema_volume_20[t2] <= ema_volume_20[t1] * (1 - volume_decline_pct)

once divergence confirmed, wait for confirmation_close_below
    (close < the lowest low between t1 and t2) to enter SHORT.

stop = recent pivot high (t2) + stop_buffer_atr * atr_14
target = test of the swing-low between t1 and t2, then trail.
```

## Expected regime

End-of-rally distribution phases. Multi-day setup; rare but high-quality.

## Data dependencies

None beyond price and volume.

## Status

`queued`. Multi-bar pattern strategy — different from the
single-bar event strategies elsewhere in the folder. Implementation
requires a small state machine (pivot detection + memory) which the
existing baseline crate's `Strategy` trait may need a helper for.

## References

- Compendium README §Map, COAST ↔ TRAP CITY border zone.
- Mirror: `rsi_volume_bullish_divergence` (idea pool) — same pattern at lows.
- Classical reference: Pring, *Technical Analysis Explained*, divergence chapter.

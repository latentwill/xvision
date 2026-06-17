# rsi_capitulation_long

**Status:** queued
**Map region:** CAPITULATION CANYON
**Conditions:** RSI ≤ 20 AND volume ≥ p95 of trailing window
**Cross-domain template:** medical-diagnosis "treat only if both agree"

## Thesis

When RSI prints an extreme oversold reading (≤ 20) *simultaneously* with
volume at the 95th percentile of its trailing distribution, the market
is in capitulation: forced selling, panic exits, and stop-runs all
firing at once. The two-source verification is what distinguishes
capitulation from a slow grind down — RSI alone in a downtrend is
chronic-oversold and gives no edge; volume alone without RSI confirmation
could be ordinary distribution.

The combination — both signals in extreme-agreement — has historically
marked local lows in equities, futures, and crypto. The trade is a long
fade entered into the panic and exited at the mean (RSI ≥ 50 or a
volume-normalization bar).

## Inputs

- `IndicatorPanel.rsi_14` — RSI of close.
- `PriceFrame.volume`.
- Computed: `volume_pct = pct_rank(volume, lookback=lookback)`.
- `IndicatorPanel.atr_14`.
- `PriceFrame.close`.

## Parameters

| Param                     | Default | Range            |
| ------------------------- | ------- | ---------------- |
| `rsi_period`              | 14      | 7 / 14 / 21      |
| `rsi_oversold`            | 20      | 15 – 30          |
| `volume_lookback`         | 60      | 30 – 180         |
| `volume_pct_min`          | 95      | 85 – 99          |
| `confirmation_bars`       | 1       | 1 – 3            |
| `target_rsi`              | 50      | 40 – 70          |
| `time_stop_bars`          | 10      | 5 – 30           |
| `stop_atr_multiple`       | 3.0     | 2.0 – 5.0        |

`stop_atr_multiple = 3.0` is wide because capitulation can extend several
bars before reverting; loose stops are necessary to avoid being shaken
out before the actual low.

## Decision rule

```
capitulation_event = rsi_14 <= rsi_oversold
                     AND volume_pct >= volume_pct_min
                     AND <event holds for confirmation_bars>

if capitulation_event and is_flat:
    enter long
    stop = entry - stop_atr_multiple * atr_14
    target = exit when rsi_14 >= target_rsi
    time_stop = time_stop_bars
```

## Expected regime

Sharp downtrends, capitulation flushes, news-driven panic. Underperforms
in slow grinding declines (no volume spike) and in choppy oversold
oscillations (no extreme volume). The volume gate excludes both failure
modes.

## Data dependencies

None beyond price and volume.

## Status

`queued`. Highest-conviction RSI/volume strategy; cleanest expression of
the medical-consensus template. Pair with `rsi_euphoria_short` for
symmetric coverage of the climax-canyon / climax-coast diagonal.

## References

- Compendium README §Map, CAPITULATION CANYON.
- Pair: [`rsi_euphoria_short`](rsi_euphoria_short.md).
- Equity analog: VIX > p95 + SPX RSI < 20 has been a high-conviction
  long-entry signal historically.

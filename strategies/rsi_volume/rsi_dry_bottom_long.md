# rsi_dry_bottom_long

**Status:** queued
**Map region:** WHIMPER COVE
**Conditions:** RSI ≤ 20 AND volume ≤ p20 of trailing window
**Cross-domain template:** "the selling pressure ran out of fuel"

## Thesis

The mirror of `rsi_dry_top_fade`. When RSI is oversold but volume is dry,
the decline is exhausted: there are no more sellers willing to push the
price lower, even though RSI says the market wants to. The phrase
"selling on no volume" is an old equity desk maxim — declines without
participation are bear traps that resolve with sharp recoveries when
real bid emerges.

Differs sharply from `rsi_capitulation_long`: that one trades the
*panic flush* (volume extreme); this one trades the *whimper bottom*
(volume gone). Both go long oversold RSI, but for opposite microstructure
reasons. Both should be in the strategy population because they trigger
on different days.

## Inputs

Same as `rsi_dry_top_fade`.

## Parameters

| Param                     | Default | Range            |
| ------------------------- | ------- | ---------------- |
| `rsi_oversold`            | 20      | 15 – 30          |
| `volume_pct_max`          | 20      | 10 – 30          |
| `volume_decay_bars`       | 5       | 3 – 10           |
| `confirmation_bars`       | 3       | 2 – 6            |
| `min_decline_pct_5d`      | 5%      | 2% – 15%         |
| `target_rsi`              | 50      | 40 – 60          |
| `time_stop_bars`          | 12      | 6 – 25           |
| `stop_atr_multiple`       | 1.5     | 1.0 – 3.0        |

`min_decline_pct_5d` ensures we're entering after a real decline, not on
a fluke oversold print at sideways prices. The premise of "selling
exhaustion" requires that there has been recent selling.

## Decision rule

```
volume_decaying = volume_pct[t] <= volume_pct_max
                  AND mean(volume_pct[t-volume_decay_bars : t]) <= volume_pct_max + 10

real_decline = (close[t-5] - close[t]) / close[t-5] >= min_decline_pct_5d

dry_bottom = rsi_14 <= rsi_oversold
             AND volume_decaying
             AND real_decline
             AND <event holds for confirmation_bars>

if dry_bottom and is_flat:
    enter long
    stop = entry - stop_atr_multiple * atr_14
    target = rsi_14 >= target_rsi
    time_stop = time_stop_bars
```

## Expected regime

End-of-decline phases where selling has dried up. Best on assets
post-flush, after the initial panic has resolved. Underperforms in
slow-grind declines that haven't yet reached exhaustion (no "no-volume"
condition until late).

## Data dependencies

None beyond price and volume.

## Status

`queued`. Pair with `rsi_capitulation_long` — between them they cover
both microstructure forms of an oversold setup.

## References

- Compendium README §Map, WHIMPER COVE.
- Pair (panic-flush form): [`rsi_capitulation_long`](rsi_capitulation_long.md).
- Mirror (overbought side): [`rsi_dry_top_fade`](rsi_dry_top_fade.md).

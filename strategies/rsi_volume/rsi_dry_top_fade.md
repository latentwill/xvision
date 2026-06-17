# rsi_dry_top_fade

**Status:** queued
**Map region:** TRAP CITY
**Conditions:** RSI ≥ 80 AND volume ≤ p20 of trailing window
**Cross-domain template:** court "physical evidence is missing — testimony alone is unreliable"

## Thesis

When RSI is overbought *but volume is dry*, the rally is mechanical —
price has drifted up without participation. This is a classic bull trap:
the move looks bullish on price alone, but no one is actually buying.
The absence of corroborating volume *is* the signal. The strategy
shorts (or exits longs) on the principle that rallies without volume
are unsustainable and snap back when participants notice.

This is the cleanest expression of the *information value of an absent
signal*. RSI alone says "overbought, fade." Volume agrees by being
quiet. The agreement is "yes, this is a fake move — fade it."

The opposite of `rsi_euphoria_short`: that one fires when both RSI and
volume are extreme; this one fires when RSI is extreme and volume is
*absent*. Different setups, different sizing, different exits.

## Inputs

- `IndicatorPanel.rsi_14`.
- `PriceFrame.volume`.
- Computed: `volume_pct`.
- `IndicatorPanel.atr_14`.
- `PriceFrame.close`.

## Parameters

| Param                     | Default | Range            |
| ------------------------- | ------- | ---------------- |
| `rsi_overbought`          | 80      | 70 – 85          |
| `volume_pct_max`          | 20      | 10 – 30          |
| `volume_decay_bars`       | 5       | 3 – 10           |
| `confirmation_bars`       | 3       | 2 – 6            |
| `target_rsi`              | 55      | 45 – 65          |
| `time_stop_bars`          | 15      | 8 – 30           |
| `stop_atr_multiple`       | 2.0     | 1.5 – 3.5        |

`volume_decay_bars` adds a stronger filter — require volume to have been
declining over recent bars, not just one dry print. This separates "true
dry-top" from "post-event volume normalization."

## Decision rule

```
volume_decaying = volume_pct[t] <= volume_pct_max
                  AND mean(volume_pct[t-volume_decay_bars : t]) <= volume_pct_max + 10

dry_top = rsi_14 >= rsi_overbought
          AND volume_decaying
          AND <event holds for confirmation_bars>

if dry_top and is_flat:
    enter short
    stop = entry + stop_atr_multiple * atr_14
    target = rsi_14 <= target_rsi
    time_stop = time_stop_bars

if in_long_position and dry_top:
    exit long (regardless of short entry decision)
```

## Expected regime

Late-stage drift-up rallies in low-volatility regimes. Best on assets
prone to "ghost rallies" — illiquid alts, post-listing hype phases,
holiday-period drift. Underperforms when RSI runs away in a real trend
(rare under low-volume conditions, but possible).

## Data dependencies

None beyond price and volume.

## Status

`queued`. Distinct edge from `rsi_euphoria_short` — those two cover the
opposite ends of the volume axis at the same RSI extreme.

## References

- Compendium README §Map, TRAP CITY (the dry-volume column).
- Compendium README §Cross-domain, court template ("missing evidence").
- Mirror: [`rsi_dry_bottom_long`](rsi_dry_bottom_long.md) — bear-trap exhaustion.

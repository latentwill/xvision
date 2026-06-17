# volume_breakout_rsi_confirm

**Status:** queued
**Map region:** HIGHWAY (the central trade artery)
**Conditions:** 40 < RSI < 60 AND volume ≥ p80 of trailing window
**Cross-domain template:** earthquake-prediction "count first, then magnitude"

## Thesis

The HIGHWAY cell of the map: RSI is neutral (not extreme) but volume
suddenly surges into the top quintile. A volume surge from a neutral
state usually precedes a directional move. Volume leads; RSI provides
the *direction* by tracking how price responds in the bars immediately
after the surge.

This is an inverted role compared to the other strategies: in TRAP
CITY / CLIMAX COAST / CAPITULATION CANYON / WHIMPER COVE, RSI states
the position and volume confirms or denies. Here volume *initiates*
("something is happening") and RSI *resolves* ("which direction").

The cross-domain template is earthquake prediction: micro-quake count
(volume signal) precedes the magnitude reading (RSI direction). The
sequence is "volume first, RSI second"; trading rules respect that
sequence.

## Inputs

- `IndicatorPanel.rsi_14`.
- `PriceFrame.volume`, `PriceFrame.close`.
- Computed: `volume_pct = pct_rank(volume, lookback=lookback)`.
- Computed: `rsi_change = rsi_14[t] - rsi_14[t-rsi_lookback]`.
- `IndicatorPanel.atr_14`.

## Parameters

| Param                     | Default | Range            |
| ------------------------- | ------- | ---------------- |
| `rsi_neutral_low`         | 40      | 35 – 45          |
| `rsi_neutral_high`        | 60      | 55 – 65          |
| `volume_pct_min`          | 80      | 70 – 95          |
| `rsi_direction_lookback`  | 3       | 2 – 6            |
| `min_rsi_change`          | 5       | 3 – 10           |
| `confirmation_bars`       | 1       | 1 – 3            |
| `stop_atr_multiple`       | 2.0     | 1.5 – 3.0        |
| `take_profit_rr`          | 2.0     | 1.5 – 4.0        |

`min_rsi_change` is the disambiguator: a volume surge with RSI flat tells
you nothing. A volume surge with RSI moving 5+ points in 3 bars tells
you direction.

## Decision rule

```
in_neutral_rsi = rsi_neutral_low < rsi_14 < rsi_neutral_high
volume_surge = volume_pct >= volume_pct_min
rsi_change = rsi_14[t] - rsi_14[t - rsi_direction_lookback]

if in_neutral_rsi and volume_surge and rsi_change >= +min_rsi_change and is_flat:
    enter long
    stop = entry - stop_atr_multiple * atr_14
    target = entry + take_profit_rr * (entry - stop)

elif in_neutral_rsi and volume_surge and rsi_change <= -min_rsi_change and is_flat:
    enter short with mirror stops/targets
```

## Expected regime

Pre-trend / regime-transition. The strategy fires at the *beginning* of
moves, not at extremes. Underperforms in chop where every volume spike
reverses, and in established trends where RSI is already extreme (not
neutral).

## Data dependencies

None beyond price and volume.

## Status

`queued`. The only "trend-initiation" strategy in the folder; all others
trade extremes or confirm post-move. Keeps the population diverse.

## References

- Compendium README §Map, HIGHWAY (central column).
- Compendium README §Cross-domain, earthquake template.
- Sibling at the tree's "across" branch: MACD-with-volume strategies use
  the same volume-first-direction-second template with MACD as the
  direction provider.

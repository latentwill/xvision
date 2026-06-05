# bb_squeeze_breakout_sol_avax

**Status:** queued
**Matrix cell:** B0 (squeeze) × S5/S6 or S0/S1; high-vol breakout variant
**Scope:** SOL and AVAX only

## Thesis

SOL and AVAX often compress hard, then move violently once the squeeze resolves. This strategy is the aggressive version of the canonical BB squeeze breakout: wait for a true low-vol squeeze, require a confirmed directional break with expansion, and ride the move with an ATR-based trailing exit.

The goal is not to be cute. The goal is to capture large directional bursts on two high-beta altcoins while keeping the downside mechanically capped. That makes it suitable for a high-risk, high-reward basket that still has a defined drawdown ceiling.

## Inputs

- `IndicatorPanel.bb_upper_20_2`, `bb_lower_20_2`, `bb_middle_20`
- `IndicatorPanel.atr_14`
- `IndicatorPanel.volume_sma_20`
- `PriceFrame.close`, `high`, `low`, `volume`
- Computed:
  - `bandwidth = (bb_upper - bb_lower) / bb_middle`
  - `bandwidth_percentile = pct_rank(bandwidth, lookback=100)`
  - `volume_ratio = volume / volume_sma_20`

## Parameters

| Param | Default | Range |
| --- | --- | --- |
| `bb_period` | 20 | 10 – 50 |
| `bb_mult` | 2.0 | 1.5 – 2.5 |
| `bandwidth_percentile_max` | 20 | 10 – 30 |
| `breakout_atr_multiple` | 0.75 | 0.5 – 1.5 |
| `volume_ratio_min` | 1.2 | 1.0 – 2.0 |
| `initial_stop_atr_multiple` | 1.8 | 1.2 – 3.0 |
| `trail_atr_multiple` | 2.2 | 1.5 – 4.0 |
| `take_profit_rr` | 3.0 | 2.0 – 5.0 |
| `time_stop_bars` | 24 | 12 – 72 |
| `risk_per_trade_pct` | 0.45 | 0.20 – 0.75 |
| `portfolio_max_drawdown_pct` | 18 | 15 – 20 |

`risk_per_trade_pct` is the main aggressiveness knob. Keep it modest enough that a bad streak does not blow through the drawdown ceiling.

## Decision rule

```
bandwidth = (bb_upper - bb_lower) / bb_middle
in_squeeze = bandwidth_percentile <= bandwidth_percentile_max
volume_expanding = volume_ratio >= volume_ratio_min

break_up = close > bb_upper + breakout_atr_multiple * atr_14
break_down = close < bb_lower - breakout_atr_multiple * atr_14

if was_in_squeeze[t-1] and in_squeeze and volume_expanding and break_up and is_flat:
    enter long
    stop = entry - initial_stop_atr_multiple * atr_14
    trail = max(trail, close - trail_atr_multiple * atr_14)
    target = entry + take_profit_rr * (entry - stop)

elif was_in_squeeze[t-1] and in_squeeze and volume_expanding and break_down and is_flat:
    enter short
    stop = entry + initial_stop_atr_multiple * atr_14
    trail = min(trail, close + trail_atr_multiple * atr_14)
    target = entry - take_profit_rr * (stop - entry)

exit on:
  - stop hit,
  - trailing stop hit,
  - target hit,
  - opposite break after entry,
  - `time_stop_bars` elapsed.

If portfolio drawdown >= `portfolio_max_drawdown_pct`, stand down until the next evaluation window.
```

## Expected regime

Best in B0 → B3 transitions on SOL and AVAX when volatility expands cleanly after compression. It should be avoided in choppy B1/B2 drift and in B4 crisis conditions unless the platform explicitly wants to test a crisis breakout variant.

## Data dependencies

None beyond price and standard volume/indicator panels.

## Status

`queued`. This is the aggressive SOL/AVAX variant to test against the compendium's more conservative BB setups.

## References

- [`bb_squeeze_breakout`](bb_squeeze_breakout.md) — canonical squeeze breakout.
- [`bb_squeeze_failure_fade`](bb_squeeze_failure_fade.md) — false-break sibling.
- [`../README.md`](../README.md) — Bollinger compendium index and regime map.

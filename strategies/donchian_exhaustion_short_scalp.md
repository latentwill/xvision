# Donchian Exhaustion Short Scalps

## Thesis

This strategy is a very selective 15m intraday short-scapper. It looks for an exhausted upside move that has already broken out of a Donchian range, then waits for strong confirmation that the move is stretched rather than continuing cleanly. The goal is to catch fast failed-breakout or snapback shorts only when volatility, momentum, and participation all agree that the move is late.

The strategy should *not* trade often. A strong filter gate is intentional: if the setup is not unusually clean, the correct action is to skip.

## Inputs

- `PriceFrame.close`
- `PriceFrame.high`
- `PriceFrame.low`
- `PriceFrame.volume`
- `IndicatorPanel.donchian_upper_20`
- `IndicatorPanel.donchian_lower_20`
- `IndicatorPanel.atr_pct_14`
- `IndicatorPanel.bb_width_20`
- `IndicatorPanel.rsi_14`
- `IndicatorPanel.ema_20`
- `IndicatorPanel.volume_zscore_20`

## Parameters

- `timeframe`: 15m
- `donchian_lookback`: 20
- `rsi_overbought_gate`: 70 to 78
- `atr_pct_gate`: 0.8 to 1.25
- `bb_width_gate`: 0.025 to 0.05
- `volume_zscore_gate`: 1.0 to 2.0
- `time_stop_bars`: 4 to 12
- `stop_atr_multiple`: 1.5 to 2.5
- `cooldown_bars`: 6 to 18

## Decision rule

- Only wake when price has already pushed through the Donchian upper band and the move is stretched.
- Require high relative volatility and wide bands so the move is happening in an active market, not dull chop.
- Require momentum to be overheated and participation to be elevated.
- Prefer short only when the breakout looks like an exhaustion move rather than a healthy trend continuation.
- If the move stays strong, do not fade it; skip instead.
- If the short is entered, cover quickly on mean reversion or invalidation.

Pseudocode:

```text
if
  close crosses_above donchian_upper_20 and
  rsi_14 >= overbought_gate and
  atr_pct_14 >= atr_pct_gate and
  bb_width_20 >= bb_width_gate and
  volume_zscore_20 >= volume_zscore_gate and
  close > ema_20:
    if breakout_fails_or_rolls_over:
        short
    else:
        skip
else:
    skip
```

## Expected regime

- 15m high-volatility expansion
- Late-stage breakout failure
- Fast intraday exhaustion after a strong impulse
- Assets with enough liquidity to support sharp but brief dislocations

## Data dependencies

- Donchian, ATR%, Bollinger width, RSI, EMA, and volume z-score indicators available in the indicator panel
- No external feeds required beyond standard OHLCV/indicator data

## Status

queued

## References

- `strategies/README.md`
- `crates/xvision-dashboard/wiki/filter-dsl-catalog.md`
- `references/high-volatility-short-strategy-recipe.md`
- `references/xvision-filter-gated-strategy-recipe.md`

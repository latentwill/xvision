# range_reversion_rsi_bollinger

## Judge summary

A clean mean-reversion example for chop, with a built-in no-trade filter for trend.

## Thesis

When the market is not trending, the edge shifts toward fading extremes instead of chasing moves. This strategy only trades mean reversion after the trend regime is rejected, using RSI and Bollinger extremes to identify stretched moves in a range.

## Failure regime

- Strong trend with persistent EMA slope
- Breakout expansion with increasing volume
- News shock / event regime where extremes can keep running

## Inputs

- `PriceFrame` on 1h candles
- `IndicatorPanel.rsi_14`
- Bollinger band fields from the indicator panel
- Optional ADX for regime rejection
- Optional volume filter to avoid dead markets

## Parameters

- `bb_period = 20`
- `bb_mult = 2.0`
- `rsi_oversold = 30`
- `rsi_overbought = 70`
- `adx_max_for_range = 18`
- `take_profit_midline = true`
- `stop_atr = 1.0`
- `max_hold_bars = 12`

## Decision rule

```text
if 4h or 1h adx_14 > adx_max_for_range:
    do_not_trade_this_strategy

if price closes below lower Bollinger band and rsi_14 <= oversold:
    enter long only if candle shows rejection / reclaim confirmation

if price closes above upper Bollinger band and rsi_14 >= overbought:
    enter short only if candle shows rejection / reclaim confirmation

exit on:
    - return to BB midline
    - opposite band touch
    - stop loss
    - max hold time
```

## Expected regime

- Range-bound markets
- Low-to-moderate volatility where extremes tend to revert
- Non-trending sessions after trend filter has already failed

## Data dependencies

- OHLCV candles at 1h
- RSI and Bollinger indicators
- ADX for regime gating

## Status

queued

## References

- Transcript-derived rule: detect regime first
- Transcript-derived rule: use a higher-timeframe filter
- Transcript-derived rule: require multi-confirmation before entry
- `strategies/README.md`

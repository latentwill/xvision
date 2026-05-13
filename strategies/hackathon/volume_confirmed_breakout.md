# volume_confirmed_breakout

## Thesis

Breakouts are cheap signals by themselves. This strategy only acts when a breakout happens in the direction of the higher-timeframe trend and the breakout candle shows real participation. The goal is to avoid fakeouts and only trade moves that have both structure and activity behind them.

## Inputs

- `PriceFrame` on 15m or 1h for the signal
- `PriceFrame` on 4h for regime bias
- `IndicatorPanel.ema_50`, `IndicatorPanel.ema_200`
- `IndicatorPanel.adx_14`
- Rolling volume average or volume z-score
- Optional ATR for breakout buffer

## Parameters

- `htf = 4h`
- `signal_tf = 1h`
- `lookback_high = 20`
- `lookback_low = 20`
- `volume_mult_min = 1.5`
- `adx_min = 20`
- `breakout_buffer_atr = 0.25`
- `stop_atr = 1.25`
- `take_profit_r_multiple = 2.5`

## Decision rule

```text
if 4h close > 4h ema_200 and 4h ema_50 rising:
    regime = bullish_trend
else:
    regime = no_trade_or_other_strategy

if regime == bullish_trend:
    if close breaks above 20-bar high by breakout_buffer_atr
       and volume > volume_sma * volume_mult_min
       and adx_14 >= adx_min:
        enter long

if breakout fails back below the breakout level quickly:
    exit early
otherwise:
    trail stop under recent swing low or ATR stop
```

## Expected regime

- Trend continuation after compression
- Expansion from a tightening range
- Markets with enough liquidity for volume confirmation to matter

## Data dependencies

- OHLCV candles at 15m/1h and 4h
- Volume average / z-score calculation
- ATR and EMA indicators

## Status

queued

## References

- Transcript-derived rule: detect regime first
- Transcript-derived rule: use a higher-timeframe filter
- Transcript-derived rule: require multiple confirmations
- `strategies/README.md`

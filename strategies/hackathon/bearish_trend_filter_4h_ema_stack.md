# bearish_trend_filter_4h_ema_stack

## Judge summary

A clean bearish mirror of the bull trend strategy: only short when the higher-timeframe trend is clearly down and the lower-timeframe bounce fails.

## Thesis

Trade only when the market is already acting like a downtrend. Use a higher-timeframe filter to decide whether the asset is shortable, then enter on a lower-timeframe rally that fails back into the trend. This is the bearish mirror of the bull pullback strategy.

## Failure regime

- Chop / sideways range
- Fast reversal after the downtrend weakens
- Low-ADX conditions where EMAs are tangled

## Inputs

- `PriceFrame` on 1h candles for entries
- `PriceFrame` on 4h candles for regime detection
- `IndicatorPanel.ema_20`, `IndicatorPanel.ema_50`, `IndicatorPanel.ema_200`
- `IndicatorPanel.adx_14`
- Optional volume confirmation on the rejection candle

## Parameters

- `htf = 4h`
- `entry_tf = 1h`
- `trend_ema_fast = 20`
- `trend_ema_mid = 50`
- `trend_ema_slow = 200`
- `adx_min = 20` or `25`
- `pullback_max_distance_atr = 1.0`
- `stop_atr = 1.5`
- `take_profit_r_multiple = 2.0`

## Decision rule

```text
if 4h ema_20 < ema_50 < ema_200 and 4h adx_14 >= adx_min:
    regime = trend_down
else:
    no_trade

if regime == trend_down:
    wait for 1h rally into ema_20 or ema_50
    require close back below fast EMA
    require either:
      - bearish rejection candle, or
      - volume > recent average on the failed rally, or
      - lower high confirmed
    enter short

exit if:
    - stop loss hit
    - trend filter fails on 4h close
    - take profit reaches target
```

## Expected regime

- Strong downtrend
- Clean pullbacks inside trend
- Low-chop environments where the 4h trend filter stays stable

## Data dependencies

- OHLCV candles at 1h and 4h
- EMA and ADX indicators already available in the analytics pipeline

## Status

queued

## References

- Transcript-derived rule: detect regime first
- Transcript-derived rule: use a higher-timeframe filter
- Transcript-derived rule: require multiple confirmations before entry
- `strategies/README.md`

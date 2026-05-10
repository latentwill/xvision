# Fibonacci Pullback Reentry

## Thesis

Strong trends often retrace before continuing. This strategy waits for a pullback into common Fibonacci retracement zones, then re-enters only when trend structure is still intact and momentum stops weakening.

## Inputs

- `IndicatorPanel.ema_12`
- `IndicatorPanel.ema_26`
- `IndicatorPanel.rsi_14`
- `IndicatorPanel.macd_histogram`
- `IndicatorPanel.bollinger_upper`
- `IndicatorPanel.bollinger_lower`
- `OnchainPanel.funding_rate_8h`
- `Regime`

## Parameters

- `retracement_382`: 0.382 of the recent impulse range
- `retracement_500`: 0.500 of the recent impulse range
- `retracement_618`: 0.618 of the recent impulse range
- `rsi_pullback_floor`: 40 to 50 in an uptrend, 50 to 60 in a downtrend
- `ema_trend_gap`: require short EMA above long EMA for longs, below for shorts
- `funding_filter`: avoid fading a retracement when funding is already extremely crowded

## Decision rule

- If `Regime` is not trending, stay flat.
- If price retraces into the 0.382 to 0.618 zone and the EMA trend still points the same way, look for re-entry.
- Require RSI to stop falling and MACD histogram to flatten or turn back.
- If retracement depth is too shallow, skip it. If it is too deep and trend structure breaks, skip it.
- If funding shows severe crowding against the trend, tighten the filter or stand aside.

Pseudocode:

```text
if regime != Trend:
    flat
elif pullback_in_zone and trend_intact and momentum_turns:
    reenter_with_trend
else:
    flat
```

## Expected regime

- Clean directional markets with orderly pullbacks
- Markets where the move advances in waves instead of in one straight line
- Periods where trend continuation is stronger than mean reversion

## Data dependencies

- OHLCV bars from the market snapshot
- Existing indicator panel values
- Funding rate feed for crowding context
- No extra model inputs required

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `decisions/strategy-choices.md`
- `strategies/x strategy/funding_skew_fade.md`

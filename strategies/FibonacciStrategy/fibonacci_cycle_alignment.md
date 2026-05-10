# Fibonacci Cycle Alignment

## Thesis

Some markets move in nested waves. This strategy only acts when short-term, medium-term, and higher-timeframe structure line up, using Fibonacci-like spacing as a way to think about cycle rhythm and confirmation.

## Inputs

- `IndicatorPanel.ema_12`
- `IndicatorPanel.ema_26`
- `IndicatorPanel.macd_histogram`
- `IndicatorPanel.rsi_14`
- `IndicatorPanel.atr_14`
- `IndicatorPanel.bollinger_upper`
- `IndicatorPanel.bollinger_lower`
- `Regime`
- `OnchainPanel.realized_volatility_30d`

## Parameters

- `trend_alignment_min`: required alignment across short and medium EMAs
- `momentum_confirmation`: MACD and RSI confirmation floor
- `cycle_spacing`: preferred spacing between pullbacks and resumption points
- `volatility_filter`: avoid acting when volatility is too chaotic
- `timeframe_stack`: minimum number of aligned horizons before entry

## Decision rule

- If the regime is noisy or the EMAs disagree, stay flat.
- If the short trend, medium trend, and momentum all agree, take the signal.
- Prefer entries after a measured pullback instead of chasing the first impulse.
- If volatility spikes without alignment, treat the move as random noise.
- If a higher-timeframe trend is present, allow the lower timeframe to time the entry.

Pseudocode:

```text
if regime == Trend and timeframe_stack_aligned and momentum_confirmed:
    trade_with_cycle
else:
    flat
```

## Expected regime

- Nested trends with recurring pullback rhythm
- Markets where structure matters more than raw speed
- Swing setups that benefit from confirmation across horizons

## Data dependencies

- OHLCV bars and indicator panel values
- Realized volatility feed
- No extra model inputs required

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `decisions/strategy-choices.md`
- `strategies/x strategy/fibonacci_pullback_reentry.md`

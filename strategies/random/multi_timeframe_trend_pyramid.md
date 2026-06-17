# Multi-Timeframe Trend Pyramid

## Thesis

Strong trends usually reward patience and scaling. This strategy only participates when the higher-timeframe structure agrees, then adds on shallow pullbacks instead of trying to predict tops.

## Inputs

- `IndicatorPanel.sma_20`
- `IndicatorPanel.sma_50`
- `IndicatorPanel.sma_200`
- `IndicatorPanel.ema_12`
- `IndicatorPanel.ema_26`
- `IndicatorPanel.macd`
- `IndicatorPanel.macd_signal`
- `Regime`
- `MarketSnapshot.recent_bars`

## Parameters

- `trend_stack`: SMA20 above SMA50 above SMA200 for longs, reverse for shorts
- `pullback_depth`: 0.5 to 1.5 ATR retracement from the local impulse
- `add_size_fraction`: 0.25 to 0.5 of initial size on each confirmed pullback
- `max_adds`: 1 to 3

## Decision rule

- Only enter when the moving average stack and MACD agree with the trade direction.
- Use the first entry to establish exposure.
- Add on shallow pullbacks that hold above the trend anchor.
- Exit when the stack compresses or the regime flips out of trend.

Pseudocode:

```text
if trend_stack_aligned and macd_confirmed:
    enter_small
    on_pullback_hold: add_small
    on_trend_failure: exit
else:
    flat
```

## Expected regime

- Persistent directional markets
- Periods where pullbacks are orderly, not violent
- Markets where trend persistence beats mean reversion

## Data dependencies

- MA, EMA, and MACD inputs already in the snapshot
- Recent bars for pullback detection
- No extra feeds required

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `crates/xvision-eval/src/baselines/buy_and_hold.rs`
- `crates/xvision-eval/src/baselines/ma_crossover.rs`

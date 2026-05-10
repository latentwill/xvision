# Volatility Squeeze Breakout

## Thesis

Compression often precedes expansion. This strategy waits for low volatility, then enters with the breakout when price escapes a tight range and the direction agrees with the higher-level trend.

## Inputs

- `IndicatorPanel.bb_upper`
- `IndicatorPanel.bb_middle`
- `IndicatorPanel.bb_lower`
- `IndicatorPanel.atr_14`
- `IndicatorPanel.donchian_upper`
- `IndicatorPanel.donchian_lower`
- `IndicatorPanel.sma_20`
- `IndicatorPanel.sma_50`
- `MarketSnapshot.recent_bars`

## Parameters

- `squeeze_window`: 10 to 30 bars
- `atr_percentile_cutoff`: bottom 20% to 35% of trailing ATR values
- `donchian_breakout_buffer`: 0 to 0.25 ATR
- `trend_alignment`: SMA20 above SMA50 for longs, below for shorts

## Decision rule

- Identify a compression regime using narrow Bollinger width and low ATR.
- Require a breakout through the Donchian band.
- Confirm direction with the short and medium moving averages.
- If the breakout fails quickly and price returns inside the squeeze, exit fast.

Pseudocode:

```text
if squeeze_detected and breakout_up and trend_aligned:
    buy
elif squeeze_detected and breakout_down and trend_aligned:
    sell
elif breakout_failed:
    flat
```

## Expected regime

- Low-volatility compression followed by expansion
- Clean intraday or swing breakouts
- Assets that often trend after range contraction

## Data dependencies

- Bollinger Bands and ATR already in the indicator panel
- Recent bar history for range and confirmation logic

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `crates/xvision-eval/src/baselines/ma_crossover.rs`
- `crates/xvision-eval/src/baselines/macd_momentum.rs`

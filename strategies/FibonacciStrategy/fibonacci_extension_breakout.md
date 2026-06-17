# Fibonacci Extension Breakout

## Thesis

Compression setups often resolve into fast expansion. This strategy looks for a squeeze, then uses Fibonacci extension levels to estimate breakout continuation and exit targets once price escapes the range.

## Inputs

- `IndicatorPanel.bollinger_upper`
- `IndicatorPanel.bollinger_lower`
- `IndicatorPanel.atr_14`
- `IndicatorPanel.rsi_14`
- `IndicatorPanel.ema_12`
- `IndicatorPanel.ema_26`
- `IndicatorPanel.donchian_high`
- `IndicatorPanel.donchian_low`
- `OnchainPanel.open_interest_usd`
- `OnchainPanel.realized_volatility_30d`

## Parameters

- `squeeze_width_max`: narrow Bollinger width threshold
- `breakout_buffer`: small buffer beyond the recent range before entry
- `extension_1272`: 1.272 extension target
- `extension_1618`: 1.618 extension target
- `extension_2618`: 2.618 extension target for strong trends
- `atr_stop`: stop distance scaled to ATR

## Decision rule

- If volatility is compressed and Donchian range is tight, mark a squeeze setup.
- Enter only after a clean break beyond the range with confirming momentum.
- Use 1.272, 1.618, and 2.618 extensions as staged targets.
- If open interest spikes but price cannot hold the breakout, treat it as a false move and exit quickly.
- If realized volatility is already extreme, reduce position size or ignore the setup.

Pseudocode:

```text
if squeeze and breakout_confirmed and momentum_supports:
    enter_breakout
    set_targets([1.272, 1.618, 2.618])
else:
    flat
```

## Expected regime

- Low-volatility compression before expansion
- Assets that frequently trend in impulsive legs
- Periods where range breakouts outperform mean-reversion

## Data dependencies

- OHLCV bars and indicator panel values
- Open interest and realized volatility feeds
- No extra model inputs required

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `decisions/strategy-choices.md`
- `strategies/x strategy/fibonacci_pullback_reentry.md`

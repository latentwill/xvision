# Fibonacci Pyramid Scaling

## Thesis

Instead of entering a full position at once, this strategy scales in as the trade proves itself. It adds exposure in Fibonacci-sized increments when the trend persists and volatility stays controlled.

## Inputs

- `IndicatorPanel.ema_12`
- `IndicatorPanel.ema_26`
- `IndicatorPanel.rsi_14`
- `IndicatorPanel.macd_histogram`
- `IndicatorPanel.atr_14`
- `OnchainPanel.funding_rate_8h`
- `OnchainPanel.long_short_ratio`
- `Regime`

## Parameters

- `base_size`: initial position fraction
- `add_0382`: next add size as 0.382 of base risk
- `add_0618`: third add size as 0.618 of base risk
- `add_1000`: final add size as 1.000 of base risk
- `trend_strength_floor`: minimum EMA and MACD confirmation
- `risk_cap`: hard cap on total exposure

## Decision rule

- Open a small starter position only when trend structure is clear.
- Add the next layer after a confirmed higher high or lower low plus momentum confirmation.
- Add larger increments only if funding and long-short positioning do not show dangerous crowding.
- Stop adding if volatility expands too quickly or momentum weakens.
- Never let the final exposure exceed the preset risk cap.

Pseudocode:

```text
if trend_confirmed:
    enter_small
    if trade_confirms:
        add(0.382)
    if trade_confirms_again:
        add(0.618)
    if strongest_confirmation:
        add(1.000)
else:
    flat
```

## Expected regime

- Persistent trends with orderly pullbacks
- Markets where conviction can be built in steps
- Regimes where overcommitting early is the bigger error

## Data dependencies

- OHLCV bars and indicator panel values
- Funding and positioning feed
- No extra model inputs required

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `decisions/strategy-choices.md`
- `strategies/x strategy/fibonacci_pullback_reentry.md`

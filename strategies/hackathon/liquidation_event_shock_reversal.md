# liquidation_event_shock_reversal

## Judge summary

A crash-recovery example: only fade the panic move after the shock shows exhaustion and stabilization.

## Thesis

Some of the best reversals happen after liquidation cascades or news shocks. This strategy does not try to catch every falling knife. It waits for panic to exhaust, then fades the overshoot once price stabilizes.

## Failure regime

- Trend-day continuation after the first flush
- Illiquid wick traps with no stabilization
- Repeated shock waves that keep making new lows

## Inputs

- `PriceFrame` on 15m or 1h candles
- ATR / volatility expansion
- RSI oversold / overbought
- Wick / rejection candle detection
- Optional liquidation or volume spike proxy if available

## Parameters

- `shock_window = 3 to 6 bars`
- `atr_spike_min = 1.5`
- `rsi_extreme = 20` or `80`
- `rejection_wick_ratio_min = 2.0`
- `stabilization_bars = 2`
- `stop_atr = 1.25`
- `take_profit_r_multiple = 2.0`

## Decision rule

```text
if volatility expands sharply and price flushes through a prior range:
    do not enter immediately

wait for:
    - liquidation / volume spike to peak
    - a rejection wick or reversal candle
    - 2 bars of stabilization after the flush
    - RSI to remain extreme and then stop getting worse

if stabilization confirms:
    enter against the flush

exit if:
    - price makes a fresh extreme with volume expansion
    - stop loss hit
    - target reached
```

## Expected regime

- Event shock
- Liquidation cascade
- Capitulation / panic flush

## Data dependencies

- OHLCV candles at 15m or 1h
- ATR, RSI, and candle-shape calculations
- Optional liquidation feed or proxy

## Status

queued

## References

- Gap analysis: event-shock / liquidation regime
- Transcript-derived rule: detect regime first
- Transcript-derived rule: require multiple confirmations before entry
- `strategies/README.md`

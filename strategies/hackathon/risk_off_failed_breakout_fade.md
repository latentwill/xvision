# risk_off_failed_breakout_fade

## Judge summary

A safety-first regime filter: block aggressive breakout chasing when conditions look hostile, and fade failed breakouts only after confirmation breaks down.

## Thesis

This strategy is less about chasing alpha and more about avoiding bad trades. It detects when the market is in a risk-off condition, then refuses to chase breakouts. If a breakout does happen and fails quickly, the strategy fades the failure instead of forcing momentum.

## Failure regime

- Strong clean trend with real continuation volume
- Persistent breakout expansion that does not fail
- Thin data where the risk-off filter is noisy or unavailable

## Inputs

- `PriceFrame` on 1h or 4h candles
- Volume trend / volume dryness
- Higher-timeframe EMA alignment
- ADX / volatility regime
- Optional onchain or funding stress signals if available

## Parameters

- `risk_off_adx_max = 18`
- `risk_off_volume_min = low`
- `breakout_lookback = 20`
- `failure_confirm_bars = 2`
- `retest_distance_atr = 0.5`
- `stop_atr = 1.0`
- `take_profit_r_multiple = 1.5`

## Decision rule

```text
if risk_off conditions are present:
    suppress breakout chasing

if price breaks a range but fails to hold for failure_confirm_bars:
    wait for a retest
    if retest rejects with weak participation:
        fade the failed breakout

otherwise:
    stay flat
```

## Expected regime

- Risk-off market conditions
- False breakouts / fakeouts
- Weak participation environments where follow-through is unlikely

## Data dependencies

- OHLCV candles at 1h or 4h
- EMA, ADX, and volume regime signals
- Optional onchain / funding stress feed

## Status

queued

## References

- Gap analysis: failure-mode and no-trade coverage
- Transcript-derived rule: detect regime first
- Transcript-derived rule: require multiple confirmations before entry
- `strategies/README.md`

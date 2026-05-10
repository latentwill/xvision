# Funding Skew Fade

## Thesis

When perp funding, long-short positioning, and momentum all lean the same way, the move is often crowded. This strategy fades extreme positioning when the market looks stretched and the regime is not clearly trending.

## Inputs

- `IndicatorPanel.rsi_14`
- `IndicatorPanel.ema_12`
- `IndicatorPanel.ema_26`
- `OnchainPanel.funding_rate_8h`
- `OnchainPanel.long_short_ratio`
- `Regime`

## Parameters

- `funding_high`: 0.01 to 0.04 annualized equivalent, depending on asset
- `funding_low`: -0.01 to -0.04 annualized equivalent
- `rsi_overbought`: 65 to 75
- `rsi_oversold`: 25 to 35
- `long_short_extreme`: 1.3 to 1.8
- `trend_filter_gap`: require EMA spread to be flat or weak

## Decision rule

- If `Regime` is `Trend` and EMAs are strongly aligned, stand aside.
- If funding is high, long-short ratio is long-heavy, and RSI is overbought, prefer `Sell`.
- If funding is deeply negative, short interest is crowded, and RSI is oversold, prefer `Buy`.
- If the crowding is present but momentum is not stretched, stay `Flat`.

Pseudocode:

```text
if trend_is_strong:
    flat
elif funding_extreme and crowd_extreme and rsi_extreme:
    trade_against_crowd
else:
    flat
```

## Expected regime

- Range-bound or late-cycle chop
- Assets with persistent perp markets and visible funding skew
- Periods where crowding builds faster than spot trend confirmation

## Data dependencies

- `xvision-core` market snapshot fields already present in the repo
- Reliable funding and long-short feeds
- No extra model inputs required

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `crates/xvision-eval/src/baselines/rsi_mean_reversion.rs`
- `decisions/strategy-choices.md`

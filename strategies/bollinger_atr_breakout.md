# Bollinger ATR Breakout

## Thesis

Bollinger Bands identify volatility compression and expansion. When price
closes outside the bands *and* ATR-14 confirms elevated volatility, the
move is more likely to sustain than a band-touch during low-vol chop.
This strategy enters in the direction of the breakout and uses ATR-based
position sizing and stop placement so risk stays proportional to current
market noise.

## Inputs

- `PriceFrame` — OHLCV bars (close needed for band/ATR calc)
- `IndicatorPanel` — `bb_upper`, `bb_lower`, `atr_14`

## Parameters

| Parameter | Default | Range | Description |
|-----------|---------|-------|-------------|
| `bb_period` | 20 | 10–50 | Bollinger Band SMA lookback |
| `bb_std` | 2.0 | 1.5–3.5 | Std-dev multiplier for band width |
| `atr_period` | 14 | 10–20 | ATR lookback for confirmation |
| `atr_mult_sl` | 1.5 | 1.0–3.0 | Stop-loss = ATR × mult |
| `atr_mult_tp` | 3.0 | 2.0–5.0 | Take-profit = ATR × mult |
| `size_bps` | 600 | 300–1000 | Position size in basis points |
| `min_atr_pct` | 0.8 | 0.3–2.0 | Minimum ATR as % of price to confirm "elevated" |

## Decision rule

```text
if close > bb_upper and atr_14 / close > min_atr_pct:
    → Buy Long (size_bps)
    stop = close - atr_14 * atr_mult_sl
    target = close + atr_14 * atr_mult_tp

elif close < bb_lower and atr_14 / close > min_atr_pct:
    → Sell Short (size_bps)
    stop = close + atr_14 * atr_mult_sl
    target = close - atr_14 * atr_mult_tp

else:
    → None
```

## Expected regime

- **Works best:** Trending markets with expanding volatility (breakout phases
  after compression).
- **Fails in:** Persistent chop with false band touches and no follow-through;
  very low ATR environments where the confirmation threshold never fires.

## Data dependencies

- `xvision-data` must compute `bb_upper`, `bb_lower`, `atr_14` and populate
  `IndicatorPanel` before the snapshot reaches the baseline.

## Status

`queued`

## References

- `strategies/bollinger/README.md` — Bollinger compendium (symmetry-breaking
  pivot, spectrum × scale matrix).
- `crates/xvision-eval/src/baselines/` — baseline trait implementations.
- `crates/xvision-core/src/market.rs` — `IndicatorPanel` fields.

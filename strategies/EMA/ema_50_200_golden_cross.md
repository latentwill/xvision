# ema_50_200_golden_cross

**Status:** queued
**Atlas cell:** P2 (two-EMA-cross) × R1 (strong trend)
**Periods:** 50 / 200 — high cultural-weight pair (Page 4 of compendium)

## Thesis

The canonical cross. When the 50-EMA crosses above the 200-EMA on a
sustained trend, the cohort of traders who watch this signal coordinates
into long positions — and so do the institutional desks whose risk models
use the 200-EMA as their primary regime line. The signal's reliability
comes from this coordination, not from any property of the underlying
math: an 87/213 cross has identical mathematical content and zero edge,
because nobody watches it.

This strategy lives in the cell where the design intention (smoothing) and
the cultural-emergent property (self-fulfilling S/R) align maximally.
Including it as a baseline is non-negotiable; the question is what
*beats* it, not whether to include it.

## Inputs

- `IndicatorPanel.ema_50` — 50-period EMA of close.
- `IndicatorPanel.ema_200` — 200-period EMA of close.
- `PriceFrame.close`.
- `IndicatorPanel.atr_14` — for stop placement.

## Parameters

| Param                    | Default | Range          |
| ------------------------ | ------- | -------------- |
| `fast_period`            | 50      | fixed (cultural) |
| `slow_period`            | 200     | fixed (cultural) |
| `min_slope_slow`         | 0       | 0 – 0.001 (per bar fraction of price) |
| `confirmation_bars`      | 1       | 1 – 5          |
| `stop_atr_multiple`      | 2.5     | 1.5 – 4.0      |
| `take_profit_rr`         | 2.0     | 1.0 – 4.0      |

`min_slope_slow` filters out crossings that occur because the slow EMA
is rolling sideways — those are R5 (chop) cross-events, not R1 trend
crossings.

## Decision rule

```
golden_cross = ema_50 crossed above ema_200 within last `confirmation_bars`
death_cross  = ema_50 crossed below ema_200 within last `confirmation_bars`

if golden_cross and slope(ema_200, window=5) >= min_slope_slow:
    enter long, stop = entry - stop_atr_multiple * atr_14
elif death_cross and slope(ema_200, window=5) <= -min_slope_slow:
    enter short, stop = entry + stop_atr_multiple * atr_14

exit on opposite cross or stop/take-profit hit.
```

## Expected regime

R1 (strong trend) and the *transition* into R1. Underperforms in R5
(chop) — `min_slope_slow` is the regime filter that prevents entries
during R5 crossings.

## Data dependencies

None beyond price. Pure TA.

## Status

`queued` — first to ship; sets the floor every other EMA strategy must
beat. The existing `ma_crossover.rs` baseline can host this as a
parameterization.

## References

- Atlas Page 3 (P2 × R1) and Page 4 (cultural weight of 50/200).
- `crates/xvision-eval/src/baselines/ma_crossover.rs` — existing infrastructure.
- Standard reference: any TA textbook from the last forty years.

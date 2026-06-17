# ema_trend_persistence

**Status:** queued
**Atlas cell:** P3 (three-EMA-ribbon) / P4 (EMA-slope) / P6 (price-EMA-distance) × R1/R2 (trend + pullback)
**Periods:** 20 / 50 / 200 — fast, mid, and regime anchor

## Thesis

The cleanest moving-average edge is not the first cross; it is *trend persistence*. Once the fast, mid, and slow averages are stacked in the trend direction, the best entries come from brief pullbacks that respect the stack and then reclaim the fast average. That setup filters out a large share of chop while still participating in the middle of the move, where trend continuation is strongest.

This strategy is designed to be mechanical: trend gate first, pullback confirmation second, entry third, and a hard exit if the moving-average structure stops behaving like a trend. It intentionally avoids forecasting tops and bottoms.

## Inputs

- `IndicatorPanel.ema_20` — fast trend line and pullback trigger.
- `IndicatorPanel.ema_50` — mid trend line and structural support/resistance.
- `IndicatorPanel.ema_200` — regime anchor.
- `IndicatorPanel.atr_14` — stop distance and pullback tolerance.
- `PriceFrame.close`, `PriceFrame.high`, `PriceFrame.low`.

## Parameters

| Param | Default | Range |
| --- | ---: | --- |
| `fast_period` | 20 | 13 / 20 / 21 / 34 |
| `mid_period` | 50 | 34 / 50 / 55 |
| `slow_period` | 200 | fixed / 100 / 200 |
| `min_stack_bars` | 3 | 2 – 8 |
| `pullback_tolerance_atr` | 0.75 | 0.25 – 1.5 |
| `reclaim_confirmation_bars` | 1 | 1 – 3 |
| `max_entry_tranches` | 3 | 1 – 4 |
| `initial_risk_fraction` | 0.50 | 0.25 – 0.75 |
| `add_risk_fraction` | 0.25 | 0.10 – 0.40 |
| `stop_atr_multiple` | 1.75 | 1.0 – 3.0 |
| `trail_after_tranche_2` | `true` | bool |

`pullback_tolerance_atr` defines how far price may wick through the fast average and still count as a valid trend pullback. Too tight and the strategy becomes rare; too loose and it starts confusing chop for trend.

## Decision rule

```
bull_regime = price > ema_200 and slope(ema_200, 5) >= 0
bear_regime = price < ema_200 and slope(ema_200, 5) <= 0

bull_stack = ema_20 > ema_50 > ema_200
bear_stack = ema_20 < ema_50 < ema_200

bull_pullback = recent_low touches or slightly pierces ema_20
                within pullback_tolerance_atr * atr_14
                and close reclaims ema_20
                and close stays above ema_50

bear_pullback = mirror condition

if flat and bull_regime and bull_stack for min_stack_bars:
    if bull_pullback:
        enter long with initial_risk_fraction
        stop = recent_swing_low - stop_atr_multiple * atr_14

if long and trail_after_tranche_2:
    add on a higher-low reclaim of ema_20 or a close above the prior swing high
    never exceed max_entry_tranches
    trail stop to below ema_50 once tranche 2 is live

if flat and bear_regime and bear_stack for min_stack_bars:
    if bear_pullback:
        enter short with mirror rules

if long:
    exit on close below ema_50 for 2 consecutive bars,
    or close below ema_200,
    or slope(ema_50, 5) turns negative and ema_20 loses ema_50

if short:
    mirror the exit rules
```

## Position management

- **Entry 1:** establish the position only after the pullback reclaims the fast average.
- **Entry 2:** add only if price makes a higher low and then reasserts the trend.
- **Entry 3:** optional breakout add only when the trend resumes with a fresh expansion bar.
- **Risk:** the first tranche carries the full initial stop; later tranches inherit the same structural invalidation.
- **Trailing:** after tranche 2, the stop can ratchet to just beyond `ema_50` so the trade is no longer dependent on the original swing low.

## Expected regime

Best in:

- R1 (strong trend)
- R2 (pullback in trend)
- early expansion after a clean breakout

Worst in:

- R5 (chop / range)
- late reversal where the ribbon compresses and reorders repeatedly

The `ema_200` gate is the main defense against chop. Without it, this strategy collapses into a generic EMA bounce system and loses its edge.

## Data dependencies

- None beyond standard price and indicator data.
- No external feeds or API keys required.

## Status

`queued` — a mechanical moving-average trend strategy meant to sit between the faster ribbon-style entries and the slower golden-cross baseline.

## References

- `strategies/EMA/ema_50_200_golden_cross.md`
- `strategies/EMA/ema_ribbon_alignment.md`
- `strategies/EMA/ema_pullback_bounce.md`
- `strategies/EMA/ema_bullbear_regime_filter.md`
- `strategies/README.md`

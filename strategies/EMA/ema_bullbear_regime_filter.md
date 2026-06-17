# ema_bullbear_regime_filter

**Status:** queued (filter / meta-strategy, not standalone entry)
**Atlas cell:** P1 × all regimes — meta filter
**Role:** veto layer applied on top of other strategies' decisions

## Thesis

This is not a standalone strategy. It is a *filter* — a meta-rule that
takes another strategy's directional signal and either passes it
through or vetoes it based on whether the regime supports it. The
canonical example: `price > 200-EMA` for longs, `price < 200-EMA` for
shorts, slope-of-200-EMA reinforces the gate.

The atlas Page 3 matrix observed that R5 (chop) collapses every
cross-strategy to AVOID. This filter implements that collapse. Without
it, EMA-cross strategies bleed continuously in chop. With it, they sit
on their hands and preserve capital.

Ships separately because (a) it has its own parameters, (b) it can wrap
*any* directional strategy in the population (including non-EMA ones),
and (c) its evaluation requires comparing
`base_strategy_alone vs base_strategy_filtered` — a meta-experiment.

## Inputs

- `IndicatorPanel.ema_200` — the regime line.
- `PriceFrame.close`.
- The base strategy's proposed action (`Buy` / `Sell` / `Flat` / `Close`).

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `regime_ema_period`            | 200     | 100 / 200 / 300  |
| `min_slope_atr_pct`            | 0.02%   | 0.0% – 0.1%      |
| `slope_window`                 | 10      | 5 – 30           |
| `mode`                         | `strict` | `strict` / `loose` |
| `chop_response`                | `flat`  | `flat` / `pass_through` |

`mode = strict`: require both `price` side and `slope` side to agree.
`mode = loose`: only require `price` side. Strict mode vetoes more
trades; loose mode catches more entries.

`chop_response = flat`: when in chop (slope below threshold AND price
oscillating around EMA), force flat regardless of base signal.

## Decision rule

```
regime_slope = slope(ema_200, window=slope_window) / atr_14
regime_slope_normalized = regime_slope * ATR_normalizer

bull_regime = price > ema_200 and regime_slope_normalized >= +min_slope_atr_pct
bear_regime = price < ema_200 and regime_slope_normalized <= -min_slope_atr_pct
chop_regime = abs(regime_slope_normalized) < min_slope_atr_pct

# loose mode: drop the slope condition.
if mode == "loose":
    bull_regime = price > ema_200
    bear_regime = price < ema_200
    chop_regime = price oscillated across ema_200 within last N bars

# Filter the base strategy's action:
def filter(base_action):
    if chop_regime and chop_response == "flat":
        return Flat
    if base_action == Buy  and not bull_regime: return Flat (veto long)
    if base_action == Sell and not bear_regime: return Flat (veto short)
    return base_action
```

## Expected behaviour

Strict mode reduces trade count by ~40-60% in typical evaluation; the
remaining trades have higher hit rate but lower total opportunity. The
right tuning depends on the base strategy's natural false-positive rate —
high-FP strategies (cross-based) gain more from strict mode; low-FP
strategies (pullback-bounce) may be over-filtered.

## Data dependencies

None beyond price.

## Status

`queued`. Implemented as a `Strategy` *adapter* in the eval harness
rather than a standalone — it composes with another strategy.
Evaluation must report `(filtered_pnl, unfiltered_pnl, kept_trade_pct)`
to make the trade-off visible.

## References

- Atlas Page 3 (the AVOID / FILTER cells in R5 column).
- Wraps: any directional strategy in this folder or `crates/xvision-eval/src/baselines/`.
- Particularly impactful on: [`ema_50_200_golden_cross`](ema_50_200_golden_cross.md), [`ema_ribbon_alignment`](ema_ribbon_alignment.md), [`ema_slope_momentum`](ema_slope_momentum.md).

# ema_acceleration_reversal

**Status:** queued
**Atlas cell:** P5 (EMA-acceleration) × R4 (early reversal)
**Side-effect lift:** trades the *second* derivative — the most under-explored EMA primitive

## Thesis

If slope is momentum, acceleration is momentum-of-momentum. Acceleration
flips *before* slope flips, which flips before EMA-cross occurs:

```
acceleration sign change   →  slope sign change   →  EMA cross
        (earliest)              (earlier)            (latest)
```

Trading acceleration is therefore the earliest possible reversal entry
inside the EMA family. The cost of that early-ness is signal noise: the
second derivative is much noisier than the first, so this strategy
requires *consecutive-bar confirmation* of the acceleration sign flip
before acting.

This cell is sparsely populated in conventional TA — most traders stop
at slope. The atlas page-3 matrix flagged P5 as a fertile under-explored
primitive; this is the prototype.

## Inputs

- `IndicatorPanel.ema_50` — the line whose acceleration is computed.
- Computed: `slope[t] = ema_50[t] - ema_50[t-N]`,
  `accel[t] = slope[t] - slope[t-N]`.
- `IndicatorPanel.atr_14` for normalization and stop.
- `PriceFrame.close`.

## Parameters

| Param                          | Default | Range            |
| ------------------------------ | ------- | ---------------- |
| `ema_period`                   | 50      | 34 / 50 / 100    |
| `slope_window`                 | 5       | 3 – 10           |
| `accel_window`                 | 5       | 3 – 10           |
| `min_accel_atr_pct`            | 0.05%   | 0.01% – 0.3%     |
| `confirmation_bars`            | 3       | 2 – 6            |
| `mode`                         | `reversal` | `reversal` / `momentum_continuation` |
| `stop_atr_multiple`            | 1.5     | 1.0 – 3.0        |

`confirmation_bars` is the critical parameter — set too low (1–2), the
strategy fires constantly on noise; set too high (6+), the early-entry
advantage over slope-strategies disappears.

## Decision rule

```
slope[t] = (ema_50[t] - ema_50[t - slope_window]) / slope_window
accel[t] = (slope[t] - slope[t - accel_window]) / accel_window
accel_norm = accel / atr_14

# Reversal mode — acceleration flips against current slope:
if mode == "reversal"
   and slope < 0   and accel_norm >= +min_accel_atr_pct
       for >= confirmation_bars consecutive bars
       and is_flat:
    enter long  (downtrend losing steam, expect reversal)
elif mode == "reversal"
   and slope > 0   and accel_norm <= -min_accel_atr_pct
       for >= confirmation_bars consecutive bars
       and is_flat:
    enter short (uptrend losing steam)

# Momentum-continuation mode — acceleration agrees with slope:
elif mode == "momentum_continuation"
   and slope > 0 and accel_norm > 0 and is_flat:
    enter long  (accelerating uptrend; rare but high-quality)
... mirror short

exit when acceleration reverses against position.
```

## Expected regime

R4 (early reversal) primarily. The momentum-continuation sub-mode applies
to R1 → stronger-R1 transitions where trend is accelerating. Toxic in
R5 — second derivative of a flat line is pure noise; ZERO-cell rule
applies.

## Data dependencies

None beyond price.

## Status

`queued`. Highest-novelty strategy in the EMA folder; pair with the
slope-momentum and golden-cross strategies on the same instrument to
measure the entry-speed gradient (cross → slope → accel) against
win-rate cost.

## References

- Atlas Page 5 (Acceleration side-effect).
- Atlas Page 3, observation: P5 row is sparse — only 2 ● cells.
- Sibling: [`ema_slope_momentum`](ema_slope_momentum.md).

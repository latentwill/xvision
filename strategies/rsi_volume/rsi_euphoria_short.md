# rsi_euphoria_short

**Status:** queued
**Map region:** CLIMAX COAST
**Conditions:** RSI ≥ 80 AND volume ≥ p95 of trailing window
**Cross-domain template:** medical-diagnosis (consensus); animal-tracking (fresh & strong)

## Thesis

The mirror of `rsi_capitulation_long`. When RSI prints an extreme
overbought reading (≥ 80) at the same time as volume at the 95th
percentile, the move is exhibiting blow-off characteristics: high
participation chasing a price that has already moved a lot. The
combination signals euphoria — fresh longs entering at the worst possible
price.

Asymmetric tuning vs the long-side mirror: euphoria takes *longer* to
resolve than capitulation (the BB compendium's symmetry-breaking insight
applies here too). Time-stops are longer; stops wider; position size
modest because the move can extend further than capitulation moves can
extend down.

## Inputs

Same as `rsi_capitulation_long`.

## Parameters

| Param                     | Default | Range            | Asymmetric vs long-side |
| ------------------------- | ------- | ---------------- | ----------------------- |
| `rsi_period`              | 14      | 7 / 14 / 21      | same                    |
| `rsi_overbought`          | 80      | 70 – 85          | mirror of `rsi_oversold`|
| `volume_pct_min`          | 95      | 85 – 99          | same                    |
| `confirmation_bars`       | 2       | 1 – 4            | **higher** — euphoria persists |
| `target_rsi`              | 50      | 40 – 60          | same                    |
| `time_stop_bars`          | 20      | 10 – 50          | **higher** than long-side (10) |
| `stop_atr_multiple`       | 4.0     | 2.5 – 6.0        | **higher** than long-side (3.0) |
| `position_size_factor`    | 0.7     | 0.4 – 1.0        | **lower** than long-side |

The asymmetric defaults encode the empirical observation that euphoria
moves are slower to resolve than capitulation moves and risk overshoot;
parameters that work for the long-side fade are too tight for the
short-side fade.

## Decision rule

```
euphoria_event = rsi_14 >= rsi_overbought
                 AND volume_pct >= volume_pct_min
                 AND <event holds for confirmation_bars>

if euphoria_event and is_flat:
    enter short with size = base_size * position_size_factor
    stop = entry + stop_atr_multiple * atr_14
    target = exit when rsi_14 <= target_rsi
    time_stop = time_stop_bars
```

## Expected regime

Late-stage rallies, blow-off tops, news-driven euphoria. Often dies in
strong reflexive trends (meme rallies, narrative-driven parabolics) where
RSI stays >80 for many days; the time_stop cuts losses there.

## Data dependencies

None beyond price and volume.

## Status

`queued`. Asymmetric pair to `rsi_capitulation_long` — the asymmetry is
the point, not a parameter accident.

## References

- Compendium README §Map, CLIMAX COAST.
- Pair: [`rsi_capitulation_long`](rsi_capitulation_long.md).
- Symmetry-breaking analog: [`../bollinger/bb_climax_fade`](../bollinger/bb_climax_fade.md) — same insight (capitulation faster than euphoria) operationalized in BB.

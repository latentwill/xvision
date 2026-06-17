# EMA strategies

Compendium of trading strategies built on Exponential Moving Averages —
the canonical smoothing primitive of technical evaluation. EMAs operate
on `PriceFrame` close series and are queryable as
`IndicatorPanel.ema_<period>`.

Generated via the `ideonomy-rich` skill — operators applied:
**organon-construction** + **combination**, organon: **atlas (6 pages)**,
dimension-prompts: **intentionality · side-effect · decomposability**.

## The atlas

This compendium is structured as an *atlas* — a bound collection of
perspectives on the same territory. Six pages, each a different organon:

1. **Mechanistic** — list of EMA primitives (P1–P10).
2. **Regime** — scale of price-action contexts where EMAs live (R1–R5).
3. **Combinatorial** — matrix of primitive × regime → named strategies.
4. **Cultural** — scale of EMA period numbers ranked by cultural weight.
5. **Side-effect** — dictionary of what EMAs leak besides smoothed price.
6. **Decomposability** — tree of independent axes that multiply each strategy.

The pivot dimension is **side-effect**: most existing EMA strategies trade
the *intended* effect (crosses on smoothed price); under-explored alpha
lives in the *leaked* properties — slope (= momentum proxy), acceleration
(= momentum of momentum), distance (= stretch), convergence (= coiled
volatility), and EMA-of-non-price (volume, RSI, etc.).

## Page 1 — EMA primitive vocabulary

| Code | Primitive | Description |
| ---- | --------- | ----------- |
| P1   | single-EMA-vs-price       | `price > EMA` → directional bias |
| P2   | two-EMA-cross             | fast crosses slow → entry/exit signal |
| P3   | three-EMA-ribbon          | stack alignment → trend strength |
| P4   | EMA-slope                 | `d(EMA)/dt` — first derivative; momentum proxy |
| P5   | EMA-acceleration          | `d²(EMA)/dt²` — momentum of momentum |
| P6   | price-EMA-distance        | `|price − EMA| / ATR` — stretch / overextension |
| P7   | EMA-channel               | `EMA ± k·ATR` or `k·σ` — band trading |
| P8   | EMA-convergence           | std-dev of EMA stack — coiled-volatility detector |
| P9   | EMA-of-non-price          | EMA of volume / RSI / MACD-hist / orderbook |
| P10  | EMA-divergence            | price-trend vs EMA-trend disagreement |

## Page 2 — Regime scale

```
strong trend  ●━━━━━━━●━━━━━━━━━○━━━━━━━○━━━━━━━━━━━━○  pure chop / range
                R1       R2        R3       R4            R5
```

| Regime | Name              | EMAs behave |
| ------ | ----------------- | ----------- |
| R1     | Strong trend      | EMAs aligned, slope steep, pullbacks shallow — golden ground |
| R2     | Pullback in trend | Trend intact, brief retracement to fast/mid EMA — bounce setup |
| R3     | Squeeze           | EMAs converging, vol low — coiled, expansion pending |
| R4     | Early reversal    | Slope flips, stack re-orders, cross imminent — most ambiguous |
| R5     | Chop / range      | Crosses are noise, ribbons tangle, slope ≈ 0 — toxic for cross-strategies |

## Page 3 — Primitive × Regime matrix

```
+-----------+-----------+-----------+-----------+-----------+-----------+
| PRIM\REG  |   R1      |   R2      |   R3      |   R4      |   R5      |
|           | trend     | pullback  | squeeze   | reversal  | chop      |
+-----------+-----------+-----------+-----------+-----------+-----------+
| P1 single |    ●      | bounce ●  |    ◇      |    ○      |   FILTER  |
| P2 cross  | golden ●  |    ○      |    ◇      |  fail ○   |   AVOID   |
| P3 ribbon | align ●   |    ●      | squeeze ● |    ◇      |   AVOID   |
| P4 slope  | mom ●     |    ●      |    ◇      | accel ○   |   ZERO    |
| P5 accel  |    ◇      |    ◇      |    ●      | accel ●   |   ZERO    |
| P6 dist   |   stretch | bounce ●  |    ◇      |    ○      | meanrev ● |
| P7 chan   |    ○      |    ◇      |    ◇      |    ◇      | chan ●    |
| P8 conv   |    ●      |    ●      | squeeze ● |    ●      |    ●      |
| P9 non-px |    ●      |    ●      |    ●      |    ●      |    ●      |
| P10 div   |    ◇      |    ◇      |    ◇      |  div ●    |    ◇      |
+-----------+-----------+-----------+-----------+-----------+-----------+

   ●  follow signal           ○  fade / counter-trend
   ◇  open coinage slot       FILTER / AVOID / ZERO  meta-rules
```

**Density observations:**

- **P9 (EMA-of-non-price)** is full ● across all regimes — applying EMA to
  volume / RSI / MACD opens an entire sub-folder. Only one strategy
  scoped here; flagged as fertile ground.
- **P5 (acceleration)** is sparse — the second derivative is the least
  explored EMA primitive in conventional TA. Two ● cells; both candidates
  for edge.
- **R5 (chop)** collapses every cross-strategy to AVOID. A regime filter
  that vetoes EMA-cross strategies in chop is itself a major win and is
  scoped as `ema_bullbear_regime_filter`.

## Page 4 — Cultural weight of EMA periods

```
cultural weight    LOW ○━━━━━━━━━━━●━━━━━━━━━●━━━●━━━━━━━━━━━━━●━●━━━━●  HIGH
                  11   34   55   89    9   21    50  100   200      period
                   ░    ░    ▒   ▒     ▓    ▓    █   █     █        density
```

Period numbers are *intentional* (designer chose them) but their efficacy
is *culturally-emergent*. A 200-EMA "works" partly because every desk
plots it and positions accordingly. An 87-EMA does not — same math,
different cultural payload.

**Implication for parameter optimization:** a sweep that finds 167 as
"optimal" should be *distrusted* — that's overfit to history, not a level
anyone else watches. Round + Fibonacci numbers carry external
coordination payload; their robustness comes from the crowd, not despite
it. This inverts the usual overfit-tolerance heuristic.

## Page 5 — EMA side-effects (dictionary)

| Side-effect       | Type        | What it gives you |
| ----------------- | ----------- | ----------------- |
| Slope             | leaked      | Momentum direction (sign) and strength (magnitude). Cleaner than raw rate-of-change. → `ema_slope_momentum`. |
| Acceleration      | leaked      | Momentum of momentum. Fires *earlier* than slope-flip, which fires earlier than cross. → `ema_acceleration_reversal`. |
| Distance          | leaked      | `|price − EMA| / ATR` — stretch / mean-reversion pressure. → `price_ema_distance_meanrev`. |
| Convergence       | leaked      | Std-dev of EMA stack; collapse → squeeze → expansion. → `ema_squeeze_breakout`. |
| Lag               | unintended  | EMA always trails price — cost, not signal, but informative at trend-flips. |
| Whipsaw           | unintended  | Cross-strategies fail in chop. Fix is regime-gating, not a different EMA. → `ema_bullbear_regime_filter`. |
| Cultural anchor   | unintended  | Round-number EMAs become self-fulfilling S/R. See Page 4. |
| EMA-of-X          | leaked      | Smoothing operator generalizes to any series — volume, RSI, MACD, orderbook. |

## Page 6 — Decomposability tree

Every EMA strategy decomposes into independent axes:

```
STRATEGY
  ├── primitive       P1..P10        (page 1, 10 choices)
  ├── regime          R1..R5         (page 2, 5 choices + regime-gate y/n)
  ├── period(s)       9, 21, 50,     (page 4, ≥2 if primitive ∈ {P2, P3, P8})
  │                   100, 200, custom
  ├── confirmation    standalone / OR-with-momentum / AND-with-volume
  ├── horizon         1h / 4h / 1d / 1w
  └── sizing          fixed / vol-target / Kelly-on-edge
```

Nominal strategy count: 10 × 5 × 6 × 3 × 4 × 3 = **10,800**. Most are
absurd or redundant. The Page 3 matrix collapses primitive × regime to
~12 named survivors; the remaining axes (period × confirmation × horizon
× sizing) multiply each survivor by ~216 and live as the `Parameters`
section inside each strategy file — not as separate files.

## Index

### Queued (full file scoped)

- [`ema_50_200_golden_cross`](ema_50_200_golden_cross.md) — P2 × R1; canonical cultural-weight cross.
- [`ema_ribbon_alignment`](ema_ribbon_alignment.md) — P3 × R1; multi-EMA stack alignment.
- [`ema_pullback_bounce`](ema_pullback_bounce.md) — P1/P6 × R2; buy pullback to fast EMA in trend.
- [`ema_trend_persistence`](ema_trend_persistence.md) — P3/P4/P6 × R1/R2; stacked MAs with reclaim-and-add trend continuation.
- [`ema_slope_momentum`](ema_slope_momentum.md) — P4 × R1; trade slope (derivative), not crosses.
- [`ema_acceleration_reversal`](ema_acceleration_reversal.md) — P5 × R4; second-derivative early-reversal signal.
- [`price_ema_distance_meanrev`](price_ema_distance_meanrev.md) — P6 × R5; stretch mean-reversion in chop.
- [`ema_squeeze_breakout`](ema_squeeze_breakout.md) — P3/P8 × R3; coiled-volatility expansion play.
- [`ema_bullbear_regime_filter`](ema_bullbear_regime_filter.md) — P1; meta-filter, not standalone entry.

### Idea pool (matrix-named, not yet scoped)

- `ema_divergence_reversal` — P10 × R4; price/EMA-trend disagreement.
- `failed_ema_break_reversal` — P2 × R4; failed cross → reversal trade.
- `ema_channel_trade` — P7 × R5; EMA ± ATR rejection trades in chop.
- `ema_of_volume` — P9; EMA of volume → clean volume-trend.
- `ema_of_rsi` — P9; EMA-smoothed RSI → de-noised overbought/oversold.

### Open coinage (◇ slots, no candidate yet)

- P1 × R3 — single-EMA behaviour during squeeze; what's the play?
- P2 × R3 — cross during squeeze; usually noise, but is the *first* post-squeeze cross signal?
- P3 × R4 — ribbon during reversal; stack-disorder as early signal?
- P4 × R3 — slope during squeeze; flat slopes + low convergence as a trigger?
- P5 × R1, R2, R5 — acceleration outside reversal regime; under-explored.
- P6 × R3, R4 — distance during squeeze and reversal.
- P7 × R1–R4 — channel trades outside chop; band-walking as trend confirm?
- P10 × R1–R3, R5 — EMA-divergence outside reversal regime.

## Not surfaced (worth a follow-up ideonomy pass)

1. **Multi-timeframe EMA** — 200-EMA(1d) as regime gate for entries on
   1h cross. Only the horizon axis was touched in Page 6's tree.
2. **EMA vs other MAs** — SMA, WMA, HMA, KAMA, ALMA. Abstraction-lift on
   "EMA" → "any moving smoother" wasn't run. Each MA-type would be a
   substitution variant of this entire atlas.
3. **Adaptive-EMA** — period as a function of volatility (KAMA-style).
   Hinted at by the intentionality dimension (designed → emergent period)
   but no full strategy scoped.
4. **EMA-as-loss-function** — used in eval (track realized-vs-predicted
   with an EMA error), not in trading. Meta-strategy slot.

Suggested next tuple: `substitution + abstraction-lift + tree` over
{smoother-family, adaptive-period, multi-timeframe}.

## See also

- [`../README.md`](../README.md) — parent strategies compendium.
- [`../nansen/README.md`](../nansen/README.md) — sibling Nansen compendium (matrix organon, same independent-axis decomposition).
- `crates/xvision-eval/src/baselines/ma_crossover.rs` — existing MA-cross baseline; `ema_50_200_golden_cross` should be implemented as a parameterization of this or as a sibling.

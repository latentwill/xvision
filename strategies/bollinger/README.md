# Bollinger Band strategies

Compendium of trading strategies built on Bollinger Bands — SMA(period)
± k·σ — and their substitution variants. Bands operate on `PriceFrame`
close series and are queryable as `IndicatorPanel.bb_upper_<period>_<mult>`,
`bb_lower_<period>_<mult>`, plus derived `pct_b` and `bandwidth`.

Generated via the `ideonomy-rich --more` skill — operators applied:
**substitution** + **dimension-identification** + **combination**, organons:
**spectrum (%B continuum)** + **scale (bandwidth volatility)**, dimension-prompts:
**visibility · materiality · naturalness · symmetry · cardinality**.

## Pivot dimensions

**Symmetry (★)** — Bollinger Bands are *structurally* symmetric (upper and
lower equidistant from the SMA). Markets are *behaviorally* asymmetric:
drawdowns are faster than rallies, capitulation differs from euphoria,
volume profile at lower-touch differs from upper-touch. Strategies that
respect the structural symmetry by treating upper-touch and lower-touch
identically *lose this asymmetry as edge.* Symmetry-breaking is the most
generative move in this folder.

**Cardinality (★ co-pivot)** — most BB thinking pins cardinality at
"2 bands" without examining alternatives. Substitution yields four distinct
strategy families:

- **1 band** — collapses to a moving-average strategy (degenerate).
- **2 bands** — standard BB (current default).
- **many bands** — stacked 1σ / 2σ / 3σ; signal = *which* band price is touching.
- **continuous** — drop the bands; trade %B as a pure oscillator (0–1 normalized).

## The 7 axes BB strategies vary on

| Axis             | Default          | Substitution variants |
| ---------------- | ---------------- | --------------------- |
| Centerline       | SMA              | EMA / Hull / KAMA / VWAP / median |
| Dispersion       | std-dev          | ATR (Keltner) / MAD / IQR / EWMA-σ / GARCH-σ |
| Period           | 20               | 10 / 50 / adaptive-vol / adaptive-regime |
| σ-multiplier     | 2.0              | 1.0 / 1.5 / 2.5 / 3.0 / multi-stacked |
| Cardinality ★    | 2                | 1 / many / continuous-as-oscillator |
| Symmetry ★       | symmetric        | asymmetric (upper-rule ≠ lower-rule) |
| Signal-usage     | touch / break    | walk / squeeze / failure / divergence |

The last three axes (cardinality, symmetry, signal-usage) are the
substitution-fertile axes — they are usually pinned to defaults
without examination, so swapping them yields most of the named strategies
in this folder.

## Empty dimensions (flagged and skipped)

The picker drew **materiality** and **naturalness** in this round; both
are empty for BB strategies — all TA is informational and all bands are
man-made. Recording the empty draws so the picker's selection is auditable.

## Organon 1 — Bandwidth scale (volatility regime)

```
bandwidth = (upper_band - lower_band) / middle_band     (normalized)

░░░░░░░░░░ ▒▒▒▒▒▒▒▒▒▒ ▓▓▓▓▓▓▓▓▓▓ ████████████████ ████████████████ ██████████
bw =  0       low(p20)    median       p80          p95            extreme

├─────────────┼──────────┼──────────┼──────────────┼─────────────────────────┤
B0            B1         B2         B3             B4
squeeze       calm       normal     expansion      crisis / climax
```

| Regime | Definition | Owns |
| ------ | ---------- | ---- |
| B0 — Squeeze   | bandwidth in 5th–20th pct of trailing window | squeeze-breakout family |
| B1 — Calm      | bandwidth low-but-stable | mean-reversion strategies |
| B2 — Normal    | bandwidth at median ± noise | baseline; most strategies work here |
| B3 — Expansion | bandwidth in p80+ and rising | walk-the-band trend strategies |
| B4 — Crisis    | bandwidth at extreme | most strategies should down-size, not trade |

## Organon 2 — %B spectrum (price position within bands)

```
%B = (price - lower_band) / (upper_band - lower_band)

◀── below lower (panic) ──╋══════ within bands ══════╋── above upper (euphoria) ──▶
   %B < 0   %B = 0    %B = 0.25   %B = 0.5    %B = 0.75   %B = 1   %B > 1
     S0       S1         S2          S3          S4         S5       S6
```

| Position | Name | Notes |
| -------- | ---- | ----- |
| S0 / S6 | Extreme outside | wicks beyond band; climax exhaustion or trend acceleration |
| S1 / S5 | At-band | touch / kiss; the canonical BB signal point |
| S2 / S4 | Inside-edge | drifting toward edge; early continuation hint |
| S3      | At-centerline | no band-derived signal; neutral position |

The spectrum *looks* symmetric. Behavior isn't (capitulation at S0/S1 ≠
euphoria at S5/S6 in speed of approach, volume profile, and typical
resolution). This is the operational locus of the symmetry-breaking move.

## Combination — %B spectrum × Bandwidth scale

```
+-----------+-------------+-------------+-------------+-------------+-------------+
|  %B \ BW  |   B0        |   B1        |   B2        |   B3        |   B4        |
|           |  squeeze    |  calm       |  normal     |  expansion  |  crisis     |
+-----------+-------------+-------------+-------------+-------------+-------------+
| S0 < 0    |  bias_dn ●  |  meanrev↑●  |  meanrev↑●  |  walk_dn ●  | climax↑ ●   |
| S1 = 0    |  bias_dn ●  |  meanrev↑●  |  meanrev↑●  |  walk_dn ●  | climax↑ ●   |
| S2 ≈ .25  |     ◇       |     ◇       |  drift_dn○  |     ◇       |     ○       |
| S3 = .5   | sq_brk ●    |   no sig    |   no sig    |     ○       |     ○       |
| S4 ≈ .75  |     ◇       |     ◇       |  drift_up○  |     ◇       |     ○       |
| S5 = 1    |  bias_up ●  | meanrev↓ ●  | meanrev↓ ●  |  walk_up ●  | climax↓ ●   |
| S6 > 1    |  bias_up ●  | meanrev↓ ●  | meanrev↓ ●  |  walk_up ●  | climax↓ ●   |
+-----------+-------------+-------------+-------------+-------------+-------------+

  ●  follow / take signal     ○  fade / counter-signal     ◇  open coinage slot
```

**Density observations:**

- Rows S2 / S4 (inside-edge, no touch) are sparse — drift-into-edge is not
  a high-edge signal; the touch *is* the signal point. These rows hold ◇
  slots awaiting cleverness.
- Row S3 (centerline) is a non-signal across all regimes *except* B0 —
  only the squeeze-pre-break trade lives there. Confirms "BB without
  dispersion isn't BB."
- Column B4 (crisis) **inverts the sign** on S0/S1: extreme bandwidth +
  price at lower = capitulation = fade short (long bias). The regime
  overrides the touch direction.

## Symmetry-breaking — the pivot made operational

The matrix is structurally symmetric: S0/S1 mirrors S5/S6 with sign-flipped
cells. The symmetry-break exploits behavioral asymmetry:

- Upper-band touches in B3 (expansion) → *follow* (walk_up); same touch in B1/B2 → *fade*. Mirror for lower.
- Capitulation (S0/S1 in B4) resolves *faster* than euphoria (S5/S6 in B4). Climax-fade-long uses tighter time-stops than climax-fade-short.
- Volume profile asymmetric: lower-touch + high volume = capitulation (long-bias); upper-touch + high volume = distribution (short-bias). `bb_asymmetric_breakouts` encodes this directly.

The symmetric matrix is the canvas; the asymmetric rules are how you trade it.

## Index

### Queued (full file scoped)

- [`bb_squeeze_breakout`](bb_squeeze_breakout.md) — B0 × any %B; first directional break out of squeeze.
- [`bb_squeeze_failure_fade`](bb_squeeze_failure_fade.md) — squeeze breaks then fails → fade the false break.
- [`bb_meanrev_zscore`](bb_meanrev_zscore.md) — B1/B2 × S0/S1 or S5/S6; canonical band-touch fade.
- [`bb_donchian_range_fade`](bb_donchian_range_fade.md) — BB touch fade with Donchian mid-range filter; compact multi-asset crypto setup.
- [`bb_squeeze_breakout_sol_avax`](bb_squeeze_breakout_sol_avax.md) — aggressive SOL/AVAX squeeze breakout with ATR trail and drawdown cap.
- [`bb_band_walk_follow`](bb_band_walk_follow.md) — B3 × S5/S6 (or S0/S1); ride the band in trend.
- [`bb_climax_fade`](bb_climax_fade.md) — B4 × S0/S1 or S5/S6; capitulation/euphoria fade.
- [`bb_asymmetric_breakouts`](bb_asymmetric_breakouts.md) — symmetry-breaking variant (upper-rule ≠ lower-rule + volume).
- [`bb_pctb_oscillator`](bb_pctb_oscillator.md) — cardinality-substitution; %B as oscillator, no bands.
- [`bb_multi_sigma_stack`](bb_multi_sigma_stack.md) — cardinality-substitution; 1σ / 2σ / 3σ stack.
- [`bb_adaptive_volatility`](bb_adaptive_volatility.md) — period-substitution; period scales with vol-of-vol.

### Idea pool (substitution-named or matrix-named, not yet scoped)

- `bb_squeeze_directional_bias` — B0 × S0/S1 or S5/S6; pre-positioned squeeze break.
- `bb_ema_center` — substitute centerline SMA → EMA; faster-reacting bands.
- `bb_keltner` — substitute dispersion σ → ATR; well-known sibling, evaluate vs BB head-to-head.
- `bb_garch_bands` — substitute dispersion σ → forward-looking GARCH-σ.
- `bb_with_volume_confirm` — overlay volume signal on band touches.
- `bb_double_bottom_at_lower` — combine BB with classical pattern at lower band.
- `bb_squeeze_then_walk` — chain: squeeze → break → walk follow.

### Open coinage (◇ slots — drift-into-edge cells, no candidate yet)

- S2 × B0, B1 — drifting toward lower edge during squeeze/calm: predictive of touch?
- S2 × B3 — drifting toward lower edge during expansion: trend acceleration confirm?
- S4 × B0, B1 — mirror of S2 cases on upper side.
- S4 × B3 — mirror.

## Not surfaced (worth a follow-up ideonomy pass)

1. **Multi-timeframe BB** — daily bandwidth as regime gate for hourly-touch
   entries. Horizon axis wasn't drawn this round.
2. **BB as regime-detector for non-BB strategies** — bandwidth-percentile
   gates EMA / Nansen strategies. Cross-strategy plumbing belongs outside
   a single-strategy folder.
3. **Bandwidth-of-bandwidth** — vol-of-vol as a latent-visibility play.
   The visibility dimension's deep end; flagged here but no strategy scoped.
4. **Generalization to volatility-band family** — BB / Keltner / Donchian /
   ATR-channels share structural shape. An abstraction-lift over them
   would identify what's BB-specific vs what's "any-volatility-channel."

Suggested next tuple: `tree-finding + abstraction-lift + dictionary` over
`{BB, Keltner, Donchian, ATR-channel}` — the volatility-band family.

## See also

- [`../README.md`](../README.md) — parent strategies compendium.
- [`../EMA/README.md`](../EMA/README.md) — sibling EMA compendium.
  `bb_ema_center` substitution variant connects directly.
- [`../EMA/ema_squeeze_breakout.md`](../EMA/ema_squeeze_breakout.md) —
  EMA-stack squeeze; same regime as `bb_squeeze_breakout`, different
  detection mechanism. Head-to-head evaluation candidate.
- `crates/xvision-eval/src/baselines/` — implementation target;
  Bollinger queued in FOLLOWUPS SLF7 / F15.

# RSI / Volume strategies

Compendium of trading strategies that combine RSI (Relative Strength
Index, a bounded momentum oscillator) with volume signals. Mantle DEX
trade flow on `PriceFrame.volume`; RSI on `IndicatorPanel.rsi_14`.

Generated via the `ideonomy-rich` skill — operators applied:
**cross-domain-reinstantiation** + **tree-finding**, organon: **map**,
dimension-prompts: **predictability · direction · animacy**.

## Pivot dimensions

**Direction (★)** — RSI is an *oscillator* (bounded 0–100, mean-reverts
to 50). Volume *drifts and spikes* (unbounded, no mean-reversion). They
are categorically different time-series, not two flavors of the same
thing. The strategies live in the *interaction* of these two different
temporal directions: RSI says *where* (price-momentum), volume says
*how loud* (participation). Most strategies that combine them treat
both as noise-to-be-thresholded; the better framing is asking what each
signal *is doing temporally* and how their disagreement is information.

**Animacy (co-pivot)** — RSI is mechanical math; volume is the closest
TA gets to *seeing the lifeforms in the data* — who is trading, how many,
with what urgency. Combining RSI + volume mixes a measurement with a
presence-detector.

## The lift — two-source verification

Lifted form: **a primary signal that may be noisy, paired with an
independent corroborating signal whose presence/absence is itself
informative.**

Cross-domain instances and their templates:

| Domain               | Primary       | Secondary       | Domain's rule                  |
| -------------------- | ------------- | --------------- | ------------------------------ |
| Medical diagnosis    | symptom       | biomarker       | treat only if both agree       |
| Court                | testimony     | physical evid.  | primary direction, secondary R |
| Espionage            | SIGINT        | HUMINT          | confidence × independence      |
| Animal tracking      | paw-prints    | scent           | fresh & strong → act           |
| Astronomy            | light-curve   | spectroscopy    | variability + composition      |
| Earthquake prediction| micro-quakes  | seismic-energy  | count first, then magnitude    |
| Linguistics          | word-freq     | collocation     | both → live phrase             |

The medical-diagnosis template fits best (both signals can independently
err — RSI on noise, volume on idiosyncratic spikes — so independence
multiplies confidence). Lifts back to `rsi_volume_consensus` strategies.

## Tree-finding — siblings and the MFI question

Walking *across* the tree of "oscillator + volume combinations" surfaces
the structural insight: **Money Flow Index (MFI)** is RSI fused with
volume *inside* the indicator. Instead of combining two signals at the
strategy level, MFI fuses them at the indicator level.

| Layer                | Approach                                             |
| -------------------- | ---------------------------------------------------- |
| Strategy-level fuse  | RSI + volume rule (two independent signals)         |
| Indicator-level fuse | MFI / Volume-Weighted RSI (one fused signal)         |

These are not competitors — they are alternate decompositions. The
evaluation question for every strategy in this folder: **why not just
use MFI?** Sometimes the answer is "because the un-fused form gives
independent signals that fuse-inside-indicator hides," but the question
must be asked.

The compendium therefore includes both forms — strategy-level and
indicator-level — for head-to-head evaluation.

## Map — RSI / Volume territory

```
                                  VOLUME  →
              ◀── DRY ──   ─── NORMAL ───  ─── HEAVY ───   ─── EXTREME ──▶

   RSI 80+   ┃ TRAP CITY  ┃             ┃               ┃ CLIMAX COAST   ┃
   over-     ┃ ░░░░░░░░░░ ┃   ridge     ┃    ridge      ┃ ████████████   ┃
   bought    ┃ bull-trap  ┃             ┃               ┃ euphoria fade  ┃
            ─┃────────────┃─────────────┃───────────────┃────────────────┃─
   RSI 60    ┃            ┃             ┃ approach to   ┃ approach to    ┃
            ─┃────────────┃─────────────┃ CLIMAX COAST  ┃ CLIMAX COAST   ┃─
   RSI 50    ┃ DEAD ZONE  ┃   STEPPE    ┃ HIGHWAY    ●  ┃ CRASH SITE     ┃
   neutral   ┃ no signal  ┃ flat field  ┃ vol breakout  ┃ vol shock —    ┃
            ─┃────────────┃─────────────┃ RSI confirms  ┃ wait for RSI   ┃─
   RSI 40    ┃            ┃             ┃ approach to   ┃ approach to    ┃
            ─┃────────────┃─────────────┃ CAPIT. CANYON ┃ CAPIT. CANYON  ┃─
   RSI 20-   ┃ WHIMPER    ┃             ┃               ┃ CAPITULATION   ┃
   over-     ┃ COVE       ┃   ridge     ┃    ridge      ┃ CANYON         ┃
   sold      ┃ ░░░░░░░░░░ ┃             ┃               ┃ ████████████   ┃
             ┃ bear-trap  ┃             ┃               ┃ capitulation   ┃
             ┃ exhaustion ┃             ┃               ┃ long-fade      ┃
```

```
   ░  sparse / low-conviction terrain        █  high-conviction terrain
   ●  named "city" — primary strategy        ridge — transitional border zone
```

| Region              | Conditions                | Strategy |
| ------------------- | ------------------------- | -------- |
| CAPITULATION CANYON | RSI ≤ 20 & volume ≥ p95   | `rsi_capitulation_long` |
| CLIMAX COAST        | RSI ≥ 80 & volume ≥ p95   | `rsi_euphoria_short` |
| TRAP CITY           | RSI ≥ 80 & volume ≤ p20   | `rsi_dry_top_fade` |
| WHIMPER COVE        | RSI ≤ 20 & volume ≤ p20   | `rsi_dry_bottom_long` |
| HIGHWAY             | 40 < RSI < 60 & volume ≥ p80 | `volume_breakout_rsi_confirm` |
| CRASH SITE          | 40 < RSI < 60 & volume ≥ p95 | `rsi_volume_shock_wait` (idea pool) |
| STEPPE              | RSI ~ 50 & volume normal  | no entry (acknowledged dead) |
| DEAD ZONE           | RSI ~ 50 & volume ≤ p20   | AVOID — no signal, no liquidity |

### Border zones — the most interesting territory

- **CANYON ↔ HIGHWAY**: RSI rising from oversold *with* volume sustaining
  → V-bottom confirmation. (`rsi_v_bottom_confirm` — idea pool.)
- **COAST ↔ TRAP CITY**: RSI overbought, volume *fading* from extreme to
  dry → distribution. → `rsi_volume_bearish_divergence`.
- **STEPPE ↔ HIGHWAY**: neutral RSI, volume crossing into heavy →
  impending move, direction unknown. → `volume_breakout_rsi_confirm`.

### Empty territory (unmapped — candidates for exploration)

- **TRAP CITY ↔ STEPPE corridor** (overbought drifting to neutral on dry
  volume): is this a slow-bleed top? No strategy named.
- **The DEAD ZONE itself**: contains no entry but its mere *presence* is
  a regime signal. Could be a meta-filter — *"asset in dead zone? down-size
  every strategy."* Cross-folder use.
- **Middle column** (RSI 60 / RSI 40 under heavy volume): trend-confirm
  territory not currently mapped. Edge during regime transitions?

## Index

### Queued (full file scoped)

- [`rsi_capitulation_long`](rsi_capitulation_long.md) — CAPITULATION CANYON; RSI oversold + volume extreme.
- [`rsi_euphoria_short`](rsi_euphoria_short.md) — CLIMAX COAST; RSI overbought + volume extreme.
- [`rsi_dry_top_fade`](rsi_dry_top_fade.md) — TRAP CITY; RSI overbought + volume dry.
- [`rsi_dry_bottom_long`](rsi_dry_bottom_long.md) — WHIMPER COVE; RSI oversold + volume dry.
- [`volume_breakout_rsi_confirm`](volume_breakout_rsi_confirm.md) — HIGHWAY; volume leads, RSI confirms direction.
- [`rsi_volume_bearish_divergence`](rsi_volume_bearish_divergence.md) — price + RSI new highs while volume fades; classic top divergence.
- [`mfi_money_flow_index`](mfi_money_flow_index.md) — sibling fusion (RSI fused with volume at indicator level).
- [`volume_weighted_rsi`](volume_weighted_rsi.md) — alternate fusion: RSI on volume-weighted price.

### Idea pool (named, not yet scoped)

- `rsi_volume_bullish_divergence` — mirror of bearish-divergence at lows.
- `rsi_volume_consensus` — medical-template; fire only when both signals agree.
- `rsi_directional_volume_sized` — court-template; RSI direction, volume size.
- `rsi_with_volume_freshness` — animal-tracking-template; RSI valid only if volume recent.
- `rsi_count_volume_energy` — earthquake-template; count RSI extremes, volume confirms.
- `rsi_volume_shock_wait` — CRASH SITE; volume-shock without RSI agreement → wait.
- `rsi_v_bottom_confirm` — CANYON↔HIGHWAY border zone.
- `rsi_of_volume` — meta-primitive; apply RSI to volume series itself.
- `chaikin_oscillator` — classical sibling at the tree's "across" branch.
- `obv_with_rsi` — On-Balance Volume + RSI; another classical sibling.

## Not surfaced (worth a follow-up ideonomy pass)

1. **Negation of the two-source concept** — single-source strategies that
   *deliberately ignore* volume even when available. The opposite framing
   ("volume is noise, drop it") wasn't argued in this pass.
2. **Three-source strategies** — RSI + volume + a third signal (Nansen
   smart-money, on-chain flows, EMA regime). Cross-folder integration not
   pursued here.
3. **The DEAD ZONE as a meta-filter** for the entire strategy population.
   Flagged in empty-territory; not scoped.
4. **Adaptive RSI / adaptive volume baseline** — both indicators are at
   fixed periods (RSI-14, volume-EMA-20). Adaptive variants analogous to
   the BB folder's `bb_adaptive_volatility` not pursued.

Suggested next tuple: `combination + abstraction-lift` over
`{RSI, volume, Nansen, EMA-regime}` to design three-source strategies.

## See also

- [`../README.md`](../README.md) — parent strategies compendium.
- [`../EMA/README.md`](../EMA/README.md) — sibling indicator family;
  `ema_of_volume` is one of EMA's idea-pool entries that overlaps this folder.
- [`../bollinger/README.md`](../bollinger/README.md) — sibling indicator
  family; `bb_asymmetric_breakouts` already uses volume as a confirmation.
- [`../nansen/README.md`](../nansen/README.md) — Nansen's labeled-flow
  signals are *labeled volume*; a three-source extension joins this folder
  to that one.
- `crates/xvision-eval/src/baselines/rsi_mean_reversion.rs` — existing RSI
  baseline; volume-aware variants in this folder build on the same
  `IndicatorPanel.rsi_14` field.

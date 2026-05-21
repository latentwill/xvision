# Baseline Side-Effect-Freedom Audit

Audit date: 2026-05-21
Track: eval-lookahead-bias-prober
Auditor: Claude (automated + manual code review)

## Scope

Each baseline in `crates/xvision-eval/src/baselines/` is reviewed for three
properties:

1. **Indicator state held between calls** — does the baseline store any mutable
   state across `decide()` calls that could bias a second-pass replay?

2. **Bar reads classification** — does the baseline read from:
   - `bars[..t]` (past-only, correct), or
   - `bars[..=t]` (current-bar inclusive — leakage risk), or
   - `bars[t..]` (forward — confirmed lookahead)?

   Note: the baselines receive `snapshot.recent_bars` which is a slice of
   recent bars ending at the current bar. By convention `recent_bars.last()`
   is the current bar (time `t`). Reading `recent_bars.last()` without using
   its values in the signal logic is benign; using `recent_bars.last().close`
   *as an input* to the decision (when that bar is still forming) is a
   `bars[..=t]` leakage.

3. **Shared mutable state between instances** — any `static` or global state
   that would contaminate independent prober runs.

---

## `always_long` (AlwaysLong)

**File:** `src/baselines/always_long.rs`

**State held:** None. The struct has no fields.

**Bar reads:** The baseline reads `snapshot.cycle_id` and emits a fixed
decision. It does not read `snapshot.recent_bars` at all. No bar access.

**Shared state:** None.

**Verdict: PASS — side-effect-free. Covered by the prober.**

The prober's negative case uses `always_long` to confirm zero
`lookahead_suspected` findings.

---

## `ma_crossover` (MaCrossover)

**File:** `src/baselines/ma_crossover.rs`

**State held:** `Mutex<Option<(f64, f64)>>` — stores the previous bar's
(sma_fast, sma_slow). This state persists across calls. For the two-pass
prober, each pass gets a **fresh instance** of `MaCrossover`, so the
inter-call state does not pollute the comparison.

**Bar reads:** SMA is computed from `snapshot.recent_bars` via the local
`sma()` function:

```rust
fn sma(bars: &[Ohlcv], window: usize) -> Option<f64> {
    if bars.len() < window { return None; }
    let slice = &bars[bars.len() - window..];
    // …
}
```

`snapshot.recent_bars` is provided by the harness with the bars that were
available *before* the decision point. The last bar in `recent_bars` is the
bar at time `t`. Reading `bars[..=t]` (i.e., the current bar's close) is
a potential leakage if the bar is still forming, but for completed-bar
backtests this is the established bar and is acceptable. The baseline does
not read beyond `recent_bars.len() - 1`.

**Confirmed: no forward-read (`bars[t+1..]`). No shared static state.**

**Verdict: PASS — side-effect-free. Covered by the prober.**

The prober's negative case uses `ma_crossover` to confirm zero
`lookahead_suspected` findings on a sample scenario.

**`bars[..=t]` leakage note:** `MaCrossover` reads the current bar's close
as part of the SMA window. In a completed-bar backtest (as used by the
baseline harness) this is correct behaviour — the bar at `t` has closed
before the decision is made. If the harness were to provide a
partially-completed bar as `recent_bars.last()`, this would be a leakage.
That scenario is out of scope for v1 baselines (see research doc §3.7).
The AUDIT notes the distinction for future reference.

---

## `macd_momentum` (MacdMomentum)

**File:** `src/baselines/macd_momentum.rs`

**State held:** `Mutex<Option<f64>>` — stores the previous bar's `macd_hist`.
Persists across calls. Each prober pass gets a fresh instance, so this does
not pollute the comparison.

**Bar reads:** The baseline reads indicators from `snapshot.indicators`:
`macd`, `macd_signal`, `macd_hist`. These are pre-computed values from the
`IndicatorPanel` and do not read `snapshot.recent_bars` directly. No raw bar
access.

**Confirmed: no `recent_bars` access. No forward-read. No shared static state.**

**Verdict: PASS — side-effect-free. Covered by the prober.**

---

## `rsi_mean_reversion` (RsiMeanReversion)

**File:** `src/baselines/rsi_mean_reversion.rs`

**State held:** None. Struct fields are only the static configuration
(`period`, `oversold`, `overbought`) — no mutable state.

**Bar reads:** Reads `snapshot.indicators.rsi_14` only. No `recent_bars`
access.

**Confirmed: no `recent_bars` access. No forward-read. No shared static state.**

**Verdict: PASS — side-effect-free. Covered by the prober.**

---

## Other baselines (not in the contract's minimum set)

The contract specifies the four baselines above as the required audit scope.
The following are audited for completeness; they are not required acceptance
criteria.

### `always_short` (AlwaysShort)

Symmetric to `always_long`. No fields, no bar reads, no state. **PASS.**

### `buy_and_hold` (BuyAndHold)

**State held:** `AtomicBool` (`entered`). Fires once, then emits `None`.
The prober creates a fresh instance per pass, so the atomic does not
cross passes. **PASS.**

### `random_direction` (RandomDirection)

**State held:** `Mutex<SmallRng>` seeded from a fixed u64. Two passes with
identical seeds produce identical sequences (reproducible). The prober must
seed both passes identically for `random_direction` to be a valid control.
When using the prober with `random_direction`, pass the same seed to both
pass-1 and pass-2 algorithm constructors. **PASS with caveat (same seed
required).**

### `bollinger_atr_breakout` (BollingerATRBreakout)

No mutable state. Reads `snapshot.indicators.bb_upper`, `bb_lower`,
`atr_14`, and `snapshot.price`. No `recent_bars` access. **PASS.**

---

## `bars[..=t]` vs `bars[..t]` distinction

The distinction matters for the prober's Pass 2:

- **`bars[..t]` (past-only):** Pass 2 replays the strategy with bars up to but
  not including bar `t`. A baseline whose signal depends only on `bars[..t]`
  should produce an identical decision when given `bars[..=t-1]`.

- **`bars[..=t]` (current-bar inclusive):** Pass 2 replays with `bars[..=t-1]`
  (one fewer bar). If the baseline reads the last bar's close as part of its
  signal, Pass 2 cannot produce the same bar layout — it will see one bar fewer.
  A pure, non-lookahead baseline should still produce the same *action direction*
  even with one bar fewer (the warmup period may differ, but the structural
  decision should be the same).

  For MaCrossover specifically: if the signal fires at bar `t`, Pass 2 sees
  `bars[..=t-1]`. The SMA window is over the `recent_bars` slice, and bar `t`
  is the slice's last element. Pass 2 misses that bar. A non-lookahead crossover
  fires because `sma_fast(t) > sma_slow(t)` and `sma_fast(t-1) <= sma_slow(t-1)`.
  Without bar `t`, the crossover hasn't fired yet → Pass 2 emits `None`.
  This is the correct behaviour and is NOT a lookahead finding — the divergence
  is the expected consequence of withholding bar `t`, not of reading future bars.

  The prober handles this correctly: it asserts that Pass 2 produces the same
  action, not the same `Some/None` — divergence in action direction is the
  lookahead signal. See `prober/lookahead.rs` for the exact comparison logic.

- **`bars[t..]` (forward):** Any baseline reading beyond the current bar
  is confirmed lookahead. None of the v1 baselines do this.

---

## CI lint note

Future baseline additions should include an AUDIT entry in this file. A CI
lint that fails when a new `.rs` file appears in `src/baselines/` without a
corresponding entry here would close the gap. This is deferred to a follow-up
task; the contract for this track requires the doc, not the CI lint.

---

## Summary table

| Baseline              | Mutable state | Bar reads      | Forward read | Verdict |
|-----------------------|---------------|----------------|--------------|---------|
| `always_long`         | None          | None           | No           | PASS    |
| `always_short`        | None          | None           | No           | PASS    |
| `buy_and_hold`        | AtomicBool    | None           | No           | PASS    |
| `random_direction`    | Mutex<Rng>    | None           | No           | PASS (same seed required) |
| `rsi_mean_reversion`  | None          | IndicatorPanel | No           | PASS    |
| `ma_crossover`        | Mutex<prev>   | recent_bars[..=t] (SMA window) | No | PASS |
| `macd_momentum`       | Mutex<prev>   | IndicatorPanel | No           | PASS    |
| `bollinger_atr_breakout` | None       | IndicatorPanel + price | No  | PASS    |

# Optimizer Config

`autooptimizer.toml` controls the nightly Optimizer cycle (`xvn optimize`).
This page covers the evaluation-window fields, the 120-day cap, and the
`max_window_days` opt-in override that lets power users raise it.

For the full cycle overview and CLI flags, see [Optimizer](/docs?slug=optimizer)
and [CLI Reference](/docs?slug=cli-reference).

---

## Evaluation windows

The cycle backtests each candidate experiment against two windows:

| Field | Purpose |
|---|---|
| `day_window` | The primary evaluation window: number of days of bar history loaded per candidate for scoring. |
| `baseline_untouched_window` | The holdout window used to verify the parent strategy's score was not degraded. Must not overlap `day_window` by design. |

Both fields accept an integer number of days. Example:

```toml
# autooptimizer.toml
day_window = 90
baseline_untouched_window = 30
```

---

## The 120-day `MAX_WINDOW_DAYS` cap

By default, both `day_window` and `baseline_untouched_window` are hard-capped
at **120 days**.

**Why the cap exists.** Each candidate experiment in the cycle loads a full
bar history for the configured window before running a backtest. At wide
timeframes with many candidates, a >120-day window can load tens of thousands
of bars per candidate, exhausting the cycle container's memory budget and
killing the run mid-cycle. The 120-day default keeps per-candidate memory
headroom within safe bounds for typical operator hardware (4–8 GB container
or VPS).

**What happens when you exceed it.** If your `autooptimizer.toml` sets
`day_window` or `baseline_untouched_window` above 120 and `max_window_days`
is not set, `xvn optimize` (and `xvn optimize run`) will fail validation
with a field-level error naming the offending key and pointing you to
`max_window_days`:

```
error: `day_window` (180) exceeds the 120-day default cap.
  To raise the cap, set `max_window_days = 180` (or higher) in autooptimizer.toml.
  Note: longer windows load more bars per candidate and increase memory usage.
```

---

## `max_window_days` — opt-in override

`max_window_days` raises the effective cap for `day_window` and
`baseline_untouched_window`. It is **unset by default** (the 120-day cap
applies). Set it when you deliberately need wider evaluation windows and
have verified the cycle container has the memory headroom to handle them.

```toml
# autooptimizer.toml — raise the cap to allow 180-day evaluation windows
max_window_days = 180

day_window = 180
baseline_untouched_window = 60
```

Constraints:

- Must be **>= 1**. Values below 1 fail validation.
- Has no upper bound in the validator — the responsibility for memory
  headroom is the operator's when overriding the default.
- Applies **only** to `day_window` and `baseline_untouched_window`. The
  `regime_set` and `scenario_pool` windows remain capped at the 120-day
  default regardless of `max_window_days`; raising those is a documented
  follow-up.

**Memory headroom guidance.** A rough rule: each extra 30 days of `day_window`
at hourly granularity adds ~720 bars × 8 fields × 8 bytes ≈ ~46 KB per
candidate. With 10 candidates per cycle that is ~460 KB. At daily granularity
the footprint is ~100× smaller and raising the cap is usually safe. At
sub-hourly granularity (15m, 5m), bar counts multiply quickly — test with a
single short cycle (`xvn optimize run --max-cycles 1`) and monitor container
RSS before committing to a wide window in production.


## Gate quality thresholds

Beyond the evaluation window settings, two additional gate dimensions prevent
the optimizer from selecting degenerate strategies:

### `min_trade_retention_ratio`

Prevents **0-trade degenerate strategies** — candidates that delete all
trade-triggering logic and "improve" Sharpe from negative to 0.0 by simply
refusing to enter any position.

| Property | Value |
|---|---|
| **Key** | `min_trade_retention_ratio` |
| **Default** | `0.5` (50%) |
| **Range** | `(0.0, 1.0]` |
| **Semantics** | Child must execute at least `floor(parent.n_trades × ratio)` fill legs, with a hard floor of 1. |
| **Disable** | Set to `0.0` (not recommended — disables the guard entirely). |

```toml
# autooptimizer.toml
min_trade_retention_ratio = 0.5   # child must retain ≥50% of parent's trade count
```

### `min_realized_return_ratio`

Prevents **"open and hope" strategies** — candidates with strong
mark-to-market (unrealized) equity but negligible booked (realized) profit.
A strategy that opens 100 positions and closes only the 3 winners shows a
healthy Sharpe from the winners' MTM but sits on 97 unrealized losers.

| Property | Value |
|---|---|
| **Key** | `min_realized_return_ratio` |
| **Default** | `0.25` (25%) |
| **Range** | `[0.0, 1.0]` |
| **Semantics** | Child's realized PnL (as % of capital) divided by total return must be ≥ `min_realized_return_ratio`. Skipped when total return ≤ 0 (ratio is mathematically meaningless) or when the ratio is `0.0` (disabled). |
| **Disable** | Set to `0.0`. |

```toml
# autooptimizer.toml
min_realized_return_ratio = 0.25  # at least 25% of returns must be booked profit
```

Both checks are **non-objective risk guards** — they run regardless of which
optimization objective is selected, alongside the built-in drawdown guard
(child drawdown ≤ 1.5× parent). All checks run to completion so the rejection
reason surfaces every failing dimension.
---

## Migration — existing configs with >120-day windows

PR B22 introduced `MAX_WINDOW_DAYS = 120` as a **breaking change** for any
`autooptimizer.toml` that already had `day_window` or
`baseline_untouched_window` set above 120. Those files now fail the
`AutoOptimizerConfig::validate()` call at cycle startup.

**To migrate, choose one of:**

1. **Shrink the window.** Lower `day_window` and/or `baseline_untouched_window`
   to 120 or below. No other change required.

2. **Set `max_window_days` explicitly.** Add `max_window_days = <your window>`
   (at least as large as the largest of the two fields) to acknowledge the
   memory-headroom tradeoff:

   ```toml
   max_window_days = 180   # explicit opt-in to the wider window

   day_window = 180
   baseline_untouched_window = 60
   ```

Either path passes validation. Option 2 is the right choice when the wider
window is intentional and your cycle container has the memory to support it.

---

## Cross-references

- [Optimizer](/docs?slug=optimizer) — cycle overview, DSPy flywheel, holdout discipline.
- [CLI Reference](/docs?slug=cli-reference) — `xvn optimize` flag inventory, exit codes 10–15.

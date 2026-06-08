# Harness Improvements + Strategy QA Queue — Ollama Session 2026-06-07/08

**Source:** Session report "XVN Ollama Strategy Loop" (V8–V15, ~25 evals, Qwen3-4B at 100.90.135.112:11434)  
**Evidence requirement:** every item must include evidence before close — screenshots, terminal output, HTTP responses, or grep results as specified.

---

## Blockers (fix before next session)

### B1 — F9: Daemon restart at ~40–43 min kills long evals
**Observed:** Container restarts at the ~40–43 minute mark of any eval session. Any 15m strategy targeting 50+ decisions crosses this window. Three concurrent Ollama calls accelerate it to ~21 min.  
**Impact:** Long evals silently die; results lost if capture-on-interrupt isn't working.  
**Investigation needed first:**
- What is actually restarting? Run `docker ps --format "{{.Names}} {{.Status}}"` at the 35-min mark during a session to check uptime. Compare with `systemctl status` or Coolify task logs.
- Is this the xvn container (Docker restart policy), the Ollama daemon (memory/timeout), or the host process manager?
**Fix path:**
- If Docker restart policy: set `restart: unless-stopped` with a longer health-check interval, or detach eval runs from the container health path.
- If Ollama: reduce concurrent calls to ≤2 (already documented in Part 4 — enforce it in the eval chain script).
- If host: nothing to fix in xvn, document the window.  
**Evidence required:** Run `docker inspect xvn-app --format "{{.HostConfig.RestartPolicy}}"` and record output. Capture uptime at the point of next restart (`docker ps` output showing container start time vs wall clock).

---

### B2 — F19: Filter DSL does not support nested condition groups
**Observed:** `all containing any` (or vice versa) fails with `missing field lhs`. All conditions inside `any`/`all` must be flat `{lhs, op, rhs}` objects. Prevents combining a hard regime gate (ADX < 25) with a signal group at the filter level.  
**Why it matters:** V9d proved prompt-level regime gates are ignored by Qwen3-4B (~33% of the time). The only reliable regime gate is deterministic (filter). Without nesting, you can't write `adx_14 < 25 AND (mfi_14 < 20 OR mfi_14 > 80)` — the regime check must live in the prompt where the model can override it.  
**Files:** `crates/xvision-engine/src/strategies/` (filter DSL parser), `crates/xvision-cli/src/commands/strategy.rs` (`set-filter` validator)  
**Work:** Add a `group` or `nest` condition type that wraps an inner `any`/`all` block. One level of nesting is sufficient. Update `set-filter --validate` and the filter parser. Add a test: `all[adx_14 < 25, any[mfi_14 < 20, mfi_14 > 80]]` parses and passes validation.  
**Evidence required:** `xvn strategy set-filter <id> --filter '{"type":"all","conditions":[{"lhs":"adx_14","op":"lt","rhs":25},{"type":"any","conditions":[{"lhs":"mfi_14","op":"lt","rhs":20},{"lhs":"mfi_14","op":"gt","rhs":80}]}]}'` returns success. `xvn eval validate` against a strategy using that filter passes.

---

### B3 — F18: `rvol_tod_20` silently fails with short warmup (0 decisions, no error)
**Observed:** V9b produced 0 decisions because `rvol_tod_20` requires 20 × 96 bars (20 days of same-slot history). With the standard 3-day warmup, it can never compute. The eval produces 0 decisions with no warning anywhere.  
**Files:** `crates/xvision-engine/src/eval/` (filter evaluation), `crates/xvision-cli/src/commands/eval.rs` (`xvn eval validate`)  
**Work:** During `xvn eval validate` (and as a pre-launch check), cross-reference each filter indicator's required lookback against the scenario's available bars:
- `rvol_tod_20`: requires `20 × bars_per_day` bars of same-time-of-day history. For 15m: 20 × 96 = 1920 bars minimum.
- General rule: if `indicator_lookback_bars > scenario_total_bars - WARMUP_BARS`, warn with: "Warning: indicator `rvol_tod_20` requires 1920 bars of same-slot history but scenario provides only ~{n} bars — expect 0 decisions."
**Evidence required:** `xvn eval validate` against a strategy with `rvol_tod_20` on a 14-day 15m scenario prints the warning. `xvn eval validate` against the same strategy on a 30-day scenario does NOT print the warning.

---

## Improvements

### I1 — F11/Chain: Eval chain race condition — process group kill + lockfile
**Observed:** `kill <PID>` only killed the shell wrapper, not the background script process. Both original and replacement chains ran concurrently, launching 3 duplicate V8c-bear evals at 00:30–00:33.  
**Fix (shell tooling — no Rust change):** Two options:
1. **Process group kill:** launch chain with `setsid bash /tmp/eval_chain.sh &` and store the PGID (`PGID=$!`). Kill with `kill -- -$PGID`. This kills the entire process group including child processes.
2. **Lockfile:** Add at top of chain script:
   ```bash
   LOCKFILE=/tmp/eval_chain.lock
   exec 200>"$LOCKFILE"
   flock -n 200 || { echo "chain already running"; exit 1; }
   ```
3. **Simplest design (recommended for future sessions):** Single sequential `for` loop in a `nohup` script with an early-exit status file. No polling-wait approach. Each eval waits for the previous to complete via `xvn eval wait` (if it exists) or a polling loop on `xvn eval status`.  
**Evidence required:** Start a chain script with the lockfile guard. Attempt to start a second instance — it must exit immediately with "chain already running." Terminal output showing the rejection.

---

### I2 — wake_when_in_position documentation
**Observed:** The same strategy with different `wake_when_in_position` settings produced wildly different outcomes:
- `on_invalidation`: 153 decisions (73 extra noise injections mid-position) — Sh -8.814
- `never`: 14 decisions, 1 trade (collapsed to almost nothing) — Sh -2.054  
**Fix:** Add to `xvn strategy set-filter --help` (and the `xvision-cli` skill):
```
wake_when_in_position controls whether the trader is called while a position is open:
  never           — no mid-position calls (safest; fewest decisions; recommended for
                    mean-reversion strategies where you want to hold to target)
  on_invalidation — fire when filter condition changes state while in position (use
                    with caution on oscillating indicators like Ichimoku cloud, ORB
                    levels, or any condition that flips frequently; this multiplies
                    decision count significantly and injects noise)
  always          — fire on every bar while in position

Rule of thumb: start with never. Only use on_invalidation if the strategy needs
to exit early when the signal reverses. Never use always.
```  
**Evidence required:** `xvn strategy set-filter --help` output contains the `wake_when_in_position` section with at least `never` and `on_invalidation` documented. Diff of the help text.

---

### I3 — V9c unrealized PnL accounting: mark-to-market at scenario end
**Observed:** V9c crash+recovery posted Sh +0.207 with 0% closed-trade win rate. The strategy entered LONGs at Aug 5 crash (~$50k) and held to scenario end (~$60k). The +0.01% return comes from unrealized open positions, not closed trades. This inflates results for LONG-biased strategies run over crash+recovery scenarios.  
**Fix:** At scenario end, mark all open positions to market (close them at the final bar's close price) and include those PnL values in `metrics_json`. Currently `win_rate` only counts closed trades; the final mark-to-market trade should count too.  
**Files:** `crates/xvision-engine/src/eval/executor/backtest.rs` (scenario end handling), `crates/xvision-engine/src/eval/metrics.rs`  
**Evidence required:** Re-run V9c crash+recovery after fix. `eval_runs.metrics_json` shows a non-null final-position trade. `win_rate` reflects the outcome of that position. Compare Sharpe before/after.

---

### I4 — SKILL update: `xvision-cli` / `xvision-cli-qa` — session learnings
**File:** `.claude/skills/xvision-cli/SKILL.md`, `.claude/skills/xvision-cli-qa/SKILL.md`  
**Updates needed:**
1. **wake_when_in_position** — add the documentation from I2 above.
2. **Eval chain management** — document the process-group-kill pattern and lockfile guard. Add: "Always check for a running chain before starting another — `pgrep -f eval_chain` or check the lockfile."
3. **Indicator warmup gotchas** — add a table of indicators with non-obvious warmup requirements:
   - `rvol_tod_20`: 20 × bars_per_day (for 15m: 1920 bars = ~14 calendar days)
   - `highest_20` / `lowest_20`: 20-bar lookback; may include current bar (test before using in production)
   - `opening_range_high_30` / `opening_range_low_30`: requires market-open alignment; fine for US equity 15m
4. **Qwen3-4B prompt gate reliability note** — add: "Prompt-only regime gates (e.g. 'if adx_14 > 25, output FLAT') are unreliable with Qwen3-4B. The model regularly ignores soft rules when concrete indicator values are present. Regime gating must be in the filter DSL (deterministic) to be reliable. Blocked by F19 until nested filter groups land."
5. **Critical workflow reminder** — add prominently: "After `xvn strategy new`, ALWAYS run `xvn agent set-tools <AGENT_ID> --slot main --tools submit_decision`. Atomic strategy creation does NOT attach tools."  
**Evidence required:** Each bullet point appears in the relevant skill section. Diff shown.

---

## Strategy QA run queue

Six strategies are authored, validated, and ready for first eval. Run in order (earlier strategies inform later ones).

### Active scenarios (reuse these IDs)
| Scenario | ID | Description |
|---|---|---|
| Bear Jun 2024 | `sc_01KTHCWFS4V9S1RJV91W7P4ZW3` | Directional bear; 48–73 decisions for most 15m strategies |
| Crash+recovery Aug 2024 | `sc_01KTHCWFS5WX5HW6XAQJJ8CYBP` | High-vol; LONG-biased strategies get unrealized-gain trap (see I3) |
| ETF bull Jan 2024 | `sc_01KTHCWFRHF40HQYRZM4SVWPCY` | Trending bull; mean reversion fails here |
| Tariff chop Apr 2025 | `sc_01KTHCWFRR5C9AWT87EJXDZBFT` | Low-ADX ranging; exhaustion strategies preferred |
| Rangebound Q2 2025 | (existing from V2 runs) | Rangebound; low-trend |

---

### QA-1: V10 CCI Trend-Break (`01KTJ2B6G5NJEHHSVPHVW45E9J`)
**Method:** CCI > 100 = trend continuation signal.  
**Run on:** Bear Jun 2024 + Rangebound Q2 2025  
**What to look for:** CCI > 100 in a trending bear means continuation down — check if the strategy shorts correctly. In rangebound, CCI should oscillate and potentially over-trade.  
**Success criteria:** ≥10 decisions in bear. Sharpe > 0 in at least one scenario.  
**Evidence required:** `xvn eval results <run_id>` output showing decision count, Sharpe, trade count for each scenario run.

---

### QA-2: V11b OBV Slope (`01KTJ3DPV8PY65V9PTYKF6J5D7`)
**Method:** OBV slope_gt_4 + VWAP confirmation.  
**Run on:** Crash+recovery Aug 2024  
**What to look for:** OBV slope divergence from price during a crash = smart money accumulation signal. Note: check that `highest_20`/`lowest_20` aren't the current-bar-included issue (F7-like). If 0 decisions, may be indicator issue.  
**Success criteria:** ≥5 decisions. Log confirms OBV slope is being evaluated (not silently failing).  
**Evidence required:** `xvn eval results` output + check eval logs for any indicator errors. If 0 decisions, run `xvn eval validate` and confirm filter fires at least once in the scenario bar range.

---

### QA-3: V12 N-Bar Turtle Breakout (`01KTJ4RDD198MNSW95P9JFG0JM`)
**Method:** `highest_20` / `lowest_20` breakout (turtle-style).  
**Run on:** ETF bull Jan 2024 (fire-rate test — bull should favor breakouts)  
**Warning:** `highest_20` and `lowest_20` are valid in filter DSL but **untested at runtime** — possible F7-like issue where current bar is included in the lookback (inflating breakout frequency). Watch decision count carefully.  
**Success criteria:** Decision count is plausible for a breakout strategy in a trending bull (expect 10–40 decisions over the scenario). If ≫100 decisions, suspect current-bar inclusion.  
**Evidence required:** `xvn eval results` output. If decision count seems inflated, compare with a `highest_30` variant.

---

### QA-4: V13 MACD Crossover (`01KTJ4ZV7ZVENEG7SS83N007ZV`)
**Method:** `macd_line crosses_above macd_signal`.  
**Run on:** Crash+recovery Aug 2024  
**What to look for:** MACD crossover in a crash scenario should capture the reversal entries. This is the first crossover-type indicator tested — verify `crosses_above` fires correctly (doesn't double-fire or miss the cross).  
**Success criteria:** ≥3 decisions. At least one decision at a plausible MACD-crossover bar.  
**Evidence required:** `xvn eval results` + `xvn eval decisions <run_id>` showing at least one decision with the bar timestamp visible.

---

### QA-5: V14r MFI Exhaustion (`01KTJ8WQDTZRJVNQQ5X4KH8V3N`)
**Method:** `mfi_14 < 20 OR mfi_14 > 80` (exhaustion extremes).  
**Note:** This is the strategy that was meant to use nested filter groups (F19). If F19 is not yet fixed, this runs without the ADX regime gate (only the MFI condition).  
**Run on:** Tariff chop Apr 2025 (low-ADX ranging — exhaustion should fire here)  
**Success criteria:** ≥10 decisions. If F19 is fixed and ADX gate is added, compare decision count before/after.  
**Evidence required:** `xvn eval results` output. Note whether ADX gate is active or not.

---

### QA-6: V15 Prev-Day Breakout (`01KTJ94HRGHE30CZZJQ5EEBRAJ`)
**Method:** `prev_day_high` / `prev_day_low` breaks.  
**Run on:** ETF bull Jan 2024 (trending — daily levels should matter in a trending market)  
**What to look for:** `prev_day_high`/`prev_day_low` are confirmed valid in filter DSL (V15 validated). First runtime test of these indicators.  
**Success criteria:** ≥5 decisions. Entries appear near the scenario's daily high/low levels.  
**Evidence required:** `xvn eval results` output. If 0 decisions, check `xvn eval validate` to confirm the indicator computes in the scenario.

---

### QA-7 (proposed): V16 BB Squeeze Breakout
**Method:** `bb_width_20 zscore_lt_20 -1.5` — fires when Bollinger Band width is historically narrow (compression = imminent expansion). Direction from MACD/DI context.  
**First meta-volatility signal in the suite** — fires on the setup, not on the extreme itself.  
**Build first:** `xvn strategy new` → add filter with `bb_width_20 zscore_lt_20 -1.5` → `xvn eval validate` → confirm indicator computes.  
**Run on:** Bear Jun 2024 + ETF bull Jan 2024 (volatility compression precedes moves in both directions).  
**Success criteria:** Strategy fires ≤20 times per scenario (compression events are rare). Sharpe > 0 in at least one scenario.  
**Evidence required:** `xvn strategy set-filter` succeeds. `xvn eval validate` passes. First `xvn eval results` output.

---

## Insights to carry forward (no code change needed)

These are documented findings from the session — important context for future strategy design, not bug fixes.

**Mean reversion regime map:**
- Bear / low-trend: V2 LTAC, V3b DVE-BB, V9c volume_zscore all positive or near-flat. Use these scenarios first.
- Bull / trending: mean reversion breaks down. V7 Ichimoku, V8 SORB both negative. Need trend-following strategies.
- Crash+recovery: LONG-biased strategies get unrealized-gain trap (I3). Results there are unreliable until mark-to-market fix.

**Decision count target:** 10–50 decisions per scenario for a 15m strategy is healthy. Above 100 = noisy (see V7b Ichimoku Sh -15). Below 5 = under-traded, hard to evaluate edge.

**Best result so far:** V3b DVE-BB bear +0.29%, Sh 2.078, 4 trades, 100% win rate. This is a daily-timeframe strategy — the 15m equivalent hasn't been built yet. DVE-BB on 15m is a follow-on candidate.

**Ollama concurrency limit:** ≤2 concurrent eval chains. 3+ accelerates daemon restart to ~21 min. Enforce in chain scripts.

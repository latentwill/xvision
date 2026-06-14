# xvn eval test — error documentation & handoff

- **Date:** 2026-06-14
- **Operator goal:** test the eval end-to-end with **Gemini 3.1 Flash Lite** in the strategy, under a **1M-token budget**, with a good filter, aiming for a profitable strategy across multiple scenarios.
- **Environment:** `xvn 0.21.0` inside container `xvn-app` (`$XVN_HOME=/data`, the store the dashboard reads). Provider `openrouter`, model `google/gemini-3.1-flash-lite`. Key passed at runtime via `-e XVN_PROVIDER_OPENROUTER_KEY=…` (1Password Olympus item `rjzgzf2sgkkociqynpaqj632wq`, field `apikey`).
- **Outcome:** harness plumbing works; **trading behavior is buggy** (churn). Strategy lost money on every active scenario and **overspent the token budget 3.3×** (~3.27M vs 1M). Both failures trace to a single root cause (see ERROR-1).

---

## TL;DR status

| Area | Status |
|---|---|
| Strategy authoring / filter attach / diagnostics / validate / batch / export / persistence | ✅ working |
| Filter gating + suppression accounting | ✅ working (0-fire scenarios → 0 calls, accurate) |
| Provider/model binding (Gemini 3.1 Flash Lite) | ✅ working |
| **`wake:never` + ATR-stop position management** | ❌ **BUG — force-closes & re-enters every bar (churn)** |
| Token budget adherence | ❌ ~3.27M vs 1M cap (consequence of the churn bug) |
| Profitability | ❌ negative on all 3 active scenarios; −5.95% vs +123% buy-hold on full-2024 |
| Trader response-schema robustness | ⚠️ 58 schema-patch-repair retries logged (recovered) |
| Scenario regime labels | ⚠️ "Crypto bull Q1-2025" is actually −11.5% (mislabeled) |

---

## Artifacts (all in `/root/xvn-work/eval-test-2026-06-14/`)

| File | What |
|---|---|
| `trader_trend_v3.md` | trader prompt (long-only daily trend rider) |
| `filter_trend_v3.json` | 3-condition uptrend gate, `wake:never`, fire metadata |
| `strategy_01KV3ARXGJDW5R5JNK39Y5PQRV.json` | full Strategy object as persisted (risk_pct 0.2, stop 2×ATR) |
| `batch_result.json` | `eval batch run --json` output |
| `batch_stderr.log` | engine logs incl. schema-repair INFO events |
| `run_summary.json` | per-run NAV/fees/tokens/baselines summary |
| `exports/exp_<run_id>.json` | full `xvn eval export` per run (decisions, equity, filter events) |

**IDs**
- Strategy: `01KV3ARXGJDW5R5JNK39Y5PQRV` (`gemini-flash-trend-rider-v3`), trader agent `01KV3ARXG0BJCDSAGXWB1F79BR`, filter `01KV3AS9EQSHPMTG9JDA07W1FW`
- Batch: `batch_01KV3AVACQWTTJ91H1T4PV69FD`
- Runs: bull-q1 `01KV3AVAKFH7GJNZ2X670H1XJ8` · Q4-24 `01KV3AVFJ642BWFHVEB9NCYY25` · bear-q3 `01KV3AX6PRC07NBWJRCW6THCE6` · **full-24 `01KV3AXAJX5G21DZE0BT1R3YXF`** (canonical repro) · top-corr `01KV3B42FC9064ZFDW05APT3Q3`

---

## Results table

| Scenario | Bars | Filter wakeups | Decisions | Trades | NAV end | Return | Buy-hold | Fees | Real tokens |
|---|---|---|---|---|---|---|---|---|---|
| Crypto bull Q1-2025 *(actually −11.5%)* | 91 | 0 | 0 | 0 | $100,000 | 0% | −11.5% | $0 | 0 |
| BTC daily bull Q4-2024 | 93 | 1 | 37 | 36 | $97,986 | **−2.01%** | +49.1% | $1,792 | 487k |
| Crypto bear Q3-2024 | 93 | 0 | 0 | 0 | $100,000 | 0% | +1.3% | $0 | 0 |
| **BTC daily full-2024** | 367 | 10 | 145 | 134 | $94,047 | **−5.95%** | **+123.4%** | **$6,599** | **2.06M** |
| BTC 2025 top-corr-recovery | 244 | 3 | 53 | 52 | $95,176 | **−4.82%** | +17.0% | $2,543 | 724k |
| **TOTAL** | | | | | | all negative | | | **3.27M** |

In every active run, **fees ≈ the entire loss**. The strategy is not expressing any edge — it is churning.

---

## ERROR-1 (PRIMARY, P0) — `wake:never` + ATR stop liquidates & re-enters every bar

**Symptom.** A long-only trend strategy configured to enter once and hold (`wake_when_in_position: "never"`, `stop_loss_atr_multiple: 2.0`) instead opens a position and gets force-closed almost every single bar, then immediately re-opens.

**Evidence (run `01KV3AXAJX5G21DZE0BT1R3YXF`, full-2024):**
- 67 `long_open` and 67 `stop_loss` rows, perfectly alternating.
- **65 of 67 round-trips held exactly 1 day** (median hold = 1 bar; max = 2).
- Exits are labeled `stop_loss` (`"justification": "sltp: stop_loss"`) **even when the exit price is above entry at a profit** — a 2×ATR stop on a long cannot trigger on an up-bar. Example consecutive rows:
  ```
  ix0 2024-02-19 long_open  fill 51805.58  pnl_realized -50.00   (entry fee)
  ix1 2024-02-20 stop_loss  fill 52267.30  pnl_realized +127.81  (exit ABOVE entry, +profit, labeled stop_loss)
  ```
- Round-trip P&L is a coin-flip (27 wins / 40 losses, ≈ −$2.6k on price) but **fees were $6,599 = 6.6% of NAV**, which is essentially the entire −5.95% loss.
- Same pattern in the other active runs (Q4-24: 18/18; top-corr: 26/26).

**Why this is a bug, not config.** `xvn strategy set-filter --help` documents `wake:never` as: *"no mid-position calls; exits rely entirely on the deterministic risk gate (`risk.stop_loss_atr_multiple`)… Produces the fewest decisions."* Observed behavior is the opposite — the **most** decisions and 1-bar holds. The sltp gate is firing **unconditionally every in-position bar** rather than only when price breaches entry − 2×ATR. After each forced close, the filter regime is still true, so the gate re-fires and re-enters → churn loop.

**Severity.** P0. It (a) makes any hold-to-target / trend strategy structurally unprofitable via fee churn, and (b) is the direct cause of the token overspend (ERROR-2). On full-2024 it converted a **+123% buy-hold market into a −6% loss**.

**Where to look (engine).**
- The backtest sltp/stop evaluation path (prior memory cited `crates/**/backtest.rs` and issue #932). Check how the stop level is computed/stored per open position and whether the in-position bar evaluation closes unconditionally when `wake:never` suppresses a fresh trader call.
- Suspect: with `wake:never` the per-bar risk-gate check has no live stop level (or computes a zero/near-zero distance) and closes every bar. Confirm by logging the stop price vs bar low for each in-position bar.

**Regression note.** This is the **inverse** of the older finding (issue #932, ~2026-06-12): then, sltp was *skipped* on filter-gated bars so positions rode −22% past a 2×ATR stop. Now sltp fires *every* gated bar. The behavior flipped from "never closes" to "closes every bar" — strongly suggests a fix for #932 overcorrected. Re-test #932's repro against 0.21.0.

**How to reproduce:** see "Reproduction" below; canonical run is `01KV3AXAJX5G21DZE0BT1R3YXF`.

---

## ERROR-2 (P1) — Token budget overspend (3.27M vs 1M cap)

**Symptom.** The 5-scenario batch consumed ~3.27M real tokens (input+output), 3.3× the 1M operator cap.

**Breakdown (actual_input + actual_output from `eval export`):**
- full-2024: 2,063,593 · top-corr: 724,094 · Q4-24: 486,616 · bull-q1: 0 · bear-q3: 0.

**Root cause.** Direct consequence of ERROR-1. The churn produced 67 / 26 / ~18 LLM trader calls per run instead of the expected ~1–2 (each daily call ≈ 27–30k input tokens). With the churn bug fixed, an enter-once-and-hold run on a 90-bar window is ~1–2 calls ≈ ~50k tokens.

**Process lesson.** Launching a 5-scenario batch that included a **367-bar full-year** scenario before gauging per-bar call cost on one short window was the operator mistake that let the bug burn the whole budget. Next time: run one short scenario, read `filter_summaries.wakeups` + `actual_input_tokens`, then scale.

---

## ERROR-3 (P2) — Trader response-schema patch-repair on most calls

**Symptom.** `batch_stderr.log` contains **58** `trader_output_schema_patch_recovered` INFO events (`event="trader_output_schema_patch_recovered" class_tag="invalid_field"`). Gemini 3.1 Flash Lite repeatedly emits a field the trader output schema rejects; the engine repairs it on retry 1.

**Impact.** Recovers (no hard failure; `errors[]` is empty in exports), but every affected call pays an extra retry — added latency and token cost. This is the known `ResponseSchema::trader_output()` `additionalProperties:false` friction (prior memory: issue #933 — schema exposes only `action`/`conviction`/`justification`).

**Fix direction.** Widen the trader output schema to accept (and use) the bracket/management fields the parser already supports, or set the field as optional rather than rejected, so the model's first emission validates.

---

## ERROR-4 (P3, data hygiene) — Scenario regime mislabeling

**Symptom.** `crypto-bull-q1-2025` ("Crypto bull — Q1 2025") has a **buy_hold return of −11.5%** in its own metrics baseline — it is a correction window, not a bull. The gate correctly produced 0 trades. Consistent with the prior note that `crypto-rangebound-q2-2025` was also mislabeled.

**Impact.** Misleads regime-specific testing. **Always check `metrics.baselines.buy_hold.return_pct` before trusting a scenario's display name.**

---

## Non-issues / confirmed-correct behavior

- **Filter gating works.** bear-q3 (no uptrend) and bull-q1 (correction) both fired 0× → 0 tokens. `filter_summaries` (`bars_scanned`, `wakeups`, `suppressed_*`, `llm_calls_saved`, `estimated_tokens_saved`) is accurate.
- **`metrics.total_return_pct` matched NAV return on full-2024** (−5.953% ⇒ $100,000→$94,047). (Older memory warned these can diverge — verify per run via `equity_samples`; they agreed here.)
- **Baselines are populated and useful** (`buy_hold`, `always_flat`, `simple_trend`, `simple_mean_reversion`) — the −5.95% vs +123% buy-hold gap is the cleanest statement of the bug's cost.
- **Short-window warmup caveat:** on a 90-bar daily window, `sma_50` needs ~50 warmup bars; if pre-window bars aren't fetched, ~half the window is un-gateable. Did not isolate this here (the 0-fires were explained by regime), but worth confirming whether the engine fetches warmup bars before the scenario window.

---

## Reproduction

```bash
# key
source /root/.op_env
ORK=$(op item get rjzgzf2sgkkociqynpaqj632wq --vault Olympus --fields label=apikey --reveal)
SID=01KV3ARXGJDW5R5JNK39Y5PQRV     # already persisted in /data/strategies

# (re)author from the saved object if needed:
# docker cp strategy_01KV3ARXGJDW5R5JNK39Y5PQRV.json xvn-app:/tmp/s.json
# docker exec xvn-app xvn strategy new --from-file /tmp/s.json

# minimal repro of ERROR-1 (1 short scenario; ~50–500k tokens depending on churn):
docker exec -e XVN_PROVIDER_OPENROUTER_KEY="$ORK" xvn-app \
  xvn eval run --strategy $SID --scenario sc_01KTX8TVZJH1EP3TYY2K20HBY5 --mode backtest

# inspect: hold durations should be ~1 bar, exits labeled stop_loss on up-bars
docker exec xvn-app xvn eval export <run_id> > exp.json
python3 - <<'PY'
import json; d=json.load(open('exp.json'))
seq=[x for x in d['decisions'] if x['action'] in('long_open','stop_loss')]
print('open/stop pairs, hold days, pnl signs:')
for a,b in zip(seq[::2],seq[1::2]):
    print(a['action'],a['ts'],round(a['fill_price']),'->',b['action'],b['ts'],round(b['fill_price']),'pnl',round(b.get('pnl_realized',0)))
PY
```

**Strategy config that triggers it:** `risk.stop_loss_atr_multiple = 2.0`, `risk.risk_pct_per_trade = 0.2`, filter `wake_when_in_position = "never"`, gate = `close>sma_50 AND ema_12>ema_26 AND adx_14>20`.

---

## Recommended next steps (for whoever picks this up)

1. **Fix ERROR-1 first** — it is the blocker. Confirm whether the per-bar risk gate has a valid stored stop level when `wake:never`; it should only close on `bar_low <= entry − 2×ATR` (long). Add a regression test: enter long, feed up-bars, assert position is held (no `stop_loss`) until a real 2×ATR breach.
2. **Re-test issue #932** against 0.21.0 — behavior flipped; the fix may have overcorrected.
3. **Fix ERROR-3** — widen trader output schema so first emission validates (kills 58 retries/batch and unblocks bracket-based exits).
4. After fixes, **re-run within budget**: one 90-bar scenario first to confirm ~1–2 calls / ~50k tokens, then scale to the multi-scenario set. Expect enter-once-and-hold to track (not beat) buy-hold in bull windows and stay flat in bear/chop.
5. **Add a post-stop re-entry lockout** (cooldown after a stop-out) regardless of ERROR-1, so a legitimate stop doesn't instantly re-arm into the same chop.
6. **Don't trust scenario names** — gate regime tests on `metrics.baselines.buy_hold`.

## Draft GitHub issue (ready to file, not filed)

> **Title:** `wake:never` + ATR stop force-closes and re-enters every bar (1-bar churn, fee bleed)
>
> **Body:** With `filter.wake_when_in_position = "never"` and `risk.stop_loss_atr_multiple = 2.0`, a long-only daily strategy is liquidated and re-opened almost every bar via the sltp path. Repro: strategy `01KV3ARXGJDW5R5JNK39Y5PQRV`, run `01KV3AXAJX5G21DZE0BT1R3YXF` (xvn 0.21.0). 65/67 round-trips held exactly 1 bar; exits labeled `stop_loss` fire on up-bars at a profit (`ix0 long_open 51805 → ix1 stop_loss 52267 pnl +127`), so the gate is closing unconditionally, not at entry−2×ATR. Result: 134 trades, $6,599 fees = the entire −5.95% loss vs +123.4% buy-hold. Documented `wake:never` behavior ("fewest decisions, hold to the risk gate") is inverted. This is the **inverse of #932** (sltp previously *skipped* on gated bars) — likely an overcorrected fix. Also blew a 1M-token eval budget to 3.27M via the churn. See `/root/xvn-work/eval-test-2026-06-14/` for full exports + repro script.

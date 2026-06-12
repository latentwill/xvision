> Operator findings from the 2026-06-12 strategy-eval rounds (filed as issues #932–#940; capability wave #941–#945).

# xvn Platform Findings — wiki-strategy round, 2026-06-12

For the next coding-agent round. Environment: `xvn 0.21.0` in `xvn-app` container
(image `xvision:deploy-latest`), backtest evals via openrouter (gemini-3.1-flash-lite,
deepseek-v4-flash). Run IDs referenced are in this container's store.

## High

1. **Token/model-call accounting split between eval layer and agent-run layer (trust gap).**
   `xvn run inspect <run_id>` materializes `xvn_run.json` with `status=running`,
   `model_calls=0`, `input_tokens=0` for COMPLETED backtest runs that demonstrably billed
   tokens (e.g. run `01KTX9NVM4G19FE7YEK6KTZQDK`: eval layer shows `actual_input_tokens=46473`,
   sidecar shows 0 calls; same on `01KTX9QKAQEC83KNVJJRVTDV5P` and `01KTX9GMYC8Q2BJ2NWDF4D2JEF`).
   Engine-internal model calls bypass the sidecar accounting entirely. Operators cannot trace
   which layer made calls; `run inspect` is actively misleading for backtests.
   Repro: `xvn run inspect <any completed backtest run_id>` then compare to `xvn eval results <id> --json`.

2. **`total_return_pct` semantics inconsistent with equity curve.** Run `01KTX9QKAQEC83KNVJJRVTDV5P`:
   `metrics.total_return_pct=22.04` but equity samples go $100,000 → $110,992.85 (+10.99% NAV).
   Appears to be return on capital-at-risk (20% allocation) rather than portfolio return; either
   way it is not documented, and operators comparing to `buy_hold` baseline (which IS portfolio-pct)
   are comparing apples to oranges. Repro: `xvn eval export 01KTX9QKAQEC83KNVJJRVTDV5P` and compare
   `run.metrics.total_return_pct` vs `equity_samples`.

3. **`decision_bars: 0` on scenario card after a completed run** (and before any run).
   `xvn scenario inspect sc_01KTX8TVZJH1EP3TYY2K20HBY5 --card` shows `decision_bars: 0` despite a
   completed 93-bar run (`previous_runs.count=1`, `best_return_pct` updates correctly). Known
   display quirk from earlier rounds — still unfixed in 0.21.0.

## Medium

4. **Provider key-resolution gap (long-standing).** `provider check`/`list` read
   `/data/secrets/providers.toml`, but `eval run`/`optimize run-cycle` resolve keys from
   `XVN_PROVIDER_<NAME>_KEY` env only. Fresh `docker exec` sessions fail with "no API key" while
   the same container's secrets file holds a valid key. Operators maintain an awk keybridge
   script. Fix: run-path should fall back to the secrets file like the check-path.

5. **Token counts not surfaced in human-readable output.** `eval show --verbose` prints
   `input_tokens n/a` / `output_tokens n/a` even though `actual_input_tokens`/`actual_output_tokens`
   are persisted (visible in `eval results --json` / `eval export`). The verbose path reads
   `report.input_tokens` (null) instead of `run.actual_*`. One-line fix candidate.

6. **No filter-fire count or selectivity summary in `eval show`/`eval results`.** Fire data exists
   (`eval export` → `filter_summaries`: `llm_calls_saved`, bars scanned) but the primary verbs don't
   show fires/total_bars/fire-rate, which is THE key diagnostic for filter-gated strategies.
   Also: no synthesized-row split (`noop_skip` etc.) anywhere in results output.

7. **`cost_estimate_complete=false`, `cost_usd_estimate=null` always** (all 6 runs this round),
   despite known models and exact token counts. No CLI-visible LLM spend tracking.

8. **Scenario regime label vs reality.** `crypto-rangebound-q2-2025` is named/tagged range-bound
   but its window (2025-04-01..2025-06-01) has buy-hold +26.6% (Sharpe 3.3) — a recovery trend.
   Mean-reversion strategies "tested in range regime" against it get ~0 fires and meaningless
   results. Suggest: `scenario classify` regime labels shown on the card next to operator labels,
   and a warning when a strategy's target-regime conflicts with classified regime at `eval validate`.

9. **`n_trades` counts end-of-scenario liquidation as a trade.** `n_trades=2` with
   `action_counts.long_open=1, long_close=0` (runs `01KTX9GM…`, `01KTX9QK…`). Undocumented;
   confuses trades vs decisions vs actions reconciliation.

## Low

10. **Response-envelope inconsistency:** `strategy new --prompt --json` returns
    `{strategy_id, agent_id, eval_ready, …}`; `strategy new --from-file --json` returns
    `{id, strategy:{…}}`. Scripts must handle two shapes for one logical operation.
11. **`bars ls` has no `--asset`/`--timeframe` filters** — always dumps the full multi-hundred-line
    catalog.
12. **Filter `fire.reason` not propagated into `filter_events`** — triggered events show
    `reason: None` even when fire metadata was installed (run `01KTX9QK…`).
13. **Filter `status` stays `draft` after `set-filter`** with no promote verb and no doc on whether
    draft≡active at eval time (eval does honor it).
14. **`eval run --profile smoke` default model is stale** (`google/gemini-flash-1.5` no longer in
    the openrouter catalog) — would fail preflight if used.
15. **`provider models --name deepseek` hard-errors until a manual `refresh-models`** — consider
    auto-refresh-on-miss or a softer hint.
16. **No CLI setter for `risk.risk_pct_per_trade`** — sizing changes require
    read-JSON → python edit → `strategy new --from-file`. A `strategy set-risk` verb (or
    `--risk-pct` on atomic create) removes the sharpest remaining edge in strategy authoring.

## Positives (keep)

- Filter DSL accepted every token/operator used across 6 filters first-try (`between`,
  `crossed_above_1`, `prev_day_high`, `opening_range_high_30`, `rvol_tod_20`, Donchian, `bb_pct_b_20`).
- Filter gating works as designed and is the platform's economic core: run `01KTX9QK…` scanned 547
  bars → 2 LLM calls (`llm_calls_saved=545`, ~27M tokens saved).
- Atomic `strategy new` → `--from-file` risk edit → `set-filter` → `diagnostics` → `eval validate`
  → `eval run` pipeline executed cleanly 6/6 times, no retries.
- `scenario create` (asset-free, granularity 1m–1d) is fast and ergonomic.
- Eval baselines block (`buy_hold`, `simple_trend`, `simple_mean_reversion`, `random`, `always_flat`)
  is exactly the right ablation surface — extend it with "always-enter-on-filter-fire" (no-LLM arm)
  to directly measure LLM value-add per the research recommendation.

## Operational note (not a platform bug)

- `/tmp/xvnk.sh` keybridge does not survive container recreation and was observed 0-byte once
  (likely concurrent recreation by parallel agents). Root fix is finding #4.

---

# Round 2 additions (take-profit round, 2026-06-12 ~07:30Z)

## Blocker

17. **SL/TP (and the configured protective stop) are never evaluated on filter-gated bars.**
    In `crates/xvision-engine/src/eval/executor/backtest.rs`, the per-asset body does
    `if filter_gated { continue 'asset; }` (~line 1090) BEFORE the "Advanced SL/TP check —
    fires before the LLM pipeline" block (~line 1227). Filter-inactive bars — which for
    `wake_when_in_position: never` is EVERY in-position bar — therefore never run
    `sltp::check_and_update`. Consequences observed across 8 runs:
    - Zero SL/TP exits ever; every position rides to end-of-scenario liquidation (the
      unrealized-PnL problem).
    - The R1 fallback (`risk.stop_loss_atr_multiple`, set to 2.0 on all strategies) is dead
      code in filter-gated runs: run 01KTXB2TYE3VT76S17V5ESDJDX entered BTC 2024-03-09 and
      held through a −22% adverse move with a 2×ATR stop configured. Capital protection is
      OFF exactly when the strategy is designed to sleep.
    Fix: hoist the SL/TP check above the filter gate — it is position-scoped, not
    decision-scoped, and sltp.rs's own doc comment says capital protection trumps efficiency.

## High

18. **Trader structured-output schema forbids bracket fields.**
    `ResponseSchema::trader_output()` in `crates/xvision-engine/src/agent/llm.rs` (~line 415)
    sets `additionalProperties: false` with only `action`/`conviction`/`justification`, and the
    cline no-decision repair prompt likewise asks only for those three. Meanwhile
    `trader_output.rs` parses optional `stop_loss_pct`, `take_profit_pct`, `trailing_stop_pct`,
    `breakeven_*`, `tp1_*`, `max_bars_held`, and `backtest.rs` would honor them on entry.
    The models can never emit what the parser supports — prompt-level TP/SL instructions are
    silently ignored. Fix: add the bracket fields to the schema (optional) and document them
    in the engine's decision-format instruction.

## Medium

19. **Emitted SL/TP values are not persisted anywhere.** `eval_decisions` (SQLite) has no
    bracket columns and `eval export` decision rows carry none, so even when bracket support
    works there is no way to audit what the model chose.
20. **No strategy-level take-profit config.** `risk` has `stop_loss_atr_multiple` but no
    `take_profit_*` analog, so a deterministic realized-PnL eval structure cannot be expressed
    in config at all today; it depends entirely on (currently unreachable) model-emitted TP.

---

# GitHub handoff (filed 2026-06-12)

| Issue | Maps to findings | Severity |
|---|---|---|
| latentwill/xvision#932 | #17 SL/TP skipped on filter-gated bars | blocker |
| #933 | #18 response schema forbids bracket fields | high |
| #934 | #19 brackets not persisted | high |
| #935 | #2 total_return_pct units | high |
| #936 | #1 run-inspect accounting split | high |
| #937 | #4 provider key env-only resolution | medium |
| #938 | #3,#5,#6,#7,#9 eval observability bundle | medium |
| #939 | #8 scenario regime labels | medium |
| #940 | #10–#16 CLI ergonomics bundle | low |

Fix order for the realized-PnL goal: 932 → 933 → 934 (+935 same wave). Rerun matrix: the four
-tp strategies × their 8 scenarios, same caps, as the realized-PnL baseline.

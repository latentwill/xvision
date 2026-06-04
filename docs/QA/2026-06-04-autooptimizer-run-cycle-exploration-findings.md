# AutoOptimizer `run-cycle` Exploration — Findings

**Date:** 2026-06-04
**Operator:** edward (via agent)
**Deploy under test:** `xvision:deploy-latest`, container `xvn-app` (image built 2026-06-04T03:52Z, `xvn 0.21.0`, `XVN_HOME=/data`)
**Source synced to:** `origin/main` @ `9ebcdb39` (T8) — this deploy includes Night Watch optimizer fixes T1/T2/T3
**Goal:** Exploratory attempt to create an autooptimizer run (`xvn optimizer run-cycle`) using the "gemini long v3" strategy.

---

## TL;DR

- The strategy "gemini long v3" resolves to **`gemini_long_gate_v3_compact`** (`01KT20AS9674W1THXPWR93C1GX`, BTC/USD, 60m, filter-gated, trader bound to `openrouter / google/gemini-3.1-flash-lite`).
- The previously-blocking optimizer issues (T1 `$XVN_HOME` paths, T2 configurable mutator/judge provider, T3 provider-key secrets fallback) **are fixed in this deploy** — `--provider/--model/--config/--db/--strategy` flags are all present and the preflight gate (`require_launchable_provider`) works.
- **A real `run-cycle` still cannot complete.** It now fails for a *new* reason: the candidate's paper-test backtest fails at **decision 0** with `trader_output[missing_response]` (`input_tokens=0, output_tokens=0`).
- **Root cause (verified):** the autooptimizer paper-test adapter invokes the eval executor with an **empty `agent_slots` slice (`&[]`)**, so the candidate strategy's trader has no resolved system-prompt/model binding. Every decision is a no-op failure. This is strategy-independent.
- Net: "no successful optimizer run" is still true on `main`, but the blocker has moved from config/provider wiring (fixed) into the **paper-test adapter slot resolution** (this finding, F1).

---

## What was done (repro trail)

```bash
# 0. Synced local source: stashed dirty codex branch edits, ff-merged main 970433bb → 9ebcdb39 (402 commits).

# 1. Identified strategy by manifest display_name (xvn strategy ls shows IDs only — see F5)
docker exec xvn-app xvn strategy show 01KT20AS9674W1THXPWR93C1GX        # gemini_long_gate_v3_compact

# 2. Safe-launch preflight — all green
docker exec xvn-app xvn provider list                                    # openrouter ● set, deepseek ● set
docker exec xvn-app xvn strategy diagnostics 01KT20AS9674W1THXPWR93C1GX  # launchable: true, trader optimizable

# 3. Mock cycle — succeeds end-to-end (but see F4: not persisted)
docker exec xvn-app xvn optimizer run-cycle --strategy 01KT20AS...3C1GX --mock
#   → cycle_started → parent_selected → honesty_check_run(passed) → cycle_id, exit 0

# 4. REAL cycle — fetches bars OK, then fails at decision 0  ← THE FAILURE
docker exec xvn-app xvn optimizer run-cycle \
  --strategy 01KT20AS9674W1THXPWR93C1GX \
  --provider openrouter --model google/gemini-3.1-flash-lite --budget 3
#   → cycle_started → parent_selected
#   → WARN bar series gap detected (minor, 1–2 bars) ×2
#   → run_cycle: run 01KT8FPG8FKAB77SXCNAA8DFTH decision 0:
#        trader_output[missing_response]: trader pipeline returned no trader response slot
#        (stop_reason=none, input_tokens=0, output_tokens=0, raw_excerpt="<no_response>")
#   → exit 5

# 5. Isolation: same strategy via NORMAL eval works fine (trader responds, trades)
docker exec xvn-app xvn eval run --strategy 01KT20AS...3C1GX \
  --scenario sc_01KT4GSR7PTMZQDNWK88BN6SVC --mode backtest
#   → completed, 5 decisions, 2 trades, return -0.04%, sharpe -3.545, exit 0
```

The isolation step is decisive: the **parent strategy's trader works** through the normal eval path, so the trader/provider/model are fine. The failure is **specific to the optimizer's paper-test path**.

---

## Findings (severity-ranked)

### F1 — BLOCKER: paper-test adapter calls the executor with empty `agent_slots`, so every candidate trader is a no-op

The optimizer paper-test adapters invoke the backtest executor with a hardcoded empty slot slice:

- `crates/xvision-engine/src/autooptimizer/eval_adapter.rs:80` — `BacktestPaperTester::run` → `executor.run(&mut run, strategy, scenario, &[], dispatch, tools, &store)`
- `crates/xvision-engine/src/autooptimizer/eval_adapter.rs:132` — `CachedBacktestPaperTester::run` (the production CLI/dashboard adapter) → same `&[]`.

The executor resolves **everything about the trader** from that slice:

- `crates/xvision-engine/src/eval/executor/backtest.rs:3585` — `trader_repair_context`: `agent_slots.iter().find(|r| canonical_role(&r.role) == "trader")`
- `…:3605` — `trader_model_id`: same `.find(...)`, returns `None` when no trader slot
- `…:4112` — `resolve_inputs_policy`: same `.find(...)`
- `…:4121` — `resolve_bar_history_limit`: same `.find(...)`

With `agent_slots == &[]`, all of these return `None`: **no system prompt, no model id, no inputs policy**. The trader stage therefore has nothing to dispatch, returns `<no_response>` with **0 input / 0 output tokens**, and the run dies at decision 0 (`TraderMissingResponse`, wire tag `n`, `trader_output.rs`).

**Contrast — the normal eval path does it correctly:** `crates/xvision-engine/src/api/eval.rs:1332` builds `runtime_slots(strategy, agent_slots)`, converts via `agent_slot_to_llm_slot` (`:1889`), and passes the resolved slots into the executor: `crates/xvision-engine/src/api/eval.rs:2776` → `.run(..., &agent_slots, ...)`.

**Impact:** No real (`--mock`-free) `xvn optimizer run-cycle` can complete, for *any* strategy. The `--mock` path hides this because `StubPaperTester` returns fixed metrics without ever invoking the executor.

**Fix direction:** the paper-test adapter must resolve the candidate strategy's agent slots the same way `api/eval.rs` does (`runtime_slots(strategy, …)` → `agent_slot_to_llm_slot`) and pass them into `executor.run(...)` instead of `&[]`. The mutated child must also have its trader slot (prompt/model) resolvable at this point — verify the mutator carries the slot binding into the child `Strategy` the adapter receives.

**Test gap:** there is no integration test that runs a non-mock `run-cycle` (or `BacktestPaperTester::run`) against a real filter-gated strategy and asserts ≥1 trader decision actually dispatched (>0 tokens). The `&[]` would have been caught by one.

---

### F2 — HIGH: `--budget <USD>` is a silent no-op

`crates/xvision-cli/src/commands/autooptimizer.rs` validates (`:888` `validate_budget_usd`) and echoes (`:1085` `eprintln!("budget: {b} USD")`) the budget, but **never passes it to `run_cycle`**. `run_cycle` (`crates/xvision-engine/src/autooptimizer/cycle.rs:68`) has no budget parameter at all. Operators get a false sense of cost control: a cycle that fans out parents × mutations × 2 full-window backtests is unbounded by this flag.

**Fix:** either wire `--budget` into a real token/cost ceiling in `run_cycle` (and the paper tester), or remove the flag until it's enforced. At minimum, don't print "budget: N USD" for an ignored value.

---

### F3 — HIGH: default paper-test windows are very large and partly uncached

The day-window scenario defaults to **2024-01-01 → 2025-09-01** (~20 months of BTC 1h ≈ 14.5k bars) and baseline to **2025-09-01 → 2025-12-01** (`AutoOptimizerConfig::default`, `crates/xvision-engine/src/autooptimizer/config.rs:71-83`). Each candidate is backtested on *both*. The container's bars cache only holds fragmented BTC/USD 1h ranges (2024-01→04, 2025-01→04, plus 1-week slices), so a real cycle silently fetches ~16k bars from Alpaca per run. With `--budget` non-functional (F2), there is no guard rail. Once F1 is fixed this becomes the dominant cost/latency surprise.

**Fix:** expose `run-cycle` flags for the evaluation window / a `--scenario` override (mirroring `eval run`), and/or ship a smaller default window. Document that the default cycle pulls ~20 months of bars.

---

### F4 — MEDIUM: `--mock` cycle reports success but persists nothing

`xvn optimizer run-cycle --mock` returns a `cycle_id` and exit 0, but `xvn optimizer ls` then reports **"no optimizer runs"** and `xvn optimizer inspect <cycle_id>` returns an empty row. A smoke test that "passes" but leaves no ledger trace is misleading — operators can't tell mock success from a no-op.

**Fix:** either persist mock cycles to the ledger (flagged as mock) or have `run-cycle --mock` state explicitly that nothing is recorded.

---

### F5 — MEDIUM: `xvn strategy ls` prints IDs only — no way to find a strategy by name

`xvn strategy ls` (and `--json`) emit a bare list of IDs/handles with **no display names**. Finding "gemini long v3" required reading every `$XVN_HOME/strategies/*.json` manifest for `display_name`. (It also produced a near-miss: the ULID `01KT3HHN7PBRVWQ7**V3**CZ...` literally contains "v3", so a naive `grep v3` matches the wrong strategy.)

**Fix:** add a name column to `xvn strategy ls` (and `display_name` to `--json` rows), or a `--filter <substr>` that matches against `display_name`.

---

### F6 — LOW: `xvn run inspect` looks for the DB at the wrong path

`xvn run inspect <run_id>` failed with `sqlite database not found at /data/data/store.db (set --db or XVN_HOME)`. With `XVN_HOME=/data`, the real ledger is `/data/xvn.db` — the verb both **doubles the `data/` segment** and uses the wrong filename (`store.db` vs `xvn.db`). This made it impossible to inspect the failed optimizer backtest run via the documented verb. Possibly the same class of `$XVN_HOME` path bug T1 fixed for the optimizer, not yet applied to `run inspect`.

**Fix:** resolve the run-inspect DB path through the same `resolve_xvn_home` + `xvn.db` logic the rest of the CLI uses.

---

### F7 — LOW: stale `~/.xvn` default in `AutoOptimizerConfig::default_path()`

`crates/xvision-engine/src/autooptimizer/config.rs:106` still returns `home.join(".xvn/autooptimizer.toml")`. The CLI overrides this with `$XVN_HOME/autooptimizer.toml` (the T1 fix lives at the CLI layer), so it's currently harmless, but it's a latent landmine for any future caller that uses `default_path()` directly. Align it with `$XVN_HOME` or delete it.

---

## What works well (positives)

- T1/T2/T3 are genuinely in this deploy: `--provider/--model/--config/--db/--strategy` flags present; `require_launchable_provider` fails fast and clearly when the mutator/judge provider isn't registered (no more deep "missing API key" surprise).
- `strategy diagnostics` + `provider list` give a clean, trustworthy preflight; the strategy reported `launchable: true` and trader `optimizable`.
- The bars/Alpaca fetch path is solid — it pulled the full ~20-month day window with only two 1–2-bar gaps (logged as warnings, not fatal).
- The mutator/judge dispatch wiring (single dispatch shared across mutator+judge+paper-tester) is clean; `--provider/--model` correctly flows to all three.
- Parent-strategy normal eval is healthy end-to-end on the cheap model.

---

## Recommended fix order

1. **F1** (blocker — without it no cycle completes) → add the missing-slot integration test alongside.
2. **F3 + F2** (make a real cycle affordable and bounded) — ship together.
3. **F4, F5, F6, F7** (operator ergonomics / latent path bugs).

## Artifacts

- Real-run log: `/root/xvn-work/night-watch/optrun-geminilongv3-045354.log`
- Failed optimizer backtest run id: `01KT8FPG8FKAB77SXCNAA8DFTH` (decision-0 `missing_response`)
- Mock cycle id (unpersisted): `01KT8FJN13MBX6H6X9SAK1KBR9`
- Isolation eval run id (healthy): `01KT8FQBE2JZJWG9KW7WBWZKXB`

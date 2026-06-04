# AutoOptimizer Run Verification, Harness Parity & UI Visibility

**Date:** 2026-06-04
**Deploy:** `xvision:deploy-latest` (image built 06:33Z, includes commit `44015624` "fix(optimizer): resolve run-cycle exploration findings F1-F7")
**Strategy under test:** `gemini_long_gate_v3_compact` (`01KT20AS9674W1THXPWR93C1GX`) — BTC/USD 60m, filter-gated, trader on `openrouter/google/gemini-3.1-flash-lite`
**Follow-on to:** `docs/QA/2026-06-04-autooptimizer-run-cycle-exploration-findings.md` (F1–F7, all shipped).

This doc has three actionable findings to ship (**F8, F9, F10**), a verification result, and two minor items. Each fix lists problem → evidence → acceptance criteria → approach + file map, in the same form that drove the F1–F7 fix.

---

## Verification: the F1 fix works — the optimizer runs end-to-end

A real (non-`--mock`) `xvn optimizer run-cycle` completed for the first time:

```bash
xvn optimizer run-cycle --strategy 01KT20AS9674W1THXPWR93C1GX \
  --provider openrouter --model google/gemini-3.1-flash-lite \
  --day-start 2024-01-01 --day-end 2024-02-01 \
  --baseline-start 2025-01-01 --baseline-end 2025-02-01 --budget 3
# cycle_started → parent_selected → mutation_proposed → mutation_gated(passed:false)
# → honesty_check_run(passed:true) → cycle_id=01KT8NSKE41J0S4MWZZPX9H7QQ   (exit 0, ~9 min, under $3 cap)
```

The candidate was correctly rejected (`gate_verdict: rejected — today's score improved by 0.000000 but threshold is 0.050000`). The pre-fix failure (every decision `trader_output[missing_response]`, 0 tokens, die at decision 0) is gone — candidate backtests ran real trader decisions and filled real orders.

The cycle's real evaluation runs all sized and filled normally:

| run_id | role | opens | fills |
|---|---|---|---|
| `01KT8NV2EV…` | real eval (parent) | 2 | 3 ✅ |
| `01KT8NXSCC…` | real eval | 3 | 5 ✅ |
| `01KT8NZ2ZF…` | real eval | 8 | 11 ✅ |
| `01KT8P1P3G…` | real eval | 3 | 5 ✅ |
| `01KT8P4HA2…` | real eval | 5 | 3 ✅ |
| `01KT8P32V2…` | **canary** | 44 | **0** (see F9) |
| `01KT8P7H66…` | **canary** | 81 | **0** (see F9) |

---

## F8 — [FIX] CLI optimizer cycles must appear in the dashboard optimizer panel as historic runs, with full data

**Severity:** High. **Goal:** after any `xvn optimizer run-cycle` (CLI or dashboard), the cycle shows in the optimizer panel as a historic run with all collected data — lineage/genealogy, gate verdict, diff, provenance, and the per-candidate backtest metrics.

**Problem — store split.** The CLI and dashboard use different lineage databases:

| Surface | Lineage store |
|---|---|
| Dashboard launch **and** read | `state.pool` = `$XVN_HOME/xvn.db` (`autooptimizer_cycle.rs:83-84` write; `autooptimizer.rs:95-149` read) |
| CLI `xvn optimizer run-cycle` | `$XVN_HOME/lineage/lineage.db` (the `--db` default in `commands/autooptimizer.rs`) |

So a CLI cycle writes `lineage_nodes`/`mutator_attribution` into `lineage/lineage.db`, but the dashboard only ever queries `xvn.db` — which currently has no `lineage_nodes` table at all (only `agent_lineage`). Result, observed live:

```
GET /api/autooptimizer          → {"items":[],"total":0}
GET /api/autooptimizer/lineage  → []
```

The dashboard route guards on `table_exists(pool, "lineage_nodes")` (`autooptimizer.rs:96,169,201`) and silently returns empty when the table is absent — so the panel is empty even though a cycle succeeded and its nodes exist in the other file. (`xvn optimizer ls` is a parallel symptom: it reads a separate ledger the cycle doesn't populate, so it prints "no optimizer runs" after a successful cycle.)

**Acceptance criteria.**
1. After a CLI `run-cycle`, `GET /api/autooptimizer` lists the cycle and `GET /api/autooptimizer/lineage` returns its nodes; the optimizer panel renders it as a historic run.
2. Genealogy (parent→child), gate verdict + reason, the blob-backed diff, mutator provenance/attribution, and the per-candidate backtest metrics are all populated for that historic run.
3. `xvn optimizer ls` / `inspect <cycle_id>` show the same CLI cycle.
4. A dashboard-launched cycle and a CLI-launched cycle with identical args produce equivalent panel state (single source of truth).

**Approach + file map.**
- **Converge on one lineage store = the main `xvn.db`** (what the dashboard pool already reads/writes). Change the CLI `run-cycle --db` default from `$XVN_HOME/lineage/lineage.db` to the resolved main DB path (`$XVN_HOME/xvn.db`), so CLI cycles land where the dashboard reads. — `crates/xvision-cli/src/commands/autooptimizer.rs` (the `db_path` default).
- **Ensure the lineage migrations create `lineage_nodes`/`mutator_attribution` in `xvn.db`** as part of the main migration set (today `xvn migrate` on `xvn.db` does not create them; only the per-file `open_and_migrate_db(--db)` path does). — migrations + `LineageStore`/`open_and_migrate_db` wiring.
- **Populate the optimizer run ledger** that `optimizer ls`/`inspect` and the panel list read, so a completed cycle is a first-class "historic run" row (not just lineage nodes). — `run-cycle` persistence + `crates/xvision-dashboard/src/routes/autooptimizer.rs` list endpoint.
- Keep the blob store path consistent (`$XVN_HOME/lineage/blobs`, already shared) so the diff inspector resolves.
- **Migration note:** existing nodes already written to `lineage/lineage.db` (e.g. cycle `01KT8NSKE41J0S4MWZZPX9H7QQ`) should be migrated/imported into `xvn.db` (one-time copy) so prior CLI runs aren't lost from the panel.

---

## F9 — [FIX] Canary must surface a labeled honesty-check outcome, not raw hidden-state broker errors

**Severity:** Medium-High (correctness of operator signal). **Goal:** the honesty-check (canary) reports a clear, typed result; its intentional rejections never masquerade as real broker/runtime errors.

**Problem.** Each cycle runs a canary that sabotages a copy of the strategy to confirm the gate rejects a no-edge variant. With `sabotage_seed = 42` (`42 % 3 == 0` → `apply_sabotage_kill_trades`, `canary.rs:91`) it sets `s.risk.risk_pct_per_trade = 0.0`. Order sizing is `notional = equity × risk_pct_per_trade` (`backtest.rs:1711`), so every order is $0 → trips `MIN_ORDER_NOTIONAL_USD = $1.0` (`broker_rules.rs:209,303`) → all rejected. The shared executor emits these as raw `WARN broker rule rejected order — min_order_size_violation` — **125 such lines in this one cycle**. That is a *hidden-state error*: the operator (and this QA pass) cannot tell the canary's intentional sabotage from a genuine broker/sizing failure. The sabotage variant (`kill-trades` / `remove-loss-limit` / `absurd-cadence`) is also not surfaced anywhere operator-facing.

**Acceptance criteria.**
1. Canary backtests are tagged (e.g. `canary = true`, with the sabotage variant name) on the run + in the progress stream.
2. Broker-rule rejections produced by a sabotaged canary run are NOT emitted as bare `WARN min_order_size_violation`; they are either logged at `debug` or annotated as expected, e.g. `honesty-check canary (kill-trades): orders correctly zero-sized — expected`.
3. The `HonestyCheckResult` carries a human-readable message that the CLI prints and the UI shows, e.g. *"Honesty check passed: sabotaged variant `kill-trades` (zeroed position sizing) was correctly rejected by the gate."*
4. A genuine `min_order_size_violation` on a real (non-canary) run still logs as `WARN` — only the canary-context noise is suppressed/relabeled.

**Approach + file map.**
- Thread a canary/sabotage context into the paper-test run (a flag on the run or an executor option) so the executor can label/relabel broker-rule findings produced under sabotage. — `crates/xvision-engine/src/autooptimizer/canary.rs`, `eval_adapter.rs`, and the broker-rule logging site in `crates/xvision-engine/src/eval/executor/backtest.rs:~1730`.
- Enrich `HonestyCheckResult` (`canary.rs:24`) with `sabotage_variant` + `message`; emit it through `CycleProgressEvent` and render in the optimizer panel + CLI summary.

---

## F10 — [FIX] Optimizer paper-test must share the eval entry path (harness parity), with a parity test

**Severity:** Medium (drift risk to the interchangeability guarantee). **Goal:** "the optimizer harness" and "the eval harness" are literally one code path, proven by a test — so an optimized strategy is scored identically to how `eval run` scores it.

**Problem.** The optimizer paper-test correctly shares the *engine* (`crate::eval::executor::Executor`, `eval_adapter.rs:17,80,138`) — confirmed empirically: the parent strategy fills identically via standalone `eval run` and via the optimizer's candidate backtests, so strategies are interchangeable *today*. But the *setup plumbing* is duplicated, and each copy can silently drift:
1. **Scenario is synthesized inline** in `crates/xvision-cli/src/commands/autooptimizer.rs` with hardcoded `venue`/`fees (maker 10 / taker 25)`/`slippage None`/`fill FullAtClose`/`Capital::default()`, instead of the normal scenario builder. (Capital matches eval's $100k by coincidence of `Capital::default()`, not by sharing code.)
2. **Slot resolution** uses `agent::pipeline::resolve_agent_slots_for_strategy` (the F1 fix), a sibling of `api::eval::resolve_agent_slots` — two resolvers to keep in lockstep.
3. **Executor construction** uses `eval_adapter::build_cached_backtest_executor`, separate from `api/eval`'s executor build.

If any of these three drifts (a fee default, a fill model, a slot field, an inputs policy), the optimizer would score strategies under different conditions than eval — and optimized winners would not transfer. Nothing catches that today.

**Acceptance criteria.**
1. Scenario-build, slot-resolve, and executor-build each have ONE shared, parameterized entry point used by both `api/eval` and the optimizer paper-test adapter.
2. A parity test runs the same strategy + window through `eval run` and through the paper-test adapter and asserts identical metrics (sharpe, return, trade count, fills).
3. The optimizer no longer hand-rolls a `Scenario`; it builds one through the shared builder (overriding only the day/baseline windows).

**Approach + file map.**
- Extract the eval setup (scenario construction from windows, `resolve_agent_slots`, executor build) into shared functions in `crates/xvision-engine/src/eval/` callable by both `api/eval.rs` and `autooptimizer/eval_adapter.rs`; delete the inline scenario synth in `commands/autooptimizer.rs` in favor of the shared builder.
- Add the parity integration test under `crates/xvision-engine/tests/` (or the eval test module).

---

## Minor findings

- **Mutator identity diff.** This cycle's `child bundle_hash == parent_hash` (`e3f9f8f378`) — the mutator returned a no-op diff, so two backtests were spent to confirm a guaranteed 0.0 delta. Detect identity diffs and retry/skip rather than paying for a backtest that cannot improve. (`autooptimizer/mutator.rs` + `cycle.rs`.)
- **No final cost summary.** `run-cycle` meters paper-test inference cost for `--budget` (F2) but prints no realized total at the end. Surface "cycle cost: $X.XX" on completion. (`commands/autooptimizer.rs`.)

## Positives
- F1–F3 fixes verified live: real cycle completes end-to-end, candidate gated + honesty-checked, window-override flags work, budget cap respected.
- The shared executor means optimized strategies are scored on the same engine as eval — interchangeability is intact today; F10 hardens it against future drift.

## Fix order
1. **F8** — make CLI cycles appear in the panel as historic runs with full data (highest operator value; it's the "where did my run go" gap).
2. **F9** — canary labeled outcome + no raw `min_order_size` noise (stops false-alarm investigations like this one).
3. **F10** — unify the eval/optimizer setup path + parity test (protects the interchangeability guarantee).
4. Minor: identity-diff short-circuit; final cost summary.

## Artifacts
- Run log: `/root/xvn-work/night-watch/optrun-geminilongv3-fixed-064038.log`
- Cycle: `01KT8NSKE41J0S4MWZZPX9H7QQ` (lineage node `e3f9f8f378`, rejected, 0.0 delta) — currently only in `$XVN_HOME/lineage/lineage.db` (the F8 store split), not yet in the panel.
- Real eval runs: `01KT8NV2EV/01KT8NXSCC/01KT8NZ2ZF/01KT8P1P3G/01KT8P4HA2` (filled); canary runs: `01KT8P32V2/01KT8P7H66` (0 fills, by design — F9).

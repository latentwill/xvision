# AutoOptimizer Run-5 — verification + new findings (F28, F32–F34) — coding-agent handoff

**Date:** 2026-06-04
**Deploy:** `xvision:deploy-latest` (image built 15:19Z = 23:19 +0800) — PRs **#809 (F23)**, **#810 (F26)**, **#811 (F30)**, **#812 (F31)**.
**Test:** ran a CLI cycle (small windows) **and** a UI/dashboard cycle (`POST /api/autooptimizer/run-cycle`) on `gemini_long_gate_v3` simultaneously, to verify the now-unified path. Verified the headline fixes — and the run surfaced four issues that need the next pass.

---

## Verified fixed ✅
- **F26** — the **dashboard/UI cycle drives real backtests** (17 real eval_runs with real bars + tokens), not the old instant `sharpe 0.9` stub (`start_cycle` now builds `CachedBacktestPaperTester` → `Executor` → `run_pipeline`, the same brain as CLI/eval/live).
- **F23** — per-cycle tokens + cost persisted to `cycle_cost`: CLI cycle `01KT9KRWHF` = **3,340,711 in / 33,025 out tokens, $0.8847, 0 unpriced**.
- **F11** — realized cost is metered and non-zero ($0.88; gemini priced; 0 unknown-price calls). The `$0.00` bug is gone.
- Core loop — real `risk.*` candidate proposed → gated on real Day/Hold Sharpe → dropped (no improvement), honesty check labeled.

---

## F28 — [HIGH, now urgent] UI cycles are unbounded and uncancellable (token spew)
F26 made the UI path real — but with **no cost guard and no stop button**, so it's now a live token-spew risk.
- **No window/budget control:** `StartCycleBody` accepts only `strategy_id` + mutator/judge provider/model — no day/baseline window, no budget. So a UI launch runs the **full ~20-month default window**. Observed: one UI launch ran **17 backtests and burned 11,671,496 input / 114,955 output tokens (~$3+) in ~27 minutes** and was still going. The dashboard `BudgetCappedPaperTester` is explicitly "unbounded until F28 adds a UI budget control."
- **No cancel route:** there is no autooptimizer cancel endpoint (only `/api/eval/runs/:id/cancel` and `/api/cli/jobs/:id/cancel`, neither of which stops a cycle). The only way to halt the runaway run was a **container restart**.

**Acceptance.** (1) `StartCycleBody` + the launch form expose day/baseline window + budget (mirror the CLI flags); (2) the dashboard cycle's budget actually caps spend (route through the working F11 meter); (3) a `POST /api/autooptimizer/cycles/:id/cancel` (or run-cycle cancel) cleanly stops an in-flight cycle and the Live tab reflects it. **Files:** `routes/autooptimizer_cycle.rs` (`StartCycleBody`, budget wiring, cancel handler), `server.rs` (route), `frontend/web/src/features/autooptimizer/` (form + cancel button). Until shipped, the UI optimizer should not be exposed to operators without a warning.

## F32 — [HIGH] The mutator is deterministic — it can't explore, so the optimizer can't converge
Two independent cycles on the same parent produced the **identical** candidate:
- CLI cycle `01KT9KRWHF` and dashboard cycle `01KT9KSRMC`, both off parent `e3f9f8f378`, both yielded child `b5505dd671` (same content hash) — and every prior v3 cycle produced that same `b5505dd671`.

So a given (parent, mutator, model) always proposes the **same single mutation**. Re-running a cycle never tries anything new; the optimizer cannot search the space or accumulate improvements — it proposes one fixed tweak, and if it doesn't beat `min_improvement`, every future cycle re-proposes and re-backtests the exact same losing candidate (pure waste). This is *the* blocker to the optimizer actually optimizing over time (ties to F20: a live KEPT improvement is unlikely while exploration = 1 fixed point).

**Acceptance.** Successive cycles on the same parent produce *diverse* candidates (vary the mutator sampling/seed per cycle/per mutation, or raise `mutations_per_parent` with diversity), so the optimizer explores and can converge. A test asserts N cycles on one parent yield ≥2 distinct candidate hashes. **Files:** `autooptimizer/mutator.rs` (sampling/temperature/seed), `cycle.rs` (per-cycle seed, `mutations_per_parent`).

## F33 — [MED] Content-addressed lineage attribution is first-writer-wins → cycles show empty
Lineage nodes are keyed by candidate content hash. When two cycles (concurrent, or re-runs — common given F32) produce the same candidate hash, the node is tagged to whichever cycle **wrote it first**. Observed: `b5505dd671` is attributed only to `01KT9KSRMC`, so `xvn optimizer inspect 01KT9KRWHF` (the CLI cycle that also gated `b5505dd671`) returns an empty header — its work is invisible. F13/F23 run-detail/token surfacing silently breaks for any duplicate/re-derived candidate.

**Acceptance.** A cycle's `inspect`/`/cycles/:id` reflects the candidates *that cycle* evaluated, even when the candidate hash already exists (e.g. record a per-cycle evaluation/edge separate from the content-addressed node, or allow many cycle_ids per node). **Files:** `autooptimizer/cycle_runs.rs`, lineage write path in `cycle.rs`/`lineage.rs`.

## F34 — [MED] No concurrency guard — overlapping cycles contend and one dies
The CLI cycle and dashboard cycle ran simultaneously against the same container/provider; the CLI cycle was resource-starved and **timeout-killed at 9.7 min** before finishing (its cost still persisted, but no final summary). Nothing prevents or warns about overlapping cycles, and they degrade each other.

**Acceptance.** Either serialize cycles (a lock/queue with a clear "a cycle is already running" response) or document/handle concurrent cycles explicitly; a long-running cycle shouldn't silently starve another. **Files:** `cycle.rs` / dashboard launch (a run lock keyed by workspace).

---

## F35 — [HIGH] UI shows `$0.00` cost and no tokens (three causes)
Operator-reported: the optimizer panel shows `$0.00` cost and no token usage, despite F11/F23 working in the CLI. Three distinct causes, all confirmed:
1. **Cost/tokens only persist at cycle END.** The meter (`CycleMeter`) accumulates in-memory and `persist_cycle_cost` runs once at the end. Any cycle that is in-progress, killed, or crashed records **nothing** — e.g. the runaway UI cycle `01KT9KSRMC` spent **11.67M tokens / ~$3** but, killed by the restart, persisted no `cycle_cost` row, so the panel shows `cost_usd: 0.0, input_tokens: 0`. This is *the* reason the operator saw `$0.00` (their cycle never reached the end).
2. **`GET /api/autooptimizer/cycles/:id` (detail) omits cost/token fields** — only the `/cycles` *list* endpoint returns `cost_usd`/`input_tokens`/`output_tokens`/`unpriced_calls` (F23's LEFT JOIN was added to the list query, not the detail query). So the cycle-detail view shows no cost/tokens even for a *completed* cycle (verified: detail for completed CLI cycle `01KT9KRWHF` returns no cost/token fields, while the list row for it has them).
3. **No live cost/tokens during a run.** The "Live" tab (where an operator watches a long cycle) has no incremental spend/token display — the total only appears after completion. For a 20-min+ cycle that's exactly when the operator needs it (and when they'd notice the spew).

**Acceptance.** (1) Meter persists incrementally (e.g. after each backtest) and/or is captured on cancel/crash so partial spend is never lost; (2) `/cycles/:id` returns the same cost/token fields as `/cycles`; (3) the Live tab streams running cost + tokens (pair with the F28 cancel button so an operator can see spend climbing and stop it). **Files:** `autooptimizer/cycle_runs.rs` (persist incrementally; detail query JOIN), `routes/autooptimizer.rs` (`:id` fields), `cycle.rs`/`progress.rs` (emit running totals as events), `frontend/.../LiveCycleView.tsx`.

## F36 — [HIGH] Cancelled / failed / interrupted **eval runs** lose metrics (and cost) — same gap, whole eval layer
F35 is the optimizer-cycle face of a broader bug: **the core eval path only persists metrics at `RunStore::finalize`**, which the cancel/fail/interrupt paths never reach. So any eval that doesn't finish cleanly silently loses its results. Confirmed in the live DB:
- **Cancelled eval runs: 7 — ALL 7 have `metrics_json = NULL`** (tokens *are* retained, so token accounting is incremental but metrics/cost are not).
- **Failed eval runs: 17 — ALL 17 have `metrics_json = NULL`, and 14 also have null/zero tokens.**
- `metrics_json` is written only by `RunStore::finalize` (eval/run.rs:293); cancel (`routes/eval_runs.rs::cancel_run`) and failure paths mark status without persisting the metrics/cost accumulated so far.

This matters beyond the optimizer: it's the **same engine all eval, optimizer paper-test, and LIVE runs share** (one `Executor`). A cancelled or crashed **live/real-money** run would likewise show no metrics/cost — bad for accounting and post-mortems. The coding agent should treat F35 (optimizer `cycle_cost`) and F36 (eval `metrics_json`/tokens/cost) as one "capture-on-interrupt" fix across the shared path.

**Acceptance.** On cancel / failure / timeout / crash, an eval run persists the metrics + token + cost accumulated up to that point (status reflects partial/interrupted), rather than `NULL`. A test cancels a mid-flight run and asserts non-null partial metrics + cost. **Files:** `crates/xvision-engine/src/eval/run.rs` (`RunStore` — persist on cancel/fail, not only `finalize`), the executor decision loop (`eval/executor/backtest.rs` — flush partial metrics on interrupt), `routes/eval_runs.rs::cancel_run`.

---

## Suggested order for the next pass
1. **F28 + F35 + F36** — UI window/budget/cancel **and** honest live cost+tokens (so the operator can see spend climb and stop it; the runaway run is the proof). Highest urgency now that F26 is live.
2. **F32** — make the mutator explore (without it, the optimizer never converges — the whole point).
3. **F33 / F34** — attribution for duplicate candidates; concurrency guard.

## Status recap
- **Fixed + verified:** F1–F18, F20, F21, F22(preflight), F23, F26, F30, F31, F11.
- **Open:** **F28** (UI unbounded+uncancellable, now urgent), **F32** (deterministic mutator), **F33** (attribution collision), **F34** (concurrency), **F35** (UI shows $0.00/no tokens), **F36** (cancelled/failed eval runs lose metrics+cost, whole eval layer), plus prior **F24** (configurable objective), **F25** (model-swap axis), **F27** (panel run-list wiring), **F29** (mutate-once/retire UI).

## Artifacts
- Runaway UI cycle: 17 runs, 11.67M in / 115k out tokens, ~27 min, halted by `docker restart xvn-app`.
- CLI cycle `01KT9KRWHF` (cost persisted: $0.8847 / 3.34M tok); dashboard cycle `01KT9KSRMC`; shared candidate `b5505dd671` off parent `e3f9f8f378`.

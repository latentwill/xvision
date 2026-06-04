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

## Suggested order for the next pass
1. **F28** — UI window + budget + cancel (safety: the UI can currently spew ~$3+/27min unchecked). Highest urgency now that F26 is live.
2. **F32** — make the mutator explore (without it, the optimizer never converges — the whole point).
3. **F33 / F34** — attribution for duplicate candidates; concurrency guard.

## Status recap
- **Fixed + verified:** F1–F18, F20, F21, F22(preflight), F23, F26, F30, F31, F11.
- **Open:** **F28** (UI unbounded+uncancellable, now urgent), **F32** (deterministic mutator), **F33** (attribution collision), **F34** (concurrency), plus prior **F24** (configurable objective), **F25** (model-swap axis), **F27** (panel run-list wiring), **F29** (mutate-once/retire UI).

## Artifacts
- Runaway UI cycle: 17 runs, 11.67M in / 115k out tokens, ~27 min, halted by `docker restart xvn-app`.
- CLI cycle `01KT9KRWHF` (cost persisted: $0.8847 / 3.34M tok); dashboard cycle `01KT9KSRMC`; shared candidate `b5505dd671` off parent `e3f9f8f378`.

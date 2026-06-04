# AutoOptimizer Surface Audit & Run-2 Findings (coding-agent handoff)

**Date:** 2026-06-04
**Deploy:** `xvision:deploy-latest` (image built 09:08Z, includes `c162135a` "fix F8-F10" and `44015624` "fix F1-F7")
**Context:** Second verification pass after the F8–F10 fixes shipped. Confirms what landed, then audits the **whole optimizer surface** (CLI verbs + dashboard endpoints) to expand the backlog before handoff. Findings are numbered **F11–F19** (continuing the F1–F10 series).

This is a handoff doc: each finding has severity, evidence, acceptance criteria, and an approach + file map.

---

## Verified fixed (this pass)

- **F9 ✅** — canary no longer floods logs: raw `min_order_size_violation` WARN lines went **125 → 0** on a real cycle; honesty check now reports a labeled outcome (`sabotage_variant:"kill-trades"`, message: *"…zeroed position sizing… correctly rejected by the gate."*).
- **F10 ✅** — single shared `synthesize_optimizer_day_scenario` used by CLI + dashboard; parity test `optimizer_adapter_matches_direct_eval_executor` present.
- **F8 ⚠️ partial** — lineage is now unified in `xvn.db` and the prior CLI cycle `01KT8NSKE…` was one-time-imported; `/api/autooptimizer/lineage` and `xvn optimizer lineage ls/show` now serve it. **But** the run-list/run-detail surfaces are still empty (see F13) — a completed cycle does not appear as a discrete "historic run".

## Surface coverage matrix

| Surface | State |
|---|---|
| `xvn optimizer run-cycle` | ✅ executes end-to-end |
| `xvn optimizer lineage ls` | ✅ works |
| `xvn optimizer lineage show` | ⚠️ works but **infinite self-loop** on self-parent nodes (F12) |
| `xvn optimizer demo` | ✅ works; ⚠️ stale `cycle_sealed` term (F18) |
| `xvn optimizer ls` | ❌ "no optimizer runs" after a real cycle (F13) |
| `xvn optimizer inspect <cycle>` | ❌ header line only, no data (F13) |
| `xvn optimizer mutate-once <hash>` | ❌ "parent bundle not found" for a real node (F16) |
| `xvn flywheel status` / `velocity` | ⚠️ require `--namespace/--agent`; no global view (F17) |
| `GET /api/autooptimizer` (run list) | ❌ `{"items":[],"total":0}` (F13) |
| `GET /api/autooptimizer/:id` (detail) | ❌ HTTP 404 (F13) |
| `GET /api/autooptimizer/lineage` | ✅ works |
| `GET /api/autooptimizer/blob/:hash` | ✅ HTTP 200, returns strategy JSON |
| run-cycle cost / `--budget` | ❌ reports $0.00; budget blind (F11) |
| mutator candidate generation | ❌ identity diff (v3) or no candidate (v2) (F14) |
| progress events (real vs demo) | ⚠️ real runs miss `mutation_*`/`judge_finding` (F15) |

---

## F11 — [HIGH] Cost reported as $0.00; the `--budget` cap is blind

**Evidence.** A real cycle on `gemini_long_gate_v2` printed `cycle cost: $0.00 (metered paper-test inference)`. The `model_calls` observability ledger for the same window recorded `openrouter/google/gemini-3.1-flash-lite`: **342 calls, 7,975,941 input tokens, 31,280 output tokens, cost_usd ≈ $2.04**. Tokens were clearly used; the printed cost is wrong.

**Root cause.** Two disconnected cost paths:
- `model_calls.cost_usd` computes cost from token counts × pricing — correct.
- The run-cycle meter (`BudgetCappedPaperTester`, F2) sums `MetricsSummary.inference_cost_quote_total`, which **openrouter does not populate** → sums to 0.

**Consequence.** The `--budget` ceiling never trips for openrouter (the run cost ~$2.04 against a `$3` cap it believed was `$0.00`; `--budget 1` would have been ignored). The F2 guard is effectively non-functional for the primary provider.

**Acceptance.**
1. `cycle cost:` reflects realized spend within a small tolerance of the `model_calls` ledger sum for the cycle's runs.
2. `--budget` trips based on that realized spend (set a low budget, confirm the cycle aborts before the next backtest).
3. Mutator/judge LLM calls are included in the metered total, not just paper-test inference.

**Approach + files.** Meter from the same source `model_calls` uses (token counts × provider pricing) instead of `inference_cost_quote_total`; aggregate over all of the cycle's run_ids (paper-test + mutator + judge). — `crates/xvision-engine/src/autooptimizer/eval_adapter.rs` (`BudgetCappedPaperTester`), the cost-summary print in `crates/xvision-cli/src/commands/autooptimizer.rs`, and wherever `model_calls.cost_usd` pricing is computed (reuse it).

---

## F12 — [HIGH] Identity-diff mutations corrupt the lineage graph (self-parent → re-run block + infinite ancestry)

**Evidence.** The v3 cycle's mutator returned an identity diff, so the candidate's `bundle_hash == parent_hash` (`e3f9f8f378…`). This produced a node whose `parent_hash` equals its own `bundle_hash`. Two downstream failures:
1. **Re-run blocked.** `xvn optimizer run-cycle --strategy <v3>` now fails with `strategy … resolves to lineage parent … but that parent is not active` — the candidate's rejection wrote `status=rejected` onto the node whose hash collides with the root, and `load_strategy_parent` refuses a non-active parent (`commands/autooptimizer.rs`).
2. **Infinite ancestry.** `xvn optimizer lineage show e3f9f8f378…` walks `depth=1..N` all printing the same hash (self-cycle), never terminating at a root.

**Acceptance.**
1. The mutator never emits a child whose bundle hash equals its parent's; identity/no-op diffs are detected and skipped/retried (tie to F14).
2. A rejected candidate never mutates the status of a different lineage node, even on hash collision.
3. The ancestry walk is cycle-safe (visited-set / self-parent guard) and terminates.
4. A previously-run strategy can be re-run (a rejected candidate does not poison the active root).

**Approach + files.** Guard in `autooptimizer/mutator.rs` (reject identity diff) + `cycle.rs` (skip gating a child == parent); cycle-safe ancestry in `autooptimizer/lineage.rs` (the `show`/ancestry walk) and the CLI `lineage show` handler; re-run path in `commands/autooptimizer.rs::load_strategy_parent` (distinguish "root strategy node" from "rejected candidate" so a re-run reseeds/uses an active root).

---

## F13 — [HIGH] A completed cycle is not a first-class "run": list + detail are empty/404

**Evidence.** After two successful cycles, all run-oriented surfaces are empty:
- `GET /api/autooptimizer` → `{"items":[],"total":0}`
- `GET /api/autooptimizer/:id` → **HTTP 404** `{"code":"not_found","message":"autooptimizer run …"}`
- `xvn optimizer ls` → "no optimizer runs"
- `xvn optimizer inspect <cycle>` → prints only `optimizer inspect: autooptimizer run <id>` with no data

Only the lineage/genealogy surfaces (`/api/autooptimizer/lineage`, `lineage ls/show`) reflect the cycle. So the optimizer panel can render a genealogy graph but cannot list a cycle as a historic run, nor open its detail (gate verdict, diff, per-candidate metrics, provenance). The c162135a commit explicitly deferred this ("`optimizer ls`/`inspect` read `autooptimizer_runs` — a separate memory-distillation ledger; wiring mutation cycles into it is a semantic mismatch, deferred").

**Acceptance.**
1. Each completed `run-cycle` (CLI or dashboard) creates a run-ledger row keyed by `cycle_id`.
2. `GET /api/autooptimizer` lists it; `GET /api/autooptimizer/:id` returns its detail (gate verdict + reason, candidate diff via blob, per-candidate backtest metrics, mutator provenance, honesty-check result).
3. `xvn optimizer ls`/`inspect <cycle>` show the same data (or are redirected to the lineage surface with a clear pointer, if the `autooptimizer_runs` ledger is reserved for distillation).
4. The optimizer panel shows the cycle as a historic run with all collected data.

**Approach + files.** Resolve the ledger semantics: either (a) add a cycle/run ledger table for mutation cycles (distinct from the distillation `autooptimizer_runs`) and have the dashboard list/detail + CLI `ls`/`inspect` read it, or (b) build the run list/detail from `lineage_nodes` grouped by `cycle_id`. — `crates/xvision-dashboard/src/routes/autooptimizer.rs` (list + `:id` detail), `crates/xvision-cli/src/commands/autooptimizer.rs` (`ls`/`inspect`), migrations if a new table.

---

## F14 — [HIGH] Mutator is unreliable: identity diff (v3) or no candidate at all (v2)

**Evidence.** Across two real cycles the mutator never produced a usable, distinct candidate:
- v3 (`01KT8NSKE…`): identity diff → child == parent (feeds F12).
- v2 (`01KT8YPWP7…`): **no candidate node persisted at all** — lineage shows only the seeded root `01f5019498` (cycle_id null); no `mutation_proposed`/`mutation_gated` events; the cycle ran parent + canary backtests and the honesty check, then "succeeded" with nothing to show.

A cycle that produces no candidate still exits 0 and prints a cycle_id, so it looks successful while accomplishing nothing.

**Acceptance.**
1. The mutator reliably produces a distinct, valid candidate from a parent strategy with the configured provider/model; if it cannot, the cycle emits a typed "no candidate produced" outcome (not silent success).
2. Identity/empty diffs are detected and retried (bounded) or reported.
3. A no-candidate cycle is distinguishable in CLI output and the panel from a cycle that gated a real candidate.

**Approach + files.** Inspect mutator LLM output handling (empty/invalid diff, retries) in `autooptimizer/mutator.rs`; emit a `CycleProgressEvent` for "no candidate" in `cycle.rs`/`progress.rs`; surface it in the CLI summary and panel.

---

## F15 — [MED] Real run-cycle drops progress events that the demo emits

**Evidence.** `xvn optimizer demo` emits the full sequence (`cycle_started, parent_selected, mutation_proposed, mutation_gated, honesty_check_run, judge_finding, cycle_sealed`). Real runs are sparser: v2 emitted only `cycle_started, parent_selected, honesty_check_run`; neither real run emitted `judge_finding`. The live/event view and genealogy therefore lose steps that did happen (backtests ran).

**Acceptance.** A real cycle emits the same event vocabulary as the demo for the steps it actually performs (including `judge_finding` when the judge runs, and a candidate/no-candidate event). Demo and real path share one event-emission code path.

**Approach + files.** Audit event emission in `autooptimizer/cycle.rs` + `progress.rs` vs the demo fixture; ensure judge + mutation events are emitted (and persisted) on the real path. Partly overlaps F14 (no-candidate ⇒ no mutation events).

---

## F16 — [MED] `mutate-once` can't find a real lineage node's blob

**Evidence.** `xvn optimizer mutate-once 01f5019498… --mock --dry-run` (the v2 active root, present in `lineage_nodes`) → `parent bundle 01f5019498… not found`. Yet `GET /api/autooptimizer/blob/<hash>` returns the strategy JSON for an existing hash, so the blob exists somewhere. This is a blob-dir default mismatch (same store-split class as F8): `mutate-once`'s default `--blob-dir` differs from where `run-cycle`/the dashboard write.

**Acceptance.** `mutate-once <hash>` resolves any hash present in the unified lineage/blob store without an explicit `--blob-dir`; defaults align across `run-cycle`, `mutate-once`, and the dashboard.

**Approach + files.** Align the `--blob-dir` default in `commands/autooptimizer.rs` (`mutate-once`) with the run-cycle/dashboard blob root (`$XVN_HOME/lineage/blobs`).

---

## F17 — [LOW] `flywheel status`/`velocity` have no global view

**Evidence.** `xvn flywheel status` and `xvn flywheel velocity` both error `set either --namespace or --agent`. There is no global optimizer-activity summary for an operator who just wants the overall picture.

**Acceptance.** `flywheel status` with no args prints a workspace-wide summary (Observations / Patterns / optimizer runs across namespaces), or documents a clear default namespace.

**Approach + files.** `crates/xvision-cli/src/commands/flywheel.rs` — allow a global/default aggregation.

---

## F18 — [LOW] Stale `cycle_sealed` / "Cycle summary signed" in the demo fixture

**Evidence.** `xvn optimizer demo` emits `cycle_sealed: Cycle summary signed` — crypto-provenance terminology that was removed from the engine (PR #708/#753). The demo fixture still references the sealed/signed concept.

**Acceptance.** The demo fixture no longer emits `cycle_sealed`/"signed" language; it matches the current (provenance-free) event vocabulary.

**Approach + files.** Update the demo replay fixture + `event_operator_label`/`event_type_tag` in `commands/autooptimizer.rs` (drop or rename `CycleSealed`).

---

## F19 — [MED] Inconsistent inspect/list verbs (two of three are empty stubs)

**Evidence.** `lineage ls`/`lineage show` work and reflect real data; `optimizer ls` says "no runs" and `optimizer inspect <cycle>` prints a header with no body — for cycles that demonstrably exist in lineage. Operators get contradictory answers depending on which verb they pick.

**Acceptance.** All inspect/list verbs are consistent: either they all reflect completed cycles, or the empty ones explicitly redirect ("no mutation-cycle runs; see `xvn optimizer lineage ls`"). Overlaps F13's ledger resolution.

**Approach + files.** `commands/autooptimizer.rs` (`ls`, `inspect`) — wire to the cycle ledger (F13) or add explicit redirects.

---

## Carry-over / context (not new findings)
- **Heavy token cost:** ~8M input tokens for a 1-month, single-asset backtest cycle — bar-history/wake re-send amplification. Worth a perf pass (separate from F11's metering bug).
- F1–F10 status recap: F1–F3, F9, F10 shipped + verified; F8 plumbing shipped, run-surface remainder is F13.

## Suggested fix order for handoff
1. **F11** (cost/budget — safety: an unmetered cap can run away) and **F14** (mutator producing nothing — the optimizer's core job).
2. **F12** (lineage corruption — blocks re-runs + breaks ancestry) and **F13** (make cycles first-class runs — the visible-in-UI ask).
3. **F15, F16, F19** (event/verb/blob consistency).
4. **F17, F18** (ergonomics/terminology).

## Artifacts
- v2 run log: `/root/xvn-work/night-watch/optrun-v2-f8f10-091627.log` (cycle `01KT8YPWP72F2BCZZSRYZK6GP7`)
- v3 re-run failure: `/root/xvn-work/night-watch/optrun-geminilongv3-f8f10-091505.log` ("parent not active")
- Token/cost evidence: `model_calls` in `xvn_data` volume `xvn.db` (gemini-3.1-flash-lite: 342 calls / ~8M in-tok / $2.04)
- Lineage state: nodes `90c621fd14`(active root), `7dc0e668bf`(rejected), `e3f9f8f378`(rejected, self-parent), `01f5019498`(active root, v2)

# AutoOptimizer CLI↔UI Parity — the dashboard optimizer is disconnected (F26–F29)

**Date:** 2026-06-04
**Deploy:** `xvision:deploy-latest` (image built 13:09Z)
**Trigger:** An operator launched an optimizer run **through the UI** and it "immediately failed / rejected." Root-caused to a stubbed dashboard path, then audited the whole CLI↔UI optimizer surface for other disconnects. Findings only.

**Bottom line:** the CLI optimizer does real work; the **dashboard optimizer is largely disconnected** — it fakes execution (F26), doesn't read the real run data (F27), and can't be configured like the CLI (F28). All the F1–F25 fixes landed on the CLI path; the UI path was left behind.

---

## F26 — [CRITICAL] ✅ RESOLVED (PR #810) The dashboard "Optimizer run" is stubbed — UI cycles never backtest

**Resolution (2026-06-04, PR #810).** `start_cycle` now builds the production
`CachedBacktestPaperTester` (→ `build_cached_backtest_executor` → `Executor` →
`run_pipeline`) from a fully-wired `ApiContext` (`state.api_context()`, event bus
+ observability) and the builtin `ToolRegistry`, wrapped in
`BudgetCappedPaperTester` (unbounded ceiling until F28 adds a UI budget control)
sharing one `CycleMeter`. The `StubPaperTester` and `stub_paper_tester()` are
gone. Cost is metered at the dispatch boundary (`CostMeteringDispatch` over the
experiment writer + judge + backtest decisions) and persisted via
`persist_cycle_cost` after the run — F11/F23 now flow through the real dashboard
path. The F22 cross-provider preflight was extracted to
`xvision_engine::autooptimizer::preflight` and is called from BOTH the CLI and
the dashboard (no parallel guard). **Single-path acceptance:** the dashboard now
shares the exact `Executor`/`run_pipeline` brain as CLI/`eval run`/chat/live; the
engine test `optimizer_adapter_matches_direct_eval_executor` is the invariant and
its doc records the dashboard is on this path.

---

### (original finding)

`POST /api/autooptimizer/run-cycle` (`crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:107`) passes a **`StubPaperTester`** (fixed `sharpe: 0.9`, `:309 stub_paper_tester()`) into `run_cycle`, instead of the real `CachedBacktestPaperTester` the CLI uses. So a UI-launched cycle runs a real mutation + judge, but **fakes the paper-test scoring** — parent and candidate both get `sharpe 0.9` → `improvement = 0.000000` → the gate rejects instantly.

**Evidence (the operator's run):** cycle `01KT9DT0TWE0A9XT6PQNQST5NN`, candidate `42472e87` → **Day Sharpe 0.900 / Hold 0.900** (the stub's literal numbers), lineage node `first = last timestamp = 13:40:19.43` (same millisecond — no backtest ran; real CLI cycles take 5–9 min). Verdict: `rejected — improved by 0.000000`. This is the "launch → immediately rejected" the operator saw: the stub returns instantly and always ties.

**Impact:** the dashboard optimizer is non-functional — it can never produce a real score, never keep a candidate, and always rejects after ~0s. Every UI run is a no-op. (This is the exact gap the 2026-06-02 plan flagged — "dashboard launch uses StubPaperTester … should use CachedBacktestPaperTester" — never fixed; all subsequent work was CLI-only.)

**Fix.** Build the real `CachedBacktestPaperTester` in the dashboard handler (same `ApiContext` pool + a real LLM dispatch for the strategy's trader), exactly as `commands/autooptimizer.rs` does; feed it the synthesized day/baseline scenarios it already constructs (and currently discards). Route the F11 cost metering + F23 token accounting through it too.

---

## F27 — [HIGH] Frontend never reads the real cycle endpoints (F13 stranded behind the UI)
F13 added `GET /api/autooptimizer/cycles` and `/cycles/:id` (verified returning real data), and the CLI `optimizer ls`/`inspect` use them. But the **frontend optimizer panel never calls them.** `frontend/web/src/features/autooptimizer/api.ts` calls only:
`/api/autooptimizer/` (the flywheel memory-distillation list → `{items:[],total:0}`), `/lineage`, `/lineage/:hash`, `/ladder`, `/diversity`, `/blob/:hash`, `/events`, `/run-cycle`.

So the UI's "run list" reads the empty distillation list, not `/cycles`. Completed mutation cycles still don't appear as historic runs in the panel (the genealogy tab works via `/lineage`, but the run list/detail with gate verdict + Day/Hold Sharpe + tokens/cost does not). F13's server work doesn't reach the operator.

**Fix.** Wire the panel's run-list + run-detail to `/api/autooptimizer/cycles[/:id]` (the data is already there).

---

## F28 — [MED] UI launch can't set the evaluation window or budget (CLI-only knobs)
`StartCycleBody` (`autooptimizer_cycle.rs:34`) accepts only `strategy_id`, `mutator_provider/model`, `judge_provider/model`. It has **no day/baseline window override and no budget**. The CLI has `--day-start/--day-end/--baseline-start/--baseline-end` (F3) and `--budget`. Consequences:
- A UI cycle always uses the config **default window (~20 months of 1h bars, ~16k bars/candidate)** — slow and expensive once F26 makes it real. There's no way to run the cheap, bounded windows the CLI testing used.
- No per-run cost bound from the UI (compounding F11).
- Also no control over `mutations_per_parent` / objective (ties to F24).

**Fix.** Add window + budget (+ later objective) fields to `StartCycleBody` and the launch form, mirroring the CLI flags.

---

## F29 — [LOW] CLI-only verbs with no dashboard/UI route
Parity gaps where a CLI verb has no API/UI equivalent:
- `xvn optimizer mutate-once` — single propose→gate→commit experiment; no dashboard route.
- `xvn optimizer retire` — retire a cycle-produced node; `:id/gate|promote|demote` exist for the distillation Pattern lifecycle, but there's no mutation-cycle retire in the UI.
- (`demo` is intentionally CLI-only — fine.)

**Fix.** Add routes/UI affordances if these are meant to be operator-facing; otherwise document them as CLI-only.

---

## What IS connected (so the fix is scoped, not total)
- UI launch **does** let you pick the strategy + mutator/judge provider/model (`StartCycleBody`), and builds **real** mutator/judge dispatch — only the paper-test is stubbed (F26).
- Read surfaces wired + working: `/lineage` (genealogy), `/ladder`, `/diversity`, `/blob/:hash` (diff), `/events` (SSE live progress).
- A broad scan of `crates/xvision-dashboard/src/routes/` found **no other stubbed execution paths** — the only fake-execution route is `autooptimizer_cycle.rs`. (`diagnostics.rs`'s "hardcoded mirror" is a documented, intentional dspy-free design, not a disconnect.)

## Severity & order
1. **F26** — make the dashboard run real backtests (the headline; the UI optimizer is otherwise a no-op).
2. **F27** — wire the panel to `/cycles[/:id]` so runs show up.
3. **F28** — UI window/budget/objective controls.
4. **F29** — mutate-once/retire parity (optional).

Depends on / relates to: **F11** (cost metering), **F23** (token+cost surfacing), **F24** (objective) — all should flow through the same real dashboard paper-test path once F26 lands.

## Artifacts
- Operator's stubbed UI run: cycle `01KT9DT0TWE0A9XT6PQNQST5NN`, candidate `42472e87` (Day/Hold Sharpe 0.900 = stub), node ts `13:40:19.43` (instant)
- Stub: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:107,309`
- Frontend endpoint list: `frontend/web/src/features/autooptimizer/api.ts` (no `/cycles`)

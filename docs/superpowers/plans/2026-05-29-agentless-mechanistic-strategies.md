# Agentless Mechanistic Strategies Implementation Plan

> **For Hermes:** Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this task-by-task.

**Goal:** Let users create and compare *mechanistic* xvision strategies that open and close positions without any LLM agent, while sharing the same risk, execution, evaluation, and compare surfaces as agentic strategies.

**Architecture:** Split the strategy runtime into two decision sources: `agentic` and `mechanistic`. Agentic strategies keep the current LLM-driven trader pipeline; mechanistic strategies replace the trader with deterministic entry/exit policies and still pass through the same risk and execution spine. The key missing primitive is not just "open a trade" but a first-class **close policy** with explicit exit reasons, so the platform can model position lifecycle end-to-end and compare mechanistic vs LLM strategies fairly.

**Tech Stack:** Rust 2021 (`xvision-engine`, `xvision-cli`, `xvision-dashboard`), SQLite, serde/ts-rs, the existing backtest/paper executors, and the dashboard compare surfaces.

**Reference docs:**
- `docs/freqtrade-graphs-and-metrics.md` — exit reasons, position lifecycle, grouped performance metrics
- `crates/xvision-engine/src/api/eval.rs` — current eval validation still requires an attached agent
- `crates/xvision-dashboard/src/routes/strategies.rs` — strategy authoring / validation UI paths
- `docs/superpowers/plans/2026-05-24-multi-asset-strategies.md` — example plan structure

---

## Why this matters

Today, xvision can evaluate agentic strategies, but the product still treats "at least one agent" as mandatory in several validation paths. That blocks the simpler class of strategies the user is asking for:

- rule-based entries and exits
- deterministic risk controls
- filter-gated trade activation
- direct comparison against LLM strategies on the same market scenario

This is also where the system currently feels incomplete: a mechanistic strategy needs an explicit **exit model**, not just an open signal. Without that, "close deals" collapses into an ambiguous flat/hold behavior and becomes hard to compare against LLM decisions.

---

## Proposed shape

### Strategy modes

Add an explicit strategy decision mode with two supported paths:

- `agentic` — current behavior; requires at least one trader-capable agent
- `mechanistic` — no agents required; uses rule-based decision logic

Optional future extension:

- `hybrid` — rules can open/close, while an LLM can advise or confirm

### Mechanistic strategy primitives

A mechanistic strategy should be able to define:

- **Entry rules** — when to open a long or short position
- **Close rules** — when to exit an existing position
- **Risk rules** — size limits, stop-loss, max drawdown, circuit-breakers
- **Filter rules** — regime / signal gates for whether the strategy is allowed to act
- **Exit reasons** — structured reasons for trade closure so evals can group results cleanly

### Closing deals: the missing piece

The close side should be first-class, not implicit. A mechanistic strategy should be able to say:

- close on stop-loss
- close on take-profit
- close on trailing stop
- close on time stop
- close on opposite signal
- close on regime flip
- close on volatility / liquidity shock
- force flat at session end

That means the engine needs a position lifecycle model that can emit explicit close events and distinguish them from:

- a new entry
- a hold / no-op
- a reversal
- a forced liquidation / safety exit

---

## Implementation plan

### Task 1: Introduce a strategy decision-mode contract

**Objective:** Allow the engine to distinguish agentic strategies from mechanistic ones without breaking existing strategies.

**Files:**
- Modify: `crates/xvision-engine/src/strategies/*`
- Modify: `crates/xvision-engine/src/api/eval.rs`
- Modify: `crates/xvision-engine/src/api/strategy.rs`
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`

**Work:**
- Add a `StrategyMode` / `DecisionMode` enum with `agentic` as the current default.
- Keep current strategies working unchanged.
- Allow `mechanistic` strategies to validate without any attached agent.
- Keep the existing agent-required path for `agentic` strategies.
- Make the UI and API surface explain *why* a strategy is runnable or blocked.

**Verification:**
- A strategy with no agent can be saved and validated when marked mechanistic.
- Existing agentic strategies still validate and run exactly as before.

---

### Task 2: Define a first-class close policy

**Objective:** Model trade closure explicitly so mechanistic strategies can manage full position lifecycle.

**Files:**
- Create or modify: `crates/xvision-engine/src/strategies/*`
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs`
- Modify: `crates/xvision-engine/src/eval/executor/paper.rs`
- Modify: `docs/freqtrade-graphs-and-metrics.md`

**Work:**
- Add a `ClosePolicy` / `ExitPolicy` shape that can be evaluated independently from entry.
- Support explicit exit reasons in the lifecycle output.
- Keep reversal behavior unambiguous: close first, then optionally reopen.
- Ensure close decisions are visible in recorded results and compare output.

**Verification:**
- Backtests emit explicit close events with exit reasons.
- A strategy can open, hold, and close positions without any LLM involvement.
- Metrics can group by exit reason.

---

### Task 3: Wire mechanistic strategies through the executors

**Objective:** Make mechanistic strategies run through the same backtest/paper pipeline as agentic ones.

**Files:**
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs`
- Modify: `crates/xvision-engine/src/eval/executor/paper.rs`
- Modify: `crates/xvision-engine/src/eval/export.rs`
- Modify: `crates/xvision-engine/src/eval/compare/*` if needed

**Work:**
- Execute deterministic entry/exit rules instead of invoking an LLM when the strategy is mechanistic.
- Preserve the same risk gate, fills, and reporting surfaces.
- Ensure evaluation artifacts still include positions, orders, fills, and close reasons.
- Keep LLM strategies and mechanistic strategies comparable in one evaluation report.

**Verification:**
- A mechanistic strategy can complete a full run in backtest and paper modes.
- Compare views show both strategy types side by side.
- No agent is required anywhere in the mechanistic execution path.

---

### Task 4: Update authoring surfaces

**Objective:** Let users create and inspect mechanistic strategies without tripping over agent-only UI assumptions.

**Files:**
- Modify: `frontend/web/src/routes/authoring.tsx`
- Modify: `frontend/web/src/routes/strategies.tsx`
- Modify: `frontend/web/src/components/...` for strategy forms and rule editors
- Modify: `crates/xvision-cli/src/commands/strategy.rs`
- Modify: `crates/xvision-cli/src/commands/eval/mod.rs`

**Work:**
- Add a mode selector: agentic vs mechanistic.
- For mechanistic strategies, expose entry rules, close rules, filter gates, and risk controls.
- Remove the hard assumption that a trader agent must exist before a strategy can be evaluated.
- Keep agentic strategy creation unchanged.

**Verification:**
- Users can create a mechanistic strategy in the UI and CLI.
- Eval launch surfaces no longer imply that every strategy needs an agent.
- The form makes close policy visible, not hidden in defaults.

---

### Task 5: Make the compare surface answer the right question

**Objective:** Let users compare mechanistic strategies against LLM strategies on the same metrics.

**Files:**
- Modify: `crates/xvision-engine/src/eval/export.rs`
- Modify: `crates/xvision-dashboard/src/routes/eval_runs.rs` or compare routes
- Modify: `docs/freqtrade-graphs-and-metrics.md`

**Work:**
- Add compare breakdowns for:
  - entry type
  - exit reason
  - hold duration
  - position lifecycle stats
- Keep the compare surface explicit about whether a result came from a mechanistic or agentic strategy.
- Make it easy to inspect whether the close policy or the entry policy is doing the damage.

**Verification:**
- The dashboard can compare agentic vs mechanistic runs without ambiguity.
- Exit-reason grouping is visible in exported data.
- Users can tell whether a strategy is weak on entry, exit, or risk control.

---

### Task 6: Add compatibility and guardrails

**Objective:** Avoid breaking existing agentic flows while opening the new mode safely.

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs`
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`
- Modify: tests alongside each touched module

**Work:**
- Preserve the current default strategy behavior.
- Add regression tests for:
  - agentic strategy still requires an agent
  - mechanistic strategy does not
  - close reasons are recorded correctly
  - compare output still works for old runs
- Make validation errors actionable, not generic.

**Verification:**
- Old strategies continue to run.
- New mechanistic strategies run without a model.
- Regression tests cover the no-agent path.

---

## Acceptance criteria

- Users can create a strategy that has **no LLM agent** and still runs.
- The strategy can both **open and close** positions deterministically.
- Risk and filters remain first-class inputs.
- Exit behavior is explicit and observable.
- Mechanistic strategies can be compared directly against agentic strategies in evals.
- Existing agentic behavior remains unchanged by default.

---

## Notes / design guardrails

- Do **not** hide close behavior inside a generic "flat" state.
- Do **not** force mechanistic strategies to fake an agent just to pass validation.
- Do **not** collapse agentic and mechanistic into one ambiguous execution path; the compare surface needs the distinction.
- Keep the first version narrow: deterministic rules for entry/exit + shared execution/eval surfaces.

---

## Open question for implementation

Should the first mechanistic version support only:
- long/short open + close
- or also scale-in / scale-out / reverse

My recommendation: start with open/close only, then add scaling once the lifecycle model is stable.

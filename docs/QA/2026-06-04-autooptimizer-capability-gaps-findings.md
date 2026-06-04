# AutoOptimizer Capability Gaps — token/cost visibility, objective, model axis (F23–F25)

**Date:** 2026-06-04
**Deploy:** `xvision:deploy-latest` (image built 13:09Z, PR #805)
**Context:** Follow-on to the run-4 verification. The mutate→gate→keep loop works now; these are *capability* gaps for making the optimizer genuinely useful — surfaced from operator questions. Findings only (no code in this pass). Pairs with the still-open **F11** (cost metering).

---

## F23 — [HIGH] Surface per-cycle token usage **and** cost, in CLI and UI

**Problem.** An optimizer run reports neither token usage nor working cost. The cycle prints only `cycle cost: $0.00` (broken — F11), and `xvn optimizer inspect <cycle>` shows Day/Hold Sharpe but no tokens or cost. Operators cannot see what a cycle consumed — which matters because cycles are token-heavy.

**Evidence.** A single real cycle on `gemini_long_gate_v3` (1 candidate, small 1-month windows) consumed, in its backtests alone:

```
5 backtests → 1,935,625 input tokens / 18,859 output tokens   (eval_runs.actual_*_tokens)
```

plus uncounted mutator + judge LLM calls (in `model_calls`). On the **default** (~20-month) window this is several-fold larger. None of it is shown to the operator.

**The data already exists** — it's just not aggregated or surfaced per cycle:
- `eval_runs.actual_input_tokens` / `actual_output_tokens` — per backtest.
- `model_calls.input_token_count` / `output_token_count` / `cost_usd` — per LLM call (cost is provider-reported and correct here, ~$0.13 for the cycle above; this is the source F11 should use).

**Blocker to clean attribution:** there is **no `cycle_id` column** on `eval_runs`, `model_calls`, or `spans`, so a cycle's runs can only be linked to it by time-window heuristic today. A clean rollup needs the cycle to record its run_ids (or stamp `cycle_id` on the eval runs / model calls it spawns).

**Acceptance.**
1. The CLI cycle summary prints both: `tokens: <in> in / <out> out` and `cost: $<x.xx>` (cost sourced from `model_calls.cost_usd` per F11; `+ N call(s) with UNKNOWN price` when unpriced).
2. `xvn optimizer inspect <cycle>` and `GET /api/autooptimizer/cycles/:id` include per-cycle token totals + cost (and ideally a per-node / per-stage breakdown: parent vs candidate vs canary vs mutator/judge).
3. The optimizer panel shows tokens + cost for each historic run.
4. Cycle-spawned eval runs / model calls are attributable to the `cycle_id` (stamp it, or persist the cycle→run_id set) so the rollup is exact, not time-window-based.

**Files.** `crates/xvision-cli/src/commands/autooptimizer.rs` (summary print), `crates/xvision-engine/src/autooptimizer/cycle_runs.rs` + dashboard `routes/autooptimizer.rs` (`:id` detail), the cycle persistence path (stamp `cycle_id` on spawned runs), frontend `features/autooptimizer/`.

---

## F24 — [HIGH] Optimization objective is hardcoded to Sharpe — make it configurable (incl. return, drawdown, Sortino, and cost/budget)

**Problem.** The mutation-cycle gate (`autooptimizer/gate.rs::evaluate`) is fixed to:
1. Δ Sharpe (day window) ≥ `min_improvement`
2. Δ Sharpe (untouched window) ≥ `min_improvement`  (stricter)
3. child worst drawdown ≤ `1.5 ×` parent worst drawdown  (guard)

Only the **threshold** (`min_improvement`) is operator-configurable, not the **metric**. `MetricsSummary` already carries `total_return_pct`, `net_return_pct`, `win_rate`, `max_drawdown_pct`, `n_trades`, etc., but none can be selected as the objective. (`--metric` exists only on the unrelated `optimizer gate` distillation verb and DSPy `optimize` — not the mutation cycle.)

This directly answers two operator asks:
- *"Can you choose which axis to improve on besides Sharpe?"* — no, not today.
- *"Does it improve on budget?"* — no; cost is neither an objective nor correctly metered (F11). **Cost/budget should be a selectable objective** (minimize cost-per-decision / tokens-per-trade subject to a return floor), but that requires F11 + F23 first (you can't optimize a number you can't measure).

**Acceptance.**
1. The cycle objective is configurable (CLI flag + `AutoOptimizerConfig`): at minimum `sharpe` (default), `total_return`, `sortino`, `max_drawdown` (minimize), `win_rate`, and a **cost/efficiency** axis once F11 lands.
2. The gate evaluates the selected metric (keeping the held-out-window discipline + a guard on the non-objective risk axis, e.g. don't let drawdown blow up while maximizing return).
3. `inspect` / panel show which objective a cycle optimized.

**Files.** `crates/xvision-engine/src/autooptimizer/gate.rs` (parameterize the metric), `config.rs` (objective field), `commands/autooptimizer.rs` (`--objective`/`--metric` flag), wire the chosen metric label through `CycleConfig` + the run-detail surfaces.

---

## F25 — [MED] No model/provider mutation axis — the researcher can't try different models

**Problem.** `MutationKind` is `Prose | Param | Tool` only. The experiment writer can change the agent's prompt, params, and tools, but **cannot mutate the strategy's trader model/provider**. So the optimizer can't explore "does this strategy do better on a cheaper/stronger model?" — often one of the highest-leverage knobs (quality vs cost trade-off, and the natural pairing with an F24 cost objective).

Note: `--provider/--model` set the *mutator/judge* model (operator infra), and the paper-test trader uses the strategy's own bound model — neither is an *optimization axis*. There is no `ModelSwap` experiment kind.

**Acceptance.**
1. Add a model/provider mutation kind: the experiment writer can propose swapping the trader slot's `(provider, model)` to another *registered* provider/model.
2. Candidates are gated like any other (same backtest + objective), so a model swap only survives if it actually improves the objective (e.g. equal Sharpe at lower cost, once F24's cost axis exists).
3. Swaps are constrained to registered/available providers (avoid the F22 unavailable-provider trap).

**Files.** `crates/xvision-engine/src/autooptimizer/mutator.rs` (`MutationKind` + `MutationDiff::apply_to` for the slot binding), the mutator prompt (`resources/prompts/autooptimizer/mutator-v1.md`), `validator.rs` (only registered providers), `inversion.rs` (treat a model swap as a real change, not symmetric noise).

---

## Summary
| # | Sev | Gap | Operator question it answers |
|---|-----|-----|------------------------------|
| F23 | High | per-cycle tokens + cost not surfaced (CLI + UI); no `cycle_id` linkage for clean rollup | "do we see how many tokens an optimizer run uses?" → not today |
| F24 | High | objective hardcoded to Sharpe; not configurable; no cost/budget axis | "choose the axis besides Sharpe?" / "improve on budget?" → no |
| F25 | Med | no model/provider mutation axis | "can the researcher switch models?" → no |

Dependencies: F24's cost objective and F23's cost display both depend on **F11** (meter realized cost from `model_calls.cost_usd`). Recommended order: F11 → F23 → F24 → F25.

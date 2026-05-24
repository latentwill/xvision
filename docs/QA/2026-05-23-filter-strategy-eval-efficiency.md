# XVN filter strategy eval efficiency findings

> QA/process review from 2026-05-23 after an agent attempted to create and compare a filtered XVN strategy against a baseline.
>
> Scope: speed and reliability of the operator workflow for creating strategy/scenario evals, especially filter-functionality tests.
>
> This document records observed workflow friction and recommended product/runbook improvements. It is not a trading-strategy endorsement.

## Summary

The agent eventually created a filtered BTC 4h breakout strategy, attached a Gemini Flash Lite 3.1 trader, ran it against a BTC 4h mixed-regime scenario, created a no-filter baseline, and compared the runs. Both runs completed but produced identical aggregate behavior, so the test did not demonstrate a filter effect.

The work took longer than necessary because the agent treated a known XVN operation as an open-ended codebase investigation. The main functional issue was a fixed strategy cadence mismatch, but several process and UX improvements would make this workflow much faster.

## Triage notes on opinionated recommendations

This document mixes hard observations with product recommendations. Treat the database/run facts as objective evidence, and treat the recommendations as proposals to be validated against the product roadmap.

Items that are more opinionated than factual:
- "Remote CLI affordances are misleading" is a workflow/product judgment. The factual part is that several remote CLI creation commands were unavailable.
- "Scenario was too clean/sparse" is test-design judgment. The factual part is that filtered and baseline runs had identical aggregate behavior.
- "Baseline/variant comparison should be first-class" is a product recommendation, not a correctness bug.
- "Comparison output needs filter-specific diagnostics" is an observability recommendation. The specific diagnostics listed are examples, not required acceptance criteria.
- "Scenario browser exposes regime-diversity and trade-density tags" is a roadmap idea.

Terminology note: "legal-action derivation" means deriving the allowed trading actions (`hold`, `flat/close`, `long_open`, `short_open`) from portfolio state and risk config. It does not refer to law, compliance, or legal advice. The clearer phrase is "allowed trading-action derivation."

## Confirmed friction

### 1) 4h scenario vs fixed 60-minute strategy cadence

**Severity:** High  
**Area:** Strategy creation / eval preflight  
**Type:** Functional / UX

**Observed behavior:**
- The selected scenario was 4h (`240 min`).
- The created strategy had `decision_cadence_minutes = 60`.
- Validation reported the strategy as not eval-ready because scenario granularity and strategy cadence did not match.
- Attempting to patch `decision_cadence_minutes` through the strategy metadata endpoint failed because that endpoint only accepts metadata fields.
- The backtest still ran despite the preflight mismatch.

**Expected behavior:**
- If users can request or select a 4h strategy/scenario, the creation surface should create a 4h-compatible strategy or clearly block the mismatch.
- Preflight should either hard-block incompatible cadence or explicitly mark the run as cadence-mismatched if execution is still allowed.

**Why it matters:**
- It is confusing for a user to ask for a 4h strategy, get a 1h strategy artifact, and still receive a completed run.
- It weakens trust in eval results and slows operator debugging.

**Recommendations:**
- Expose `decision_cadence_minutes` at strategy creation time.
- Add a first-class `timeframe`/`cadence` field in the strategy authoring UI/API.
- Add a one-click “use scenario cadence” option.
- Make eval launch behavior consistent when preflight says `eval_ready: false`.

---

### 2) Strategy/scenario creation workflow is too discovery-heavy

**Severity:** Medium  
**Area:** Operator workflow / docs / API affordances  
**Type:** Productivity

**Observed behavior:**
- The agent spent significant time reading README, manual, scenario docs, strategy docs, Rust API source, CLI source, dashboard routes, validation code, and templates before running the first useful eval.
- Much of this was only needed because the supported live-node workflow and API constraints were not obvious from a single runbook.

**Expected behavior:**
- A known operation like “create filtered strategy, attach agent, run eval, compare baseline” should have a short operator recipe.

**Recommendations:**
- Maintain a single “fast eval recipe” for strategy/filter tests:
  1. List scenarios.
  2. Filter by asset/timeframe/cadence compatibility.
  3. Pick a regime-diverse scenario.
  4. Create strategy metadata.
  5. Attach trader agent with provider/model/prompt.
  6. Run eval.
  7. Clone/create baseline.
  8. Run baseline on the same scenario.
  9. Compare aggregate metrics and decision divergence.
- Document which operations are API-first versus remote-CLI-first.
- Document known API limitations near the recipe, especially metadata-only strategy patching.

---

### 3) Remote CLI affordances are misleading for creation tasks

**Severity:** Medium  
**Area:** Remote CLI / operator UX  
**Type:** Discoverability

**Observed behavior:**
- Attempts to use remote CLI subcommands such as `strategy create`, `strategy new`, and `scenario create` failed because those subcommands are not allowed on the remote CLI surface.
- The agent then had to switch to direct API calls.

**Expected behavior:**
- Operators should be directed to the supported surface immediately.
- If a command is unavailable remotely, error messages should point to the supported API/UI path.

**Recommendations:**
- Add remote CLI errors like: “This subcommand is not allowed remotely. Use POST /api/strategies plus POST /api/agents and attach via POST /api/strategy/:id/agents.”
- Add a lightweight `xvn remote recipe strategy-eval` or docs page showing the allowed path.
- Consider exposing safe remote creation commands if they are intended operator workflows.

---

### 4) Filter functionality test used a scenario that was too clean/sparse

**Severity:** Medium  
**Area:** Scenario selection / eval design  
**Type:** Test quality

**Observed behavior:**
- Filtered and unfiltered runs produced identical aggregate metrics and action counts.
- Both effectively opened one long and then mostly held/flat.
- This does not prove the filter is useless; it indicates the scenario did not exercise the gate.

**Expected behavior:**
- A filter-functionality test should include enough marginal conditions for the filtered and baseline arms to diverge.

**Recommendations:**
- Tag scenarios with regime features such as `trend`, `chop`, `false-breakout`, `volatility-expansion`, `volatility-compression`, and `high-trade-density`.
- Provide a default “filter stress test” scenario that includes obvious chop, false breakouts, at least one clean breakout, and enough post-warmup decision points.
- Prefer cadence-compatible 1h scenarios until 4h strategy cadence creation is fixed.

---

### 5) Baseline/variant comparison should be first-class

**Severity:** Medium  
**Area:** Eval UX / strategy workflow  
**Type:** Productivity / analysis

**Observed behavior:**
- The agent manually created a filtered strategy and separate baseline, then compared aggregate outputs.
- There was no obvious first-class “clone as baseline/variant” workflow.

**Expected behavior:**
- Testing a filter should make it easy to create A/B arms where only the filter instruction differs.

**Recommendations:**
- Add a “Clone as baseline” or “Create variant” action.
- Preserve shared fields: model, provider, asset universe, risk preset, scenario, and base trading prompt.
- Make the filter instruction the only intentional delta.
- Store an explicit comparison group/run set for the two evals.

---

### 6) Comparison output needs filter-specific diagnostics

**Severity:** Medium  
**Area:** Eval results / diagnostics  
**Type:** Observability

**Observed behavior:**
- Aggregate metrics were enough to detect identical behavior, but not enough to quickly explain filter effectiveness.

**Expected behavior:**
- Filter tests should expose whether the filter changed decisions and why.

**Recommended diagnostics:**
- Action divergence count between baseline and filtered arms.
- First divergence timestamp.
- Count of trades blocked by the filter.
- Hypothetical PnL of blocked baseline trades, if available.
- Regime classification histogram.
- Examples of blocked trades with rationale.
- Baseline trades taken during chop.
- Allowed-trade win rate versus baseline win rate.

---

### 7) Reviewed run did not exercise the real XVN filter system

**Severity:** High  
**Area:** Filter authoring / eval execution / results interpretation  
**Type:** Functional / observability

**Observed behavior:**
- The reviewed filtered run was `01KSAFTFKVXPMCRAD1MS3NGSYE` using strategy `01KSAFKB4AACMXG4FFYVPPPSWT`.
- The comparison baseline was `01KSAFYWCAB2CPSWXCHND1ER3E`.
- The run response reported empty `filter_events` and empty `filter_summaries`.
- Server DB inspection showed `filters` count `0` and `eval_filter_evaluations` count `0`.
- The strategy JSON had `mechanical_params: {}` and no attached filter artifact.
- The only filter-like behavior was prompt text in the trader agent that mentioned a strict regime filter.

**Expected behavior:**
- A filter-functionality test should create or attach a real XVN filter artifact.
- Filtered evals should emit filter evaluation rows/events so the operator can see pass/block decisions.
- The dashboard should clearly distinguish “prompt mentions a filter” from “XVN filter system evaluated a filter.”

**Why it matters:**
- The reviewed run cannot validate filter functionality because the filter subsystem did not participate.
- Empty filter sections in the strategy UI can make the run look like a filtered strategy while the backend records no filter evaluations.

**Recommendations:**
- Block or warn when launching a “filtered” comparison if no filter artifact is attached.
- Show filter status explicitly in the strategy/eval header: `No filter attached`, `Prompt-only filter language`, or `Filter attached and evaluated`.
- Add a preflight check requiring non-empty filter definition when the eval objective is a filter test.
- Make the UI/API path for creating and attaching filters first-class; do not rely on agent prompt wording.

---

### 8) Open-position noop skip clamps trader behavior after first entry

**Severity:** High  
**Area:** Backtest executor / guardrails / portfolio semantics  
**Type:** Functional correctness

**Observed behavior:**
- After one `long_open` at decision index `4`, the executor synthesized `hold` decisions with:
  - `noop_skip: portfolio already carries a position — only hold is legal`
- This happened `728` times in the reviewed run.
- Supervisor notes recorded `trader-noop-skip fired ... portfolio already carries a position; the LLM call was skipped and a hold decision was synthesized`.
- The strategy risk config allowed `max_concurrent_positions: 2`, and XVN has backend multi-asset potential, even if multi-asset is not yet fully surfaced in the UI.

**Expected behavior:**
- Carrying a position should not make only `hold` legal.
- The trader or risk layer should still be able to close, reduce, reverse, rebalance, or open another allowed asset/position when configured.
- At minimum, single-asset mode should allow exit/sell decisions while a position is open.

**Why it matters:**
- Once the early long opened, the eval mostly stopped asking the trader for real decisions.
- This makes filtered and unfiltered runs converge and hides whether a strategy can exit correctly.
- The message is also product-inaccurate for the intended multi-asset / sell-capable backend model.

**Recommendations:**
- Replace the coarse `portfolio already carries a position` noop rule with allowed trading-action derivation from portfolio, asset universe, position limits, and risk config.
- Allow at least exit/reduce/reverse actions while a position exists.
- If a skip optimization remains, restrict it to cases where no state-changing trading action is available and explain the exact reason.
- Surface a guardrail summary warning when most decisions are synthesized rather than model/risk decisions.

---

### 9) Early-stop policy generated many synthetic flat rows

**Severity:** Medium  
**Area:** Eval executor / cost guardrails / explainability  
**Type:** Observability / test quality

**Observed behavior:**
- Server logs repeatedly showed `early-stop policy fired — inheriting flat decisions` for run `01KSAFTFKVXPMCRAD1MS3NGSYE`.
- DB notes explained the policy as `early-stop: 8 low-conviction flats; skipping 4 bars`.
- The run contained `360` decisions with `justification = "inherited from early-stop policy"`.
- Combined with the noop skip, roughly 75% of decisions were rewritten or synthesized according to the guardrail summary.

**Expected behavior:**
- Early-stop can be useful for cost control, but eval results should make synthetic rows visually and analytically distinct from real trader decisions.
- Filter tests should probably disable or clearly annotate early-stop because skipped bars hide filter behavior.

**Why it matters:**
- Operators cannot infer strategy or filter behavior from rows that were never evaluated by the model/filter.
- Synthetic rows make headline metrics look cleaner than the underlying decision process.

**Recommendations:**
- Add result-level counts for model decisions, filter decisions, noop skips, early-stop inherited rows, and guardrail rewrites.
- Add an eval option to disable early-stop for QA/filter-functionality tests.
- Exclude synthetic rows from filter-effectiveness metrics unless explicitly requested.

---

### 10) Event recorder emitted a duplicate agent-run warning

**Severity:** Low  
**Area:** Observability / event recorder  
**Type:** Data integrity warning

**Observed behavior:**
- Server logs around the eval included:
  - `recorder failed to handle event error=sqlite: UNIQUE constraint failed: agent_runs.id`

**Expected behavior:**
- Event recording should either be idempotent for repeated agent-run events or emit enough context to identify the duplicate source.

**Why it matters:**
- This did not appear to cause the eval behavior above, but it indicates a separate observability integrity issue.

**Recommendations:**
- Make agent-run event inserts idempotent or include conflict handling.
- Attach run id / agent run id context to the warning for faster diagnosis.

---

## Faster workflow for the next run

Use this when the goal is to test filter functionality, not to research the best possible trading edge:

1. Start API-first on the live node; do not begin with source spelunking.
2. Pick a cadence-compatible scenario before creating any strategy.
3. Prefer a 1h mixed-regime scenario while strategy creation defaults to 60-minute cadence.
4. Create or attach a real XVN filter artifact; do not treat filter wording in the trader prompt as proof that filter functionality is active.
5. Confirm preflight/reporting shows non-empty filter definitions and that `filter_events` / `filter_summaries` are expected to populate.
6. Create two nearly identical prompts:
   - Baseline: breakout/trend-following trader.
   - Filtered: same trader plus the actual filter gate.
7. Use the same provider/model for both arms.
8. Attach trader agents before eval launch.
9. For QA/filter-functionality tests, disable or clearly account for early-stop/noop-skip optimizations if they would suppress real decisions.
10. Run both evals on the same scenario.
11. Compare aggregate metrics plus decision divergence, filter evaluations, and synthesized-decision counts, not return alone.
12. Only read repo source if the API response is unexplained or blocked.

## Product fixes that would reduce future operator time

- Strategy creation accepts cadence/timeframe and validates it against selected scenario.
- Eval launch blocks or clearly annotates cadence mismatches.
- Remote CLI errors point to the supported API/UI path.
- Scenario browser exposes regime-diversity and trade-density tags.
- One-click baseline/variant creation for A/B evals.
- Eval comparison page shows filter-effectiveness diagnostics.
- Strategy/eval UI explicitly shows whether a real filter artifact is attached and evaluated.
- Backtest executor allowed-action logic supports exit/sell/reduce/reverse decisions while a position is open.
- Eval results separate real model/filter decisions from noop-skip, early-stop, and other synthesized rows.
- A documented “filter stress test” scenario exists and is kept cadence-compatible with the default strategy authoring path.

## Result from the reviewed run

- Filter strategy was created and attached to a Gemini Flash Lite 3.1 trader.
- Baseline strategy was created with the same model.
- Both backtests completed on the same BTC 4h scenario.
- Both runs produced identical headline metrics and action counts.
- Follow-up inspection showed the filtered run had no real XVN filter artifact: `filter_events` and `filter_summaries` were empty, and the server had no `filters` / `eval_filter_evaluations` rows.
- The run opened one long at decision index `4`; after that, `728` decisions were synthesized by `noop_skip` because the portfolio carried a position.
- Another `360` rows were synthetic `inherited from early-stop policy` decisions.
- The result is best interpreted as an invalid filter-functionality test plus executor guardrails clamping behavior, not evidence that filtering has no value.
